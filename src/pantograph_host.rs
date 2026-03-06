use emily::error::EmilyError;
use emily::inference::{
    EmbeddingProvider, PantographEmbeddingProvider, PantographWorkflowBinding,
    PantographWorkflowEmbeddingConfig, PantographWorkflowServiceClient,
};
use emily::model::EmbeddingProviderStatus;
use inference::{BackendConfig, InferenceGateway, StdProcessSpawner};
use pantograph_workflow_service::{
    WorkflowHost, WorkflowIoRequest, WorkflowOutputTarget, WorkflowPortBinding, WorkflowRunOptions,
    WorkflowRunRequest, WorkflowRunResponse, WorkflowService, WorkflowServiceError, capabilities,
};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::watch;
use uuid::Uuid;

// Ensure workflow node descriptors are linked into this binary.
use workflow_nodes as _;

const DEFAULT_TIMEOUT_MS: u64 = 120_000;
const DEFAULT_EMBED_DIMENSIONS: usize = 1024;
const EMBEDDING_MODEL_INPUT_ALIASES: [&str; 6] = [
    "model",
    "model_path",
    "modelName",
    "model_name",
    "modelId",
    "model_id",
];

#[derive(Debug, Clone, PartialEq, Eq)]
enum DeferredProviderState {
    Bootstrapping,
    Ready,
    Failed(String),
}

struct DeferredEmbeddingProvider {
    provider: Arc<Mutex<Option<Arc<dyn EmbeddingProvider>>>>,
    state_rx: watch::Receiver<DeferredProviderState>,
}

impl DeferredEmbeddingProvider {
    fn spawn<F>(bootstrap: F) -> Arc<dyn EmbeddingProvider>
    where
        F: FnOnce() -> Result<Arc<dyn EmbeddingProvider>, String> + Send + 'static,
    {
        let provider = Arc::new(Mutex::new(None));
        let (state_tx, state_rx) = watch::channel(DeferredProviderState::Bootstrapping);
        let worker_provider = Arc::clone(&provider);

        std::thread::spawn(move || match bootstrap() {
            Ok(ready_provider) => {
                if let Ok(mut slot) = worker_provider.lock() {
                    *slot = Some(ready_provider);
                }
                let _ = state_tx.send(DeferredProviderState::Ready);
            }
            Err(error) => {
                let _ = state_tx.send(DeferredProviderState::Failed(error));
            }
        });

        Arc::new(Self { provider, state_rx })
    }

    fn provider_clone(&self) -> Result<Option<Arc<dyn EmbeddingProvider>>, EmilyError> {
        self.provider
            .lock()
            .map(|guard| guard.clone())
            .map_err(|_| EmilyError::Embedding("deferred provider lock poisoned".to_string()))
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for DeferredEmbeddingProvider {
    async fn embed_text(&self, text: &str) -> Result<Vec<f32>, EmilyError> {
        let mut state_rx = self.state_rx.clone();
        loop {
            let state = state_rx.borrow().clone();
            match state {
                DeferredProviderState::Bootstrapping => {
                    state_rx.changed().await.map_err(|_| {
                        EmilyError::Embedding(
                            "deferred provider bootstrap channel closed".to_string(),
                        )
                    })?;
                }
                DeferredProviderState::Ready => {
                    let provider = self.provider_clone()?.ok_or_else(|| {
                        EmilyError::Embedding(
                            "deferred provider reported ready without provider".to_string(),
                        )
                    })?;
                    return provider.embed_text(text).await;
                }
                DeferredProviderState::Failed(error) => {
                    return Err(EmilyError::Embedding(error));
                }
            }
        }
    }

    async fn status(&self) -> Option<EmbeddingProviderStatus> {
        let state = self.state_rx.borrow().clone();
        match state {
            DeferredProviderState::Bootstrapping => Some(EmbeddingProviderStatus {
                state: "bootstrapping".to_string(),
                session_id: None,
                queued_runs: None,
                queue_items: None,
                keep_alive: None,
                last_error: None,
            }),
            DeferredProviderState::Ready => {
                let provider = self.provider_clone().ok().flatten();
                match provider {
                    Some(provider) => provider.status().await.or(Some(EmbeddingProviderStatus {
                        state: "ready".to_string(),
                        session_id: None,
                        queued_runs: None,
                        queue_items: None,
                        keep_alive: None,
                        last_error: None,
                    })),
                    None => Some(EmbeddingProviderStatus {
                        state: "ready".to_string(),
                        session_id: None,
                        queued_runs: None,
                        queue_items: None,
                        keep_alive: None,
                        last_error: None,
                    }),
                }
            }
            DeferredProviderState::Failed(error) => Some(EmbeddingProviderStatus {
                state: "unavailable".to_string(),
                session_id: None,
                queued_runs: None,
                queue_items: None,
                keep_alive: None,
                last_error: Some(error),
            }),
        }
    }

    async fn shutdown(&self) -> Result<(), EmilyError> {
        if let Some(provider) = self.provider_clone()? {
            provider.shutdown().await?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct PantographRuntimeConfig {
    pub pantograph_root: PathBuf,
    pub workflow_id: String,
    pub timeout_ms: Option<u64>,
    pub expected_dimensions: usize,
    pub text_input_node_id: Option<String>,
    pub text_input_port_id: Option<String>,
    pub vector_output_node_id: Option<String>,
    pub vector_output_port_id: Option<String>,
    pub model_path_override: Option<PathBuf>,
}

impl PantographRuntimeConfig {
    pub fn from_env() -> Result<Self, String> {
        let pantograph_root = std::env::var("GESTALT_PANTOGRAPH_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                PathBuf::from("/media/jeremy/OrangeCream/Linux Software/Pantograph")
            });

        let workflow_id = std::env::var("GESTALT_PANTOGRAPH_WORKFLOW_ID")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "Embedding".to_string());

        let timeout_ms = std::env::var("GESTALT_PANTOGRAPH_WORKFLOW_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .or(Some(DEFAULT_TIMEOUT_MS));

        let expected_dimensions = std::env::var("GESTALT_EMBEDDING_DIMENSIONS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(DEFAULT_EMBED_DIMENSIONS);

        let text_input_node_id = std::env::var("GESTALT_PANTOGRAPH_TEXT_NODE_ID")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let text_input_port_id = std::env::var("GESTALT_PANTOGRAPH_TEXT_PORT_ID")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        let vector_output_node_id = std::env::var("GESTALT_PANTOGRAPH_VECTOR_NODE_ID")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let vector_output_port_id = std::env::var("GESTALT_PANTOGRAPH_VECTOR_PORT_ID")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        let model_path_override = std::env::var("GESTALT_PANTOGRAPH_EMBED_MODEL_PATH")
            .ok()
            .map(PathBuf::from);

        if !pantograph_root.exists() {
            return Err(format!(
                "pantograph root does not exist: {}",
                pantograph_root.display()
            ));
        }

        Ok(Self {
            pantograph_root,
            workflow_id,
            timeout_ms,
            expected_dimensions,
            text_input_node_id,
            text_input_port_id,
            vector_output_node_id,
            vector_output_port_id,
            model_path_override,
        })
    }

    fn workflow_roots(&self) -> Vec<PathBuf> {
        vec![
            self.pantograph_root.join(".pantograph/workflows"),
            self.pantograph_root.join("src/templates/workflows"),
        ]
    }

    fn project_root(&self) -> PathBuf {
        self.pantograph_root.clone()
    }

    fn binaries_dir(&self) -> PathBuf {
        self.pantograph_root.join("src-tauri/binaries")
    }

    fn data_dir(&self) -> PathBuf {
        self.pantograph_root.join("launcher-data")
    }
}

#[derive(Debug, Default)]
struct HostRuntimeState {
    active_model_path: Option<PathBuf>,
}

fn resolve_embedding_model_path_from_inputs(
    inputs: &HashMap<String, serde_json::Value>,
    model_path_override: Option<&Path>,
) -> Result<PathBuf, node_engine::NodeEngineError> {
    if let Some(model_path) = model_path_override {
        return Ok(model_path.to_path_buf());
    }

    let value = EMBEDDING_MODEL_INPUT_ALIASES
        .iter()
        .find_map(|key| {
            inputs
                .get(*key)
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
        .or_else(|| {
            EMBEDDING_MODEL_INPUT_ALIASES.iter().find_map(|key| {
                inputs
                    .get("_data")
                    .and_then(|value| value.get(*key))
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
            })
        });

    value.map(PathBuf::from).ok_or_else(|| {
        node_engine::NodeEngineError::ExecutionFailed(
            "embedding workflow is missing a resolved model input".to_string(),
        )
    })
}

struct EmbeddingWorkflowTaskExecutor {
    config: PantographRuntimeConfig,
    gateway: Arc<InferenceGateway>,
    runtime_state: Arc<Mutex<HostRuntimeState>>,
    core: Arc<node_engine::CoreTaskExecutor>,
}

impl EmbeddingWorkflowTaskExecutor {
    fn new(
        config: PantographRuntimeConfig,
        gateway: Arc<InferenceGateway>,
        runtime_state: Arc<Mutex<HostRuntimeState>>,
        core: Arc<node_engine::CoreTaskExecutor>,
    ) -> Self {
        Self {
            config,
            gateway,
            runtime_state,
            core,
        }
    }

    fn resolve_model_path_from_inputs(
        &self,
        inputs: &HashMap<String, serde_json::Value>,
    ) -> Result<PathBuf, node_engine::NodeEngineError> {
        resolve_embedding_model_path_from_inputs(inputs, self.config.model_path_override.as_deref())
    }

    async fn ensure_gateway_started(
        &self,
        model_path: &Path,
    ) -> Result<(), node_engine::NodeEngineError> {
        let should_restart = {
            let state = self.runtime_state.lock().map_err(|_| {
                node_engine::NodeEngineError::ExecutionFailed(
                    "host runtime state lock poisoned".to_string(),
                )
            })?;
            let same_model = state.active_model_path.as_deref() == Some(model_path);
            !same_model
        } || !self.gateway.is_ready().await
            || !self.gateway.is_embedding_mode().await;

        if !should_restart {
            return Ok(());
        }

        self.gateway.stop().await;
        let spawner = Arc::new(StdProcessSpawner::new(
            self.config.binaries_dir(),
            self.config.data_dir(),
        ));
        self.gateway.set_spawner(spawner).await;

        let backend_config = BackendConfig {
            model_path: Some(model_path.to_path_buf()),
            embedding_mode: true,
            ..BackendConfig::default()
        };

        self.gateway.start(&backend_config).await.map_err(|error| {
            node_engine::NodeEngineError::ExecutionFailed(format!(
                "failed to start pantograph embedding gateway: {error}"
            ))
        })?;

        let mut state = self.runtime_state.lock().map_err(|_| {
            node_engine::NodeEngineError::ExecutionFailed(
                "host runtime state lock poisoned".to_string(),
            )
        })?;
        state.active_model_path = Some(model_path.to_path_buf());
        Ok(())
    }
}

#[async_trait::async_trait]
impl node_engine::TaskExecutor for EmbeddingWorkflowTaskExecutor {
    async fn execute_task(
        &self,
        task_id: &str,
        inputs: HashMap<String, serde_json::Value>,
        context: &node_engine::Context,
        extensions: &node_engine::ExecutorExtensions,
    ) -> node_engine::Result<HashMap<String, serde_json::Value>> {
        let node_type = node_engine::resolve_node_type(task_id, &inputs);
        if node_type != "embedding" {
            return Err(node_engine::NodeEngineError::ExecutionFailed(format!(
                "Node type '{}' requires host-specific executor",
                node_type
            )));
        }

        let model_path = self.resolve_model_path_from_inputs(&inputs)?;
        self.ensure_gateway_started(&model_path).await?;

        node_engine::TaskExecutor::execute_task(
            self.core.as_ref(),
            task_id,
            inputs,
            context,
            extensions,
        )
        .await
    }
}

pub struct GestaltPantographHost {
    config: PantographRuntimeConfig,
    gateway: Arc<InferenceGateway>,
    runtime_state: Arc<Mutex<HostRuntimeState>>,
}

impl GestaltPantographHost {
    pub fn new(config: PantographRuntimeConfig) -> Result<Self, String> {
        let binaries_dir = config.binaries_dir();
        if !binaries_dir.exists() {
            return Err(format!(
                "pantograph binaries directory does not exist: {}",
                binaries_dir.display()
            ));
        }
        Ok(Self {
            config,
            gateway: Arc::new(InferenceGateway::new()),
            runtime_state: Arc::new(Mutex::new(HostRuntimeState::default())),
        })
    }

    fn apply_input_bindings(
        graph: &mut node_engine::WorkflowGraph,
        inputs: &[WorkflowPortBinding],
    ) -> Result<(), WorkflowServiceError> {
        for binding in inputs {
            let node = graph
                .nodes
                .iter_mut()
                .find(|node| node.id == binding.node_id)
                .ok_or_else(|| {
                    WorkflowServiceError::InvalidRequest(format!(
                        "input binding references unknown node_id '{}'",
                        binding.node_id
                    ))
                })?;

            if node.data.is_null() {
                node.data = serde_json::json!({});
            }

            let map = node.data.as_object_mut().ok_or_else(|| {
                WorkflowServiceError::InvalidRequest(format!(
                    "input node '{}' has non-object data payload",
                    binding.node_id
                ))
            })?;
            map.insert(binding.port_id.clone(), binding.value.clone());
        }

        Ok(())
    }

    fn resolve_output_node_ids(
        graph: &node_engine::WorkflowGraph,
        output_targets: Option<&[WorkflowOutputTarget]>,
    ) -> Result<Vec<String>, WorkflowServiceError> {
        if let Some(targets) = output_targets {
            if targets.is_empty() {
                return Err(WorkflowServiceError::InvalidRequest(
                    "output_targets cannot be empty when provided".to_string(),
                ));
            }

            let known_nodes = graph
                .nodes
                .iter()
                .map(|node| node.id.as_str())
                .collect::<HashSet<_>>();

            let mut dedup = HashSet::<String>::new();
            let mut node_ids = Vec::<String>::new();
            for target in targets {
                if !known_nodes.contains(target.node_id.as_str()) {
                    return Err(WorkflowServiceError::InvalidRequest(format!(
                        "output target references unknown node_id '{}'",
                        target.node_id
                    )));
                }
                if dedup.insert(target.node_id.clone()) {
                    node_ids.push(target.node_id.clone());
                }
            }
            return Ok(node_ids);
        }

        let output_node_ids = graph
            .nodes
            .iter()
            .filter(|node| node.node_type.ends_with("-output"))
            .map(|node| node.id.clone())
            .collect::<Vec<_>>();

        if output_node_ids.is_empty() {
            return Err(WorkflowServiceError::InvalidRequest(
                "workflow has no output nodes; add explicit *-output nodes or provide output_targets"
                    .to_string(),
            ));
        }

        Ok(output_node_ids)
    }

    fn collect_run_outputs(
        node_outputs: &HashMap<String, HashMap<String, serde_json::Value>>,
        output_node_ids: &[String],
        output_targets: Option<&[WorkflowOutputTarget]>,
    ) -> Vec<WorkflowPortBinding> {
        if let Some(targets) = output_targets {
            let mut outputs = Vec::with_capacity(targets.len());
            for target in targets {
                let Some(value) = node_outputs
                    .get(&target.node_id)
                    .and_then(|ports| ports.get(&target.port_id))
                    .cloned()
                else {
                    continue;
                };
                outputs.push(WorkflowPortBinding {
                    node_id: target.node_id.clone(),
                    port_id: target.port_id.clone(),
                    value,
                });
            }
            return outputs;
        }

        let mut outputs = Vec::<WorkflowPortBinding>::new();
        for node_id in output_node_ids {
            let Some(ports) = node_outputs.get(node_id) else {
                continue;
            };
            let mut keys = ports.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            for port_id in keys {
                if let Some(value) = ports.get(&port_id) {
                    outputs.push(WorkflowPortBinding {
                        node_id: node_id.clone(),
                        port_id,
                        value: value.clone(),
                    });
                }
            }
        }

        outputs
    }
}

#[async_trait::async_trait]
impl WorkflowHost for GestaltPantographHost {
    fn workflow_roots(&self) -> Vec<PathBuf> {
        self.config.workflow_roots()
    }

    async fn default_backend_name(&self) -> Result<String, WorkflowServiceError> {
        Ok(self.gateway.current_backend_name().await)
    }

    async fn run_workflow(
        &self,
        workflow_id: &str,
        inputs: &[WorkflowPortBinding],
        output_targets: Option<&[WorkflowOutputTarget]>,
        _run_options: WorkflowRunOptions,
        run_handle: pantograph_workflow_service::WorkflowRunHandle,
    ) -> Result<Vec<WorkflowPortBinding>, WorkflowServiceError> {
        if run_handle.is_cancelled() {
            return Err(WorkflowServiceError::RuntimeTimeout(
                "workflow run cancelled before execution started".to_string(),
            ));
        }

        let stored = capabilities::load_and_validate_workflow(workflow_id, &self.workflow_roots())?;
        let mut graph = stored.to_workflow_graph(workflow_id);
        Self::apply_input_bindings(&mut graph, inputs)?;

        let output_node_ids = Self::resolve_output_node_ids(&graph, output_targets)?;
        let execution_id = Uuid::new_v4().to_string();
        let core = Arc::new(
            node_engine::CoreTaskExecutor::new()
                .with_project_root(self.config.project_root())
                .with_gateway(self.gateway.clone())
                .with_execution_id(execution_id.clone()),
        );
        let host_executor = Arc::new(EmbeddingWorkflowTaskExecutor::new(
            self.config.clone(),
            self.gateway.clone(),
            self.runtime_state.clone(),
            core.clone(),
        ));
        let task_executor = node_engine::CompositeTaskExecutor::new(Some(host_executor), core);

        let executor = node_engine::WorkflowExecutor::new(
            execution_id,
            graph,
            Arc::new(node_engine::NullEventSink),
        );

        let mut node_outputs = HashMap::new();
        for node_id in &output_node_ids {
            if run_handle.is_cancelled() {
                return Err(WorkflowServiceError::RuntimeTimeout(
                    "workflow run cancelled during execution".to_string(),
                ));
            }
            let outputs = executor
                .demand(node_id, &task_executor)
                .await
                .map_err(|error| WorkflowServiceError::Internal(error.to_string()))?;
            node_outputs.insert(node_id.clone(), outputs);
        }

        Ok(Self::collect_run_outputs(
            &node_outputs,
            &output_node_ids,
            output_targets,
        ))
    }
}

fn binding_from_io(
    io: &pantograph_workflow_service::WorkflowIoResponse,
    config: &PantographRuntimeConfig,
) -> Result<(PantographWorkflowBinding, PantographWorkflowBinding), String> {
    let input_binding = if let (Some(node_id), Some(port_id)) = (
        config.text_input_node_id.as_ref(),
        config.text_input_port_id.as_ref(),
    ) {
        PantographWorkflowBinding::new(node_id.clone(), port_id.clone())
            .map_err(|error| error.to_string())?
    } else {
        let node = io
            .inputs
            .iter()
            .find(|node| node.node_type == "text-input")
            .or_else(|| io.inputs.first())
            .ok_or_else(|| "workflow_get_io returned no input nodes".to_string())?;
        let port = node
            .ports
            .iter()
            .find(|port| port.port_id == "text")
            .or_else(|| node.ports.first())
            .ok_or_else(|| "selected input node has no bindable ports".to_string())?;
        PantographWorkflowBinding::new(node.node_id.clone(), port.port_id.clone())
            .map_err(|error| error.to_string())?
    };

    let output_binding = if let (Some(node_id), Some(port_id)) = (
        config.vector_output_node_id.as_ref(),
        config.vector_output_port_id.as_ref(),
    ) {
        PantographWorkflowBinding::new(node_id.clone(), port_id.clone())
            .map_err(|error| error.to_string())?
    } else {
        let node = io
            .outputs
            .iter()
            .find(|node| node.node_type == "vector-output")
            .or_else(|| io.outputs.first())
            .ok_or_else(|| "workflow_get_io returned no output nodes".to_string())?;
        let port = node
            .ports
            .iter()
            .find(|port| port.port_id == "vector")
            .or_else(|| node.ports.first())
            .ok_or_else(|| "selected output node has no bindable ports".to_string())?;
        PantographWorkflowBinding::new(node.node_id.clone(), port.port_id.clone())
            .map_err(|error| error.to_string())?
    };

    Ok((input_binding, output_binding))
}

fn bootstrap_embedding_provider(
    config: PantographRuntimeConfig,
) -> Result<Arc<dyn EmbeddingProvider>, String> {
    let host = Arc::new(GestaltPantographHost::new(config.clone())?);
    run_bootstrap_blocking(async move {
        let workflow_service = WorkflowService::new();
        let io = workflow_service
            .workflow_get_io(
                host.as_ref(),
                WorkflowIoRequest {
                    workflow_id: config.workflow_id.clone(),
                },
            )
            .await
            .map_err(|error| format!("workflow_get_io bootstrap failed: {error}"))?;

        let (text_input, vector_output) = binding_from_io(&io, &config)?;

        let embedding_config = PantographWorkflowEmbeddingConfig::new(
            config.workflow_id,
            text_input,
            vector_output,
            config.timeout_ms,
            config.expected_dimensions,
        )
        .map_err(|error| error.to_string())?;

        let client = Arc::new(PantographWorkflowServiceClient::new(host));
        let provider = Arc::new(
            PantographEmbeddingProvider::new(client, embedding_config)
                .map_err(|error| error.to_string())?,
        );
        provider
            .validate()
            .await
            .map_err(|error| format!("pantograph provider validation failed: {error}"))?;

        let provider: Arc<dyn EmbeddingProvider> = provider;
        Ok(provider)
    })
}

pub fn build_deferred_embedding_provider_from_env() -> Result<Arc<dyn EmbeddingProvider>, String> {
    let config = PantographRuntimeConfig::from_env()?;
    Ok(DeferredEmbeddingProvider::spawn(move || {
        bootstrap_embedding_provider(config)
    }))
}

fn run_bootstrap_blocking<F, T>(future: F) -> Result<T, String>
where
    F: Future<Output = Result<T, String>> + Send + 'static,
    T: Send + 'static,
{
    if tokio::runtime::Handle::try_current().is_ok() {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| format!("failed creating pantograph bootstrap runtime: {error}"))
                .and_then(|runtime| runtime.block_on(future));
            let _ = tx.send(result);
        });
        return rx
            .recv()
            .map_err(|error| format!("failed waiting for pantograph bootstrap: {error}"))?;
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("failed creating pantograph bootstrap runtime: {error}"))?;
    runtime.block_on(future)
}

#[allow(dead_code)]
pub async fn run_workflow_once_from_env(text: &str) -> Result<WorkflowRunResponse, String> {
    let config = PantographRuntimeConfig::from_env()?;
    let host = GestaltPantographHost::new(config.clone())?;
    let service = WorkflowService::new();

    service
        .workflow_run(
            &host,
            WorkflowRunRequest {
                workflow_id: config.workflow_id,
                inputs: vec![WorkflowPortBinding {
                    node_id: config
                        .text_input_node_id
                        .unwrap_or_else(|| "text-input-1".to_string()),
                    port_id: config
                        .text_input_port_id
                        .unwrap_or_else(|| "text".to_string()),
                    value: serde_json::json!(text),
                }],
                output_targets: None,
                timeout_ms: config.timeout_ms,
                run_id: None,
            },
        )
        .await
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::DeferredEmbeddingProvider;
    use emily::inference::EmbeddingProvider;
    use emily::model::EmbeddingProviderStatus;
    use std::sync::Arc;
    use std::time::Duration;

    #[derive(Default)]
    struct TestProvider;

    #[async_trait::async_trait]
    impl EmbeddingProvider for TestProvider {
        async fn embed_text(&self, text: &str) -> Result<Vec<f32>, emily::error::EmilyError> {
            Ok(vec![text.len() as f32])
        }

        async fn status(&self) -> Option<EmbeddingProviderStatus> {
            Some(EmbeddingProviderStatus {
                state: "ready".to_string(),
                session_id: None,
                queued_runs: None,
                queue_items: None,
                keep_alive: Some(true),
                last_error: None,
            })
        }
    }

    #[test]
    fn deferred_provider_reports_bootstrapping_then_failure() {
        let provider = DeferredEmbeddingProvider::spawn(|| {
            std::thread::sleep(Duration::from_millis(20));
            Err("bootstrap failed".to_string())
        });
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        let initial = runtime
            .block_on(provider.status())
            .expect("initial status should be present");
        assert_eq!(initial.state, "bootstrapping");

        std::thread::sleep(Duration::from_millis(40));
        let failed = runtime
            .block_on(provider.status())
            .expect("failed status should be present");
        assert_eq!(failed.state, "unavailable");
        assert_eq!(failed.last_error.as_deref(), Some("bootstrap failed"));
    }

    #[test]
    fn deferred_provider_waits_for_bootstrap_before_embedding() {
        let provider = DeferredEmbeddingProvider::spawn(|| {
            std::thread::sleep(Duration::from_millis(20));
            let provider: Arc<dyn EmbeddingProvider> = Arc::new(TestProvider);
            Ok(provider)
        });
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        let vector = runtime
            .block_on(provider.embed_text("hello"))
            .expect("embed should wait for readiness");
        assert_eq!(vector, vec![5.0]);
    }
}
