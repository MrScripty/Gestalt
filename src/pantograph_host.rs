use emily::inference::{
    EmbeddingProvider, PantographEmbeddingProvider, PantographWorkflowBinding,
    PantographWorkflowEmbeddingConfig, PantographWorkflowServiceClient,
};
use inference::{BackendConfig, InferenceGateway, StdProcessSpawner};
use pantograph_workflow_service::{
    WorkflowHost, WorkflowIoRequest, WorkflowOutputTarget, WorkflowPortBinding, WorkflowRunOptions,
    WorkflowRunRequest, WorkflowRunResponse, WorkflowService, WorkflowServiceError, capabilities,
};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

// Ensure workflow node descriptors are linked into this binary.
use workflow_nodes as _;

const DEFAULT_TIMEOUT_MS: u64 = 120_000;
const DEFAULT_EMBED_DIMENSIONS: usize = 1024;

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

pub struct GestaltPantographHost {
    config: PantographRuntimeConfig,
    gateway: Arc<InferenceGateway>,
    runtime_state: Mutex<HostRuntimeState>,
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
            runtime_state: Mutex::new(HostRuntimeState::default()),
        })
    }

    async fn ensure_gateway_started(&self, model_path: &Path) -> Result<(), WorkflowServiceError> {
        let should_restart = {
            let state = self.runtime_state.lock().map_err(|_| {
                WorkflowServiceError::Internal("host runtime state lock poisoned".to_string())
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
            WorkflowServiceError::RuntimeNotReady(format!(
                "failed to start pantograph embedding gateway: {error}"
            ))
        })?;

        let mut state = self.runtime_state.lock().map_err(|_| {
            WorkflowServiceError::Internal("host runtime state lock poisoned".to_string())
        })?;
        state.active_model_path = Some(model_path.to_path_buf());
        Ok(())
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

    fn detect_model_path(
        &self,
        graph: &node_engine::WorkflowGraph,
    ) -> Result<PathBuf, WorkflowServiceError> {
        if let Some(model_path) = self.config.model_path_override.clone() {
            return Ok(model_path);
        }

        let embedding_model_path = graph
            .nodes
            .iter()
            .filter(|node| node.node_type == "embedding")
            .find_map(|node| {
                node.data
                    .get("model")
                    .and_then(|value| value.as_str())
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
            })
            .map(PathBuf::from);

        if let Some(path) = embedding_model_path {
            return Ok(path);
        }

        let puma_model_path = graph
            .nodes
            .iter()
            .filter(|node| node.node_type == "puma-lib")
            .find_map(|node| {
                node.data
                    .get("modelPath")
                    .or_else(|| node.data.get("model_path"))
                    .and_then(|value| value.as_str())
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
            })
            .map(PathBuf::from);

        puma_model_path.ok_or_else(|| {
            WorkflowServiceError::InvalidRequest(
                "embedding workflow is missing model path metadata on embedding/puma-lib nodes"
                    .to_string(),
            )
        })
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
        let model_path = self.detect_model_path(&graph)?;
        self.ensure_gateway_started(&model_path).await?;

        let execution_id = Uuid::new_v4().to_string();
        let core = Arc::new(
            node_engine::CoreTaskExecutor::new()
                .with_project_root(self.config.project_root())
                .with_gateway(self.gateway.clone())
                .with_execution_id(execution_id.clone()),
        );
        let task_executor = node_engine::CompositeTaskExecutor::new(None, core);

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

pub fn build_embedding_provider_from_env() -> Result<Arc<dyn EmbeddingProvider>, String> {
    let config = PantographRuntimeConfig::from_env()?;
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
