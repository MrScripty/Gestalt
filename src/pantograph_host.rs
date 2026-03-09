use emily::error::EmilyError;
use emily::inference::{
    EmbeddingProvider, PantographEmbeddingProvider,
    PantographWorkflowBinding as EmilyWorkflowBinding, PantographWorkflowEmbeddingConfig,
    PantographWorkflowServiceClient,
};
use emily::model::{EmbeddingProviderStatus, VectorizationConfigPatch};
use emily_membrane::providers::{
    InMemoryProviderRegistry, MembraneProvider, MembraneProviderRegistry, PantographProviderConfig,
    PantographWorkflowBinding as MembraneWorkflowBinding, PantographWorkflowProvider,
    ProviderCostClass, ProviderLatencyClass, ProviderMetadataClass, ProviderTarget,
    ProviderValidationCompatibility, RegisteredProviderTarget,
};
use inference::{BackendConfig, InferenceGateway, StdProcessSpawner};
use pantograph_workflow_service::{
    FileSystemWorkflowGraphStore, WorkflowGraphEditSessionCloseRequest,
    WorkflowGraphEditSessionCreateRequest, WorkflowGraphLoadRequest, WorkflowGraphSaveRequest,
    WorkflowGraphUpdateNodeDataRequest, WorkflowHost, WorkflowIoRequest, WorkflowOutputTarget,
    WorkflowPortBinding, WorkflowRunOptions, WorkflowRunRequest, WorkflowRunResponse,
    WorkflowService, WorkflowServiceError, capabilities,
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::watch;
use uuid::Uuid;

// Ensure workflow node descriptors are linked into this binary.
use workflow_nodes as _;

const DEFAULT_TIMEOUT_MS: u64 = 120_000;
const DEFAULT_EMBED_DIMENSIONS: usize = 2560;
const DEFAULT_EMBEDDING_PROFILE_ID: &str = "Qwen3-Embedding-4B-GGUF";
const DEFAULT_EMBEDDING_MODEL_ID: &str = "embedding/qwen3/qwen3-embedding-4b-gguf";
const DEFAULT_EMBEDDING_MODEL_NAME: &str = "Qwen3-Embedding-4B-GGUF";
const DEFAULT_EMBEDDING_MODEL_RELATIVE_PATH: &str = "shared-resources/models/embedding/qwen3/qwen3-embedding-4b-gguf/Qwen3-Embedding-4B-Q4_K_M.gguf";
const DEFAULT_MAX_VALUE_BYTES: usize = 256 * 1024;
const DEFAULT_REASONING_PROVIDER_ID: &str = "pantograph-qwen-reasoning";
const DEFAULT_REASONING_MODEL_ID: &str = "Qwen3.5-35B-A3B-GGUF";
const DEFAULT_REASONING_PROFILE_ID: &str = "reasoning";
const DEFAULT_REASONING_CAPABILITY_TAGS: [&str; 2] = ["analysis", "reasoning"];
const LLAMA_WRAPPER_CANONICAL_NAME: &str = "llama-server-wrapper";
const LLAMA_WRAPPER_PLATFORM_NAME: &str = "llama-server-wrapper-x86_64-unknown-linux-gnu";
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
    pub profile_id: String,
    pub text_input_node_id: Option<String>,
    pub text_input_port_id: Option<String>,
    pub vector_output_node_id: Option<String>,
    pub vector_output_port_id: Option<String>,
    pub model_path_override: Option<PathBuf>,
}

impl PantographRuntimeConfig {
    pub fn from_env() -> Result<Self, String> {
        let pantograph_root = pantograph_root_from_env();

        let workflow_id = std::env::var("GESTALT_PANTOGRAPH_WORKFLOW_ID")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "Embedding".to_string());

        let timeout_ms = timeout_ms_from_env("GESTALT_PANTOGRAPH_WORKFLOW_TIMEOUT_MS");

        let expected_dimensions = std::env::var("GESTALT_EMBEDDING_DIMENSIONS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(DEFAULT_EMBED_DIMENSIONS);
        let profile_id = std::env::var("GESTALT_EMBEDDING_PROFILE_ID")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_EMBEDDING_PROFILE_ID.to_string());

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
            profile_id,
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

    fn raw_binaries_dir(&self) -> PathBuf {
        self.pantograph_root.join("src-tauri/binaries")
    }

    fn binaries_dir(&self) -> Result<PathBuf, String> {
        let binaries_dir = self.raw_binaries_dir();
        let canonical = binaries_dir.join(LLAMA_WRAPPER_CANONICAL_NAME);
        if canonical.exists() {
            return Ok(binaries_dir);
        }

        let platform_wrapper = binaries_dir.join(LLAMA_WRAPPER_PLATFORM_NAME);
        if !platform_wrapper.is_file() {
            return Ok(binaries_dir);
        }

        prepare_llama_wrapper_shim(&platform_wrapper)
    }

    fn data_dir(&self) -> PathBuf {
        self.pantograph_root.join("launcher-data")
    }

    pub fn vectorization_patch(&self) -> VectorizationConfigPatch {
        VectorizationConfigPatch {
            enabled: Some(true),
            expected_dimensions: Some(self.expected_dimensions),
            profile_id: Some(self.profile_id.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PantographEmbeddingValidationReport {
    pub workflow_id: String,
    pub profile_id: String,
    pub configured_expected_dimensions: usize,
    pub validated_dimensions: usize,
    pub configured_model_path_override: Option<PathBuf>,
    pub effective_model_path_override: PathBuf,
    pub saved_workflow_path: String,
    pub updated_puma_lib_node_id: String,
    pub resolved_model_path: Option<PathBuf>,
    pub validate_session_id: Option<String>,
    pub session_id: Option<String>,
    pub session_state: Option<String>,
    pub workflow_probe_vector_length: usize,
    pub session_probe_vector_length: usize,
    pub second_probe_vector_length: usize,
    pub third_probe_vector_length: usize,
    pub first_run_ms: u128,
    pub second_run_ms: u128,
    pub third_run_ms: u128,
    pub session_reused_across_runs: bool,
    pub vector_preview: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PantographReasoningRuntimeConfig {
    pub pantograph_root: PathBuf,
    pub workflow_id: String,
    pub timeout_ms: Option<u64>,
    pub provider_id: String,
    pub model_id: String,
    pub profile_id: String,
    pub capability_tags: Vec<String>,
    pub text_input_node_id: Option<String>,
    pub text_input_port_id: Option<String>,
    pub text_output_node_id: Option<String>,
    pub text_output_port_id: Option<String>,
}

impl PantographReasoningRuntimeConfig {
    pub fn from_env() -> Result<Option<Self>, String> {
        Self::from_env_with(|key| std::env::var(key).ok())
    }

    fn from_env_with<F>(mut getenv: F) -> Result<Option<Self>, String>
    where
        F: FnMut(&str) -> Option<String>,
    {
        let workflow_id = getenv("GESTALT_PANTOGRAPH_REASONING_WORKFLOW_ID")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let Some(workflow_id) = workflow_id else {
            return Ok(None);
        };

        let pantograph_root = pantograph_root_from(getenv("GESTALT_PANTOGRAPH_ROOT"));
        if !pantograph_root.exists() {
            return Err(format!(
                "pantograph root does not exist: {}",
                pantograph_root.display()
            ));
        }

        let provider_id = getenv("GESTALT_PANTOGRAPH_REASONING_PROVIDER_ID")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_REASONING_PROVIDER_ID.to_string());
        let model_id = getenv("GESTALT_PANTOGRAPH_REASONING_MODEL_ID")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_REASONING_MODEL_ID.to_string());
        let profile_id = getenv("GESTALT_PANTOGRAPH_REASONING_PROFILE_ID")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_REASONING_PROFILE_ID.to_string());
        let capability_tags = getenv("GESTALT_PANTOGRAPH_REASONING_CAPABILITY_TAGS")
            .map(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            })
            .filter(|tags| !tags.is_empty())
            .unwrap_or_else(|| {
                DEFAULT_REASONING_CAPABILITY_TAGS
                    .iter()
                    .map(|value| (*value).to_string())
                    .collect()
            });

        Ok(Some(Self {
            pantograph_root,
            workflow_id,
            timeout_ms: timeout_ms_from(getenv("GESTALT_PANTOGRAPH_REASONING_TIMEOUT_MS")),
            provider_id,
            model_id,
            profile_id,
            capability_tags,
            text_input_node_id: optional_env_from(getenv(
                "GESTALT_PANTOGRAPH_REASONING_TEXT_NODE_ID",
            )),
            text_input_port_id: optional_env_from(getenv(
                "GESTALT_PANTOGRAPH_REASONING_TEXT_PORT_ID",
            )),
            text_output_node_id: optional_env_from(getenv(
                "GESTALT_PANTOGRAPH_REASONING_OUTPUT_NODE_ID",
            )),
            text_output_port_id: optional_env_from(getenv(
                "GESTALT_PANTOGRAPH_REASONING_OUTPUT_PORT_ID",
            )),
        }))
    }

    fn to_host_runtime_config(&self) -> PantographRuntimeConfig {
        PantographRuntimeConfig {
            pantograph_root: self.pantograph_root.clone(),
            workflow_id: self.workflow_id.clone(),
            timeout_ms: self.timeout_ms,
            expected_dimensions: DEFAULT_EMBED_DIMENSIONS,
            profile_id: DEFAULT_EMBEDDING_PROFILE_ID.to_string(),
            text_input_node_id: self.text_input_node_id.clone(),
            text_input_port_id: self.text_input_port_id.clone(),
            vector_output_node_id: None,
            vector_output_port_id: None,
            model_path_override: None,
        }
    }

    fn registered_target(&self) -> RegisteredProviderTarget {
        RegisteredProviderTarget {
            target: ProviderTarget {
                provider_id: self.provider_id.clone(),
                model_id: Some(self.model_id.clone()),
                profile_id: Some(self.profile_id.clone()),
                capability_tags: self.capability_tags.clone(),
                metadata: serde_json::json!({
                    "source": "gestalt-pantograph-host",
                    "workflow_id": self.workflow_id,
                }),
            },
            metadata_class: ProviderMetadataClass::Preferred,
            latency_class: ProviderLatencyClass::High,
            cost_class: ProviderCostClass::High,
            validation_compatibility: ProviderValidationCompatibility::ReviewFriendly,
            telemetry: None,
        }
    }
}

fn pantograph_root_from_env() -> PathBuf {
    pantograph_root_from(std::env::var("GESTALT_PANTOGRAPH_ROOT").ok())
}

fn pantograph_root_from(value: Option<String>) -> PathBuf {
    value
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/media/jeremy/OrangeCream/Linux Software/Pantograph"))
}

fn timeout_ms_from_env(key: &str) -> Option<u64> {
    timeout_ms_from(std::env::var(key).ok())
}

fn timeout_ms_from(value: Option<String>) -> Option<u64> {
    value
        .and_then(|value| value.parse::<u64>().ok())
        .or(Some(DEFAULT_TIMEOUT_MS))
}

fn optional_env_from(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn default_embedding_model_path_for_root(pantograph_root: &Path) -> Option<PathBuf> {
    let candidate = pantograph_root
        .parent()
        .map(|root| root.join("Pumas-Library"))
        .map(|root| root.join(DEFAULT_EMBEDDING_MODEL_RELATIVE_PATH))?;
    candidate.is_file().then_some(candidate)
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn prepare_llama_wrapper_shim(platform_wrapper: &Path) -> Result<PathBuf, String> {
    let shim_dir = std::env::temp_dir().join("gestalt-pantograph-llama-wrapper");
    fs::create_dir_all(&shim_dir).map_err(|error| {
        format!(
            "failed creating Pantograph wrapper shim directory {}: {error}",
            shim_dir.display()
        )
    })?;

    let shim_path = shim_dir.join(LLAMA_WRAPPER_CANONICAL_NAME);
    let wrapper_target = shell_single_quote(&platform_wrapper.display().to_string());
    let shim_content = format!("#!/bin/bash\nexec {wrapper_target} \"$@\"\n");
    fs::write(&shim_path, shim_content).map_err(|error| {
        format!(
            "failed writing Pantograph wrapper shim {}: {error}",
            shim_path.display()
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(&shim_path)
            .map_err(|error| {
                format!(
                    "failed reading Pantograph wrapper shim metadata {}: {error}",
                    shim_path.display()
                )
            })?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&shim_path, permissions).map_err(|error| {
            format!(
                "failed setting Pantograph wrapper shim permissions {}: {error}",
                shim_path.display()
            )
        })?;
    }

    Ok(shim_dir)
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
        let binaries_dir = self
            .config
            .binaries_dir()
            .map_err(node_engine::NodeEngineError::ExecutionFailed)?;
        let spawner = Arc::new(StdProcessSpawner::new(binaries_dir, self.config.data_dir()));
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
        if !binaries_dir?.exists() {
            return Err(format!(
                "pantograph binaries directory does not exist: {}",
                config.raw_binaries_dir().display()
            ));
        }
        Ok(Self {
            config,
            gateway: Arc::new(InferenceGateway::new()),
            runtime_state: Arc::new(Mutex::new(HostRuntimeState::default())),
        })
    }

    fn active_model_path(&self) -> Result<Option<PathBuf>, String> {
        self.runtime_state
            .lock()
            .map(|state| state.active_model_path.clone())
            .map_err(|_| "host runtime state lock poisoned".to_string())
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

    fn max_value_bytes(&self) -> usize {
        DEFAULT_MAX_VALUE_BYTES
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
) -> Result<(EmilyWorkflowBinding, EmilyWorkflowBinding), String> {
    let input_binding = if let (Some(node_id), Some(port_id)) = (
        config.text_input_node_id.as_ref(),
        config.text_input_port_id.as_ref(),
    ) {
        EmilyWorkflowBinding::new(node_id.clone(), port_id.clone())
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
        EmilyWorkflowBinding::new(node.node_id.clone(), port.port_id.clone())
            .map_err(|error| error.to_string())?
    };

    let output_binding = if let (Some(node_id), Some(port_id)) = (
        config.vector_output_node_id.as_ref(),
        config.vector_output_port_id.as_ref(),
    ) {
        EmilyWorkflowBinding::new(node_id.clone(), port_id.clone())
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
        EmilyWorkflowBinding::new(node.node_id.clone(), port.port_id.clone())
            .map_err(|error| error.to_string())?
    };

    Ok((input_binding, output_binding))
}

fn reasoning_bindings_from_io(
    io: &pantograph_workflow_service::WorkflowIoResponse,
    config: &PantographReasoningRuntimeConfig,
) -> Result<(MembraneWorkflowBinding, Vec<MembraneWorkflowBinding>), String> {
    let input_binding = if let (Some(node_id), Some(port_id)) = (
        config.text_input_node_id.as_ref(),
        config.text_input_port_id.as_ref(),
    ) {
        MembraneWorkflowBinding::new(node_id.clone(), port_id.clone())
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
        MembraneWorkflowBinding::new(node.node_id.clone(), port.port_id.clone())
            .map_err(|error| error.to_string())?
    };

    let output_binding = if let (Some(node_id), Some(port_id)) = (
        config.text_output_node_id.as_ref(),
        config.text_output_port_id.as_ref(),
    ) {
        MembraneWorkflowBinding::new(node_id.clone(), port_id.clone())
            .map_err(|error| error.to_string())?
    } else {
        let node = io
            .outputs
            .iter()
            .find(|node| node.node_type == "text-output")
            .or_else(|| io.outputs.first())
            .ok_or_else(|| "workflow_get_io returned no output nodes".to_string())?;
        let port = node
            .ports
            .iter()
            .find(|port| port.port_id == "text")
            .or_else(|| node.ports.first())
            .ok_or_else(|| "selected output node has no bindable ports".to_string())?;
        MembraneWorkflowBinding::new(node.node_id.clone(), port.port_id.clone())
            .map_err(|error| error.to_string())?
    };

    Ok((input_binding, vec![output_binding]))
}

async fn discover_embedding_bindings(
    host: &GestaltPantographHost,
    config: &PantographRuntimeConfig,
) -> Result<(EmilyWorkflowBinding, EmilyWorkflowBinding), String> {
    let workflow_service = WorkflowService::new();
    let io = workflow_service
        .workflow_get_io(
            host,
            WorkflowIoRequest {
                workflow_id: config.workflow_id.clone(),
            },
        )
        .await
        .map_err(|error| format!("workflow_get_io bootstrap failed: {error}"))?;
    binding_from_io(&io, config)
}

fn parse_embedding_vector_value(value: &serde_json::Value) -> Result<Vec<f32>, String> {
    let Some(values) = value.as_array() else {
        return Err("workflow output binding was not an embedding array".to_string());
    };

    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            value
                .as_f64()
                .map(|entry| entry as f32)
                .ok_or_else(|| format!("embedding value at index {index} was not numeric"))
        })
        .collect()
}

async fn build_embedding_provider_components(
    config: PantographRuntimeConfig,
) -> Result<(Arc<GestaltPantographHost>, Arc<PantographEmbeddingProvider>), String> {
    let host = Arc::new(GestaltPantographHost::new(config.clone())?);
    let (text_input, vector_output) = discover_embedding_bindings(host.as_ref(), &config).await?;
    let embedding_config = PantographWorkflowEmbeddingConfig::new(
        config.workflow_id,
        text_input,
        vector_output,
        config.timeout_ms,
        config.expected_dimensions,
    )
    .map_err(|error| error.to_string())?;

    let client = Arc::new(PantographWorkflowServiceClient::new(host.clone()));
    let provider = Arc::new(
        PantographEmbeddingProvider::new(client, embedding_config)
            .map_err(|error| error.to_string())?,
    );
    Ok((host, provider))
}

async fn update_embedding_workflow_graph(
    config: &PantographRuntimeConfig,
    model_path: &Path,
) -> Result<(String, String), String> {
    let workflow_service = WorkflowService::new();
    let store = FileSystemWorkflowGraphStore::new(config.project_root());
    let workflow_path = format!(".pantograph/workflows/{}.json", config.workflow_id);
    let workflow_file = workflow_service
        .workflow_graph_load(
            &store,
            WorkflowGraphLoadRequest {
                path: workflow_path,
            },
        )
        .map_err(|error| format!("workflow graph load failed: {error}"))?;

    let edit_session = workflow_service
        .workflow_graph_create_edit_session(WorkflowGraphEditSessionCreateRequest {
            graph: workflow_file.graph,
        })
        .await
        .map_err(|error| format!("workflow edit-session create failed: {error}"))?;

    let session_graph = workflow_service
        .workflow_graph_get_edit_session_graph(
            pantograph_workflow_service::WorkflowGraphEditSessionGraphRequest {
                session_id: edit_session.session_id.clone(),
            },
        )
        .await
        .map_err(|error| format!("workflow edit-session graph load failed: {error}"))?;

    let puma_lib_node = session_graph
        .graph
        .nodes
        .iter()
        .find(|node| node.node_type == "puma-lib")
        .ok_or_else(|| "embedding workflow is missing a puma-lib node".to_string())?;
    let mut updated_data = puma_lib_node.data.clone();
    updated_data["modelPath"] = serde_json::json!(model_path);
    updated_data["modelName"] = serde_json::json!(DEFAULT_EMBEDDING_MODEL_NAME);
    updated_data["model_id"] = serde_json::json!(DEFAULT_EMBEDDING_MODEL_ID);
    updated_data["dependency_requirements_id"] = serde_json::json!(DEFAULT_EMBEDDING_MODEL_ID);
    updated_data["selectionMode"] = serde_json::json!("library");
    updated_data["task_type_primary"] = serde_json::json!("feature-extraction");
    updated_data["dependency_requirements"]["model_id"] =
        serde_json::json!(DEFAULT_EMBEDDING_MODEL_ID);

    let updated_graph = workflow_service
        .workflow_graph_update_node_data(WorkflowGraphUpdateNodeDataRequest {
            session_id: edit_session.session_id.clone(),
            node_id: puma_lib_node.id.clone(),
            data: updated_data,
        })
        .await
        .map_err(|error| format!("workflow puma-lib update failed: {error}"))?;

    let save = workflow_service
        .workflow_graph_save(
            &store,
            WorkflowGraphSaveRequest {
                name: config.workflow_id.clone(),
                graph: updated_graph.graph,
            },
        )
        .map_err(|error| format!("workflow graph save failed: {error}"))?;

    workflow_service
        .workflow_graph_close_edit_session(WorkflowGraphEditSessionCloseRequest {
            session_id: edit_session.session_id,
        })
        .await
        .map_err(|error| format!("workflow edit-session close failed: {error}"))?;

    Ok((save.path, puma_lib_node.id.clone()))
}

async fn run_embedding_workflow_probe(
    host: &GestaltPantographHost,
    config: &PantographRuntimeConfig,
    text_input: &EmilyWorkflowBinding,
    vector_output: &EmilyWorkflowBinding,
    text: &str,
) -> Result<Vec<f32>, String> {
    let workflow_service = WorkflowService::new();
    let response = workflow_service
        .workflow_run(
            host,
            WorkflowRunRequest {
                workflow_id: config.workflow_id.clone(),
                inputs: vec![WorkflowPortBinding {
                    node_id: text_input.node_id.clone(),
                    port_id: text_input.port_id.clone(),
                    value: serde_json::json!(text),
                }],
                output_targets: Some(vec![WorkflowOutputTarget {
                    node_id: vector_output.node_id.clone(),
                    port_id: vector_output.port_id.clone(),
                }]),
                timeout_ms: config.timeout_ms,
                run_id: None,
            },
        )
        .await
        .map_err(|error| format!("workflow_run embedding probe failed: {error}"))?;

    let binding = response
        .outputs
        .iter()
        .find(|binding| {
            binding.node_id == vector_output.node_id && binding.port_id == vector_output.port_id
        })
        .ok_or_else(|| {
            format!(
                "workflow output '{}.{}' missing from probe response",
                vector_output.node_id, vector_output.port_id
            )
        })?;
    parse_embedding_vector_value(&binding.value)
}

fn bootstrap_embedding_provider(
    config: PantographRuntimeConfig,
) -> Result<Arc<dyn EmbeddingProvider>, String> {
    run_bootstrap_blocking(async move {
        let (_, provider) = build_embedding_provider_components(config).await?;
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

pub fn build_embedding_vectorization_patch_from_env() -> Result<VectorizationConfigPatch, String> {
    Ok(PantographRuntimeConfig::from_env()?.vectorization_patch())
}

pub fn validate_embedding_roundtrip_from_env(
    text: &str,
) -> Result<PantographEmbeddingValidationReport, String> {
    let mut config = PantographRuntimeConfig::from_env()?;
    let configured_expected_dimensions = config.expected_dimensions;
    let configured_model_path_override = config.model_path_override.clone();
    let effective_model_path_override = config
        .model_path_override
        .clone()
        .or_else(|| default_embedding_model_path_for_root(&config.pantograph_root))
        .ok_or_else(|| {
            format!(
                "no embedding model path override configured and default Qwen3-Embedding-4B model was not found under {}",
                config.pantograph_root.display()
            )
        })?;
    config.model_path_override = Some(effective_model_path_override.clone());
    let text = text.to_string();

    run_bootstrap_blocking(async move {
        let (saved_workflow_path, updated_puma_lib_node_id) =
            update_embedding_workflow_graph(&config, &effective_model_path_override).await?;
        let host = Arc::new(GestaltPantographHost::new(config.clone())?);
        let (text_input, vector_output) =
            discover_embedding_bindings(host.as_ref(), &config).await?;
        let workflow_probe_vector = run_embedding_workflow_probe(
            host.as_ref(),
            &config,
            &text_input,
            &vector_output,
            &text,
        )
        .await?;
        let resolved_model_path = host.active_model_path()?;

        let provider_config = PantographRuntimeConfig {
            expected_dimensions: workflow_probe_vector.len(),
            ..config.clone()
        };
        let (_, provider) = build_embedding_provider_components(provider_config).await?;
        provider
            .validate()
            .await
            .map_err(|error| format!("pantograph provider validation failed: {error}"))?;
        let validate_status = provider.status().await;
        let validate_session_id = validate_status
            .as_ref()
            .and_then(|status| status.session_id.clone());

        let first_text = text.clone();
        let second_text = format!("{text} Second warm embedding pass.");
        let third_text = format!("{text} Third warm embedding pass.");

        let first_started = Instant::now();
        let vector = provider
            .embed_text(&first_text)
            .await
            .map_err(|error| format!("pantograph embedding probe failed: {error}"))?;
        let first_run_ms = first_started.elapsed().as_millis();

        let first_status = provider.status().await;
        let first_session_id = first_status
            .as_ref()
            .and_then(|status| status.session_id.clone());

        let second_started = Instant::now();
        let second_vector = provider
            .embed_text(&second_text)
            .await
            .map_err(|error| format!("pantograph warm embedding probe failed: {error}"))?;
        let second_run_ms = second_started.elapsed().as_millis();

        let second_status = provider.status().await;
        let second_session_id = second_status
            .as_ref()
            .and_then(|status| status.session_id.clone());

        let third_started = Instant::now();
        let third_vector = provider
            .embed_text(&third_text)
            .await
            .map_err(|error| format!("pantograph third embedding probe failed: {error}"))?;
        let third_run_ms = third_started.elapsed().as_millis();

        let status = provider.status().await;
        let third_session_id = status.as_ref().and_then(|status| status.session_id.clone());
        provider
            .shutdown()
            .await
            .map_err(|error| format!("pantograph provider shutdown failed: {error}"))?;

        let session_reused_across_runs = validate_session_id.is_some()
            && validate_session_id == first_session_id
            && first_session_id == second_session_id
            && second_session_id == third_session_id;

        Ok(PantographEmbeddingValidationReport {
            workflow_id: config.workflow_id.clone(),
            profile_id: config.profile_id.clone(),
            configured_expected_dimensions,
            validated_dimensions: workflow_probe_vector.len(),
            configured_model_path_override,
            effective_model_path_override,
            saved_workflow_path,
            updated_puma_lib_node_id,
            resolved_model_path,
            validate_session_id,
            session_id: third_session_id,
            session_state: status.as_ref().map(|status| status.state.clone()),
            workflow_probe_vector_length: workflow_probe_vector.len(),
            session_probe_vector_length: vector.len(),
            second_probe_vector_length: second_vector.len(),
            third_probe_vector_length: third_vector.len(),
            first_run_ms,
            second_run_ms,
            third_run_ms,
            session_reused_across_runs,
            vector_preview: vector.into_iter().take(8).collect(),
        })
    })
}

fn build_membrane_provider_registry(
    config: PantographReasoningRuntimeConfig,
) -> Result<Arc<dyn MembraneProviderRegistry>, String> {
    let host_config = config.to_host_runtime_config();
    let host = Arc::new(GestaltPantographHost::new(host_config)?);
    let service = WorkflowService::new();

    let provider = run_bootstrap_blocking({
        let host = host.clone();
        let config = config.clone();
        async move {
            let (text_input, output_targets) = if config.text_input_node_id.is_some()
                && config.text_input_port_id.is_some()
                && config.text_output_node_id.is_some()
                && config.text_output_port_id.is_some()
            {
                (
                    MembraneWorkflowBinding::new(
                        config.text_input_node_id.clone().expect("checked above"),
                        config.text_input_port_id.clone().expect("checked above"),
                    )
                    .map_err(|error| error.to_string())?,
                    vec![
                        MembraneWorkflowBinding::new(
                            config.text_output_node_id.clone().expect("checked above"),
                            config.text_output_port_id.clone().expect("checked above"),
                        )
                        .map_err(|error| error.to_string())?,
                    ],
                )
            } else {
                let io = service
                    .workflow_get_io(
                        host.as_ref(),
                        WorkflowIoRequest {
                            workflow_id: config.workflow_id.clone(),
                        },
                    )
                    .await
                    .map_err(|error| format!("workflow_get_io bootstrap failed: {error}"))?;
                reasoning_bindings_from_io(&io, &config)?
            };

            let provider_config = PantographProviderConfig::new(
                config.provider_id.clone(),
                config.workflow_id.clone(),
                text_input,
                output_targets,
                config.timeout_ms,
            )
            .map_err(|error| error.to_string())?;

            let provider: Arc<dyn MembraneProvider> = Arc::new(
                PantographWorkflowProvider::new(host, provider_config)
                    .map_err(|error| error.to_string())?,
            );
            Ok(provider)
        }
    })?;

    Ok(Arc::new(InMemoryProviderRegistry::single_target(
        config.registered_target(),
        provider,
    )) as Arc<dyn MembraneProviderRegistry>)
}

pub fn build_membrane_provider_registry_from_env()
-> Result<Option<Arc<dyn MembraneProviderRegistry>>, String> {
    let Some(config) = PantographReasoningRuntimeConfig::from_env()? else {
        return Ok(None);
    };
    build_membrane_provider_registry(config).map(Some)
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
    use super::{
        DeferredEmbeddingProvider, PantographReasoningRuntimeConfig, PantographRuntimeConfig,
        binding_from_io, build_membrane_provider_registry, default_embedding_model_path_for_root,
        prepare_llama_wrapper_shim, resolve_embedding_model_path_from_inputs,
    };
    use emily::inference::EmbeddingProvider;
    use emily::model::EmbeddingProviderStatus;
    use emily_membrane::providers::MembraneProviderRegistry;
    use pantograph_workflow_service::{WorkflowHost, WorkflowIoRequest, WorkflowService};
    use std::collections::HashMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use uuid::Uuid;

    struct RecordingEmbeddingExecutor {
        recorded_models: Arc<Mutex<Vec<PathBuf>>>,
    }

    #[async_trait::async_trait]
    impl node_engine::TaskExecutor for RecordingEmbeddingExecutor {
        async fn execute_task(
            &self,
            task_id: &str,
            inputs: HashMap<String, serde_json::Value>,
            _context: &node_engine::Context,
            _extensions: &node_engine::ExecutorExtensions,
        ) -> node_engine::Result<HashMap<String, serde_json::Value>> {
            let node_type = node_engine::resolve_node_type(task_id, &inputs);
            if node_type != "embedding" {
                return Err(node_engine::NodeEngineError::ExecutionFailed(format!(
                    "Node type '{}' requires host-specific executor",
                    node_type
                )));
            }

            let model_path = resolve_embedding_model_path_from_inputs(&inputs, None)?;
            self.recorded_models
                .lock()
                .expect("recorded model lock")
                .push(model_path);

            let mut outputs = HashMap::new();
            outputs.insert(
                "embedding".to_string(),
                serde_json::json!([0.25_f32, 0.5_f32]),
            );
            Ok(outputs)
        }
    }

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

    fn test_config() -> PantographRuntimeConfig {
        PantographRuntimeConfig {
            pantograph_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../Pantograph"),
            workflow_id: "Embedding".to_string(),
            timeout_ms: Some(1_000),
            expected_dimensions: DEFAULT_EMBED_DIMENSIONS,
            profile_id: DEFAULT_EMBEDDING_PROFILE_ID.to_string(),
            text_input_node_id: None,
            text_input_port_id: None,
            vector_output_node_id: None,
            vector_output_port_id: None,
            model_path_override: None,
        }
    }

    fn pantograph_root_string() -> String {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../Pantograph")
            .display()
            .to_string()
    }

    #[test]
    fn default_embedding_model_path_finds_qwen_4b_model() {
        let pantograph_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../Pantograph");
        let model_path =
            default_embedding_model_path_for_root(&pantograph_root).expect("default model path");
        assert_eq!(
            model_path,
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../Pumas-Library")
                .join(super::DEFAULT_EMBEDDING_MODEL_RELATIVE_PATH)
        );
        assert!(model_path.is_file());
    }

    #[test]
    fn prepare_llama_wrapper_shim_creates_runtime_wrapper() {
        let root = std::env::temp_dir().join(format!("gestalt-pantograph-host-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create temp binaries dir");
        let platform_wrapper = root.join(super::LLAMA_WRAPPER_PLATFORM_NAME);
        fs::write(&platform_wrapper, "#!/bin/sh\n").expect("write platform wrapper");

        let shim_dir = prepare_llama_wrapper_shim(&platform_wrapper).expect("create shim");

        let canonical = shim_dir.join(super::LLAMA_WRAPPER_CANONICAL_NAME);
        assert!(canonical.exists());

        fs::remove_dir_all(&root).expect("remove temp binaries dir");
        let _ = fs::remove_dir_all(&shim_dir);
    }

    #[test]
    fn reasoning_runtime_config_returns_none_without_workflow_id() {
        let config = PantographReasoningRuntimeConfig::from_env_with(|_| None)
            .expect("config parse should succeed");
        assert_eq!(config, None);
    }

    #[test]
    fn embedding_runtime_config_emits_vectorization_patch_defaults() {
        let patch = test_config().vectorization_patch();
        assert_eq!(patch.enabled, Some(true));
        assert_eq!(patch.expected_dimensions, Some(DEFAULT_EMBED_DIMENSIONS));
        assert_eq!(
            patch.profile_id.as_deref(),
            Some(DEFAULT_EMBEDDING_PROFILE_ID)
        );
    }

    #[test]
    fn reasoning_runtime_config_applies_defaults_from_host_env_shape() {
        let pantograph_root = pantograph_root_string();
        let config = PantographReasoningRuntimeConfig::from_env_with(|key| match key {
            "GESTALT_PANTOGRAPH_ROOT" => Some(pantograph_root.clone()),
            "GESTALT_PANTOGRAPH_REASONING_WORKFLOW_ID" => Some("Qwen Reasoning".to_string()),
            _ => None,
        })
        .expect("config parse should succeed")
        .expect("workflow id should enable config");

        assert_eq!(config.pantograph_root, PathBuf::from(pantograph_root));
        assert_eq!(config.workflow_id, "Qwen Reasoning");
        assert_eq!(config.timeout_ms, Some(super::DEFAULT_TIMEOUT_MS));
        assert_eq!(config.provider_id, super::DEFAULT_REASONING_PROVIDER_ID);
        assert_eq!(config.model_id, super::DEFAULT_REASONING_MODEL_ID);
        assert_eq!(config.profile_id, super::DEFAULT_REASONING_PROFILE_ID);
        assert_eq!(
            config.capability_tags,
            super::DEFAULT_REASONING_CAPABILITY_TAGS
                .iter()
                .map(|value| (*value).to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(config.text_input_node_id, None);
        assert_eq!(config.text_input_port_id, None);
        assert_eq!(config.text_output_node_id, None);
        assert_eq!(config.text_output_port_id, None);
    }

    #[test]
    fn reasoning_runtime_config_parses_custom_overrides() {
        let pantograph_root = pantograph_root_string();
        let config = PantographReasoningRuntimeConfig::from_env_with(|key| match key {
            "GESTALT_PANTOGRAPH_ROOT" => Some(pantograph_root.clone()),
            "GESTALT_PANTOGRAPH_REASONING_WORKFLOW_ID" => Some("Qwen Reasoning".to_string()),
            "GESTALT_PANTOGRAPH_REASONING_TIMEOUT_MS" => Some("45000".to_string()),
            "GESTALT_PANTOGRAPH_REASONING_PROVIDER_ID" => Some("pantograph-qwen".to_string()),
            "GESTALT_PANTOGRAPH_REASONING_MODEL_ID" => Some("Qwen3.5".to_string()),
            "GESTALT_PANTOGRAPH_REASONING_PROFILE_ID" => Some("remote-reasoning".to_string()),
            "GESTALT_PANTOGRAPH_REASONING_CAPABILITY_TAGS" => {
                Some("analysis, synthesis ,planning".to_string())
            }
            "GESTALT_PANTOGRAPH_REASONING_TEXT_NODE_ID" => Some("text-input-1".to_string()),
            "GESTALT_PANTOGRAPH_REASONING_TEXT_PORT_ID" => Some("text".to_string()),
            "GESTALT_PANTOGRAPH_REASONING_OUTPUT_NODE_ID" => Some("text-output-1".to_string()),
            "GESTALT_PANTOGRAPH_REASONING_OUTPUT_PORT_ID" => Some("text".to_string()),
            _ => None,
        })
        .expect("config parse should succeed")
        .expect("workflow id should enable config");

        assert_eq!(config.timeout_ms, Some(45_000));
        assert_eq!(config.provider_id, "pantograph-qwen");
        assert_eq!(config.model_id, "Qwen3.5");
        assert_eq!(config.profile_id, "remote-reasoning");
        assert_eq!(
            config.capability_tags,
            vec![
                "analysis".to_string(),
                "synthesis".to_string(),
                "planning".to_string(),
            ]
        );
        assert_eq!(config.text_input_node_id.as_deref(), Some("text-input-1"));
        assert_eq!(config.text_input_port_id.as_deref(), Some("text"));
        assert_eq!(config.text_output_node_id.as_deref(), Some("text-output-1"));
        assert_eq!(config.text_output_port_id.as_deref(), Some("text"));
    }

    #[test]
    fn build_membrane_provider_registry_returns_registered_reasoning_target() {
        let config = PantographReasoningRuntimeConfig {
            pantograph_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../Pantograph"),
            workflow_id: "Qwen Reasoning".to_string(),
            timeout_ms: Some(30_000),
            provider_id: "pantograph-qwen-reasoning".to_string(),
            model_id: "Qwen3.5-35B-A3B-GGUF".to_string(),
            profile_id: "reasoning".to_string(),
            capability_tags: vec!["analysis".to_string(), "reasoning".to_string()],
            text_input_node_id: Some("text-input-1".to_string()),
            text_input_port_id: Some("text".to_string()),
            text_output_node_id: Some("text-output-1".to_string()),
            text_output_port_id: Some("text".to_string()),
        };

        let registry = build_membrane_provider_registry(config.clone())
            .expect("registry bootstrap should succeed");
        let targets = registry.targets();

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].target.provider_id, config.provider_id);
        assert_eq!(
            targets[0].target.model_id.as_deref(),
            Some(config.model_id.as_str())
        );
        assert_eq!(
            targets[0].target.profile_id.as_deref(),
            Some(config.profile_id.as_str())
        );
        assert_eq!(targets[0].target.capability_tags, config.capability_tags);
        assert_eq!(
            targets[0].target.metadata["source"],
            serde_json::json!("gestalt-pantograph-host")
        );
        assert_eq!(
            targets[0].target.metadata["workflow_id"],
            serde_json::json!(config.workflow_id)
        );
        assert!(registry.provider("pantograph-qwen-reasoning").is_some());
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

    #[test]
    fn resolve_embedding_model_path_prefers_bound_input_over_stale_node_data() {
        let mut inputs = HashMap::new();
        inputs.insert(
            "model".to_string(),
            serde_json::json!("/models/from-edge.gguf"),
        );
        inputs.insert(
            "_data".to_string(),
            serde_json::json!({
                "model": "/models/from-node-data.gguf",
            }),
        );

        let model_path =
            resolve_embedding_model_path_from_inputs(&inputs, None).expect("model path");
        assert_eq!(model_path, PathBuf::from("/models/from-edge.gguf"));
    }

    #[tokio::test]
    async fn test_embedding_workflow_uses_puma_lib_model_path_for_execution() {
        let config = test_config();
        let host = super::GestaltPantographHost::new(config.clone()).expect("host");
        let service = WorkflowService::new();
        let io = service
            .workflow_get_io(
                &host,
                WorkflowIoRequest {
                    workflow_id: config.workflow_id.clone(),
                },
            )
            .await
            .expect("workflow io");
        let (text_input, vector_output) = binding_from_io(&io, &config).expect("bindings");

        let stored = pantograph_workflow_service::capabilities::load_and_validate_workflow(
            &config.workflow_id,
            &host.workflow_roots(),
        )
        .expect("stored workflow");
        let mut graph = stored.to_workflow_graph(&config.workflow_id);
        super::GestaltPantographHost::apply_input_bindings(
            &mut graph,
            &[pantograph_workflow_service::WorkflowPortBinding {
                node_id: text_input.node_id.clone(),
                port_id: text_input.port_id.clone(),
                value: serde_json::json!("from emily"),
            }],
        )
        .expect("input binding");

        let output_node_id = graph
            .nodes
            .iter()
            .find(|node| node.id == vector_output.node_id)
            .expect("vector output node")
            .id
            .clone();

        let recorded_models = Arc::new(Mutex::new(Vec::<PathBuf>::new()));
        let host_executor = Arc::new(RecordingEmbeddingExecutor {
            recorded_models: recorded_models.clone(),
        });
        let core = Arc::new(node_engine::CoreTaskExecutor::new());
        let task_executor = node_engine::CompositeTaskExecutor::new(Some(host_executor), core);
        let executor = node_engine::WorkflowExecutor::new(
            "test-exec",
            graph,
            Arc::new(node_engine::NullEventSink),
        );

        let outputs = executor
            .demand(&output_node_id, &task_executor)
            .await
            .expect("workflow execution");
        assert_eq!(outputs["vector"], serde_json::json!([0.25_f64, 0.5_f64]));

        let expected_model_path = Path::new(
            "/media/jeremy/OrangeCream/Linux Software/Pumas-Library/shared-resources/models/embedding/qwen3/qwen3-embedding-06b-gguf",
        );
        let stale_model_path = Path::new(
            "/media/jeremy/OrangeCream/Linux Software/Pumas-Library/shared-resources/models/embedding/Qwen/Qwen3-Embedding-06B-GGUF",
        );
        let recorded = recorded_models.lock().expect("recorded models");
        assert_eq!(recorded.as_slice(), &[expected_model_path.to_path_buf()]);
        assert_ne!(recorded[0], stale_model_path);
    }
}
