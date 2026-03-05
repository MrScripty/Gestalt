use crate::error::EmilyError;
use crate::model::EmbeddingProviderStatus;
use async_trait::async_trait;

/// Abstraction over embedding providers used by Emily ingestion.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed_text(&self, text: &str) -> Result<Vec<f32>, EmilyError>;

    async fn status(&self) -> Option<EmbeddingProviderStatus> {
        None
    }

    async fn shutdown(&self) -> Result<(), EmilyError> {
        Ok(())
    }
}

/// Default provider for deployments where embeddings are disabled.
#[derive(Debug, Default)]
pub struct NoopEmbeddingProvider;

#[async_trait]
impl EmbeddingProvider for NoopEmbeddingProvider {
    async fn embed_text(&self, _text: &str) -> Result<Vec<f32>, EmilyError> {
        Ok(Vec::new())
    }
}

#[cfg(feature = "pantograph")]
mod pantograph {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    use pantograph_workflow_service::{
        WorkflowHost, WorkflowIoNode, WorkflowIoRequest, WorkflowIoResponse, WorkflowOutputTarget,
        WorkflowPortBinding, WorkflowPreflightRequest, WorkflowPreflightResponse,
        WorkflowRunResponse, WorkflowService, WorkflowServiceError, WorkflowSessionCloseRequest,
        WorkflowSessionCreateRequest, WorkflowSessionCreateResponse,
        WorkflowSessionKeepAliveRequest, WorkflowSessionQueueListRequest,
        WorkflowSessionQueueListResponse, WorkflowSessionRunRequest, WorkflowSessionState,
        WorkflowSessionStatusRequest, WorkflowSessionStatusResponse,
    };

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct WorkflowBinding {
        pub node_id: String,
        pub port_id: String,
    }

    impl WorkflowBinding {
        pub fn new(
            node_id: impl Into<String>,
            port_id: impl Into<String>,
        ) -> Result<Self, EmilyError> {
            let node_id = node_id.into();
            let port_id = port_id.into();
            if node_id.trim().is_empty() {
                return Err(EmilyError::InvalidRequest(
                    "workflow binding node_id cannot be empty".to_string(),
                ));
            }
            if port_id.trim().is_empty() {
                return Err(EmilyError::InvalidRequest(
                    "workflow binding port_id cannot be empty".to_string(),
                ));
            }
            Ok(Self { node_id, port_id })
        }
    }

    #[derive(Debug, Clone)]
    pub struct WorkflowEmbeddingConfig {
        pub workflow_id: String,
        pub text_input: WorkflowBinding,
        pub vector_output: WorkflowBinding,
        pub timeout_ms: Option<u64>,
        pub expected_dimensions: usize,
    }

    impl WorkflowEmbeddingConfig {
        pub fn new(
            workflow_id: impl Into<String>,
            text_input: WorkflowBinding,
            vector_output: WorkflowBinding,
            timeout_ms: Option<u64>,
            expected_dimensions: usize,
        ) -> Result<Self, EmilyError> {
            let workflow_id = workflow_id.into();
            if workflow_id.trim().is_empty() {
                return Err(EmilyError::InvalidRequest(
                    "workflow_id cannot be empty".to_string(),
                ));
            }
            if expected_dimensions == 0 {
                return Err(EmilyError::InvalidRequest(
                    "expected_dimensions must be greater than zero".to_string(),
                ));
            }
            Ok(Self {
                workflow_id,
                text_input,
                vector_output,
                timeout_ms,
                expected_dimensions,
            })
        }
    }

    #[async_trait]
    pub trait WorkflowSessionClient: Send + Sync {
        async fn workflow_get_io(
            &self,
            request: WorkflowIoRequest,
        ) -> Result<WorkflowIoResponse, EmilyError>;
        async fn workflow_preflight(
            &self,
            request: WorkflowPreflightRequest,
        ) -> Result<WorkflowPreflightResponse, EmilyError>;
        async fn create_workflow_session(
            &self,
            request: WorkflowSessionCreateRequest,
        ) -> Result<WorkflowSessionCreateResponse, EmilyError>;
        async fn run_workflow_session(
            &self,
            request: WorkflowSessionRunRequest,
        ) -> Result<WorkflowRunResponse, EmilyError>;
        async fn workflow_get_session_status(
            &self,
            request: WorkflowSessionStatusRequest,
        ) -> Result<WorkflowSessionStatusResponse, EmilyError>;
        async fn workflow_list_session_queue(
            &self,
            request: WorkflowSessionQueueListRequest,
        ) -> Result<WorkflowSessionQueueListResponse, EmilyError>;
        async fn workflow_set_session_keep_alive(
            &self,
            request: WorkflowSessionKeepAliveRequest,
        ) -> Result<(), EmilyError>;
        async fn close_workflow_session(
            &self,
            request: WorkflowSessionCloseRequest,
        ) -> Result<(), EmilyError>;
    }

    /// Workflow-service-backed client for hosts that already expose a WorkflowHost runtime.
    pub struct WorkflowServiceSessionClient<H: WorkflowHost> {
        service: WorkflowService,
        host: Arc<H>,
    }

    impl<H: WorkflowHost> WorkflowServiceSessionClient<H> {
        pub fn new(host: Arc<H>) -> Self {
            Self {
                service: WorkflowService::new(),
                host,
            }
        }
    }

    #[async_trait]
    impl<H: WorkflowHost> WorkflowSessionClient for WorkflowServiceSessionClient<H> {
        async fn workflow_get_io(
            &self,
            request: WorkflowIoRequest,
        ) -> Result<WorkflowIoResponse, EmilyError> {
            self.service
                .workflow_get_io(self.host.as_ref(), request)
                .await
                .map_err(map_workflow_service_error)
        }

        async fn workflow_preflight(
            &self,
            request: WorkflowPreflightRequest,
        ) -> Result<WorkflowPreflightResponse, EmilyError> {
            self.service
                .workflow_preflight(self.host.as_ref(), request)
                .await
                .map_err(map_workflow_service_error)
        }

        async fn create_workflow_session(
            &self,
            request: WorkflowSessionCreateRequest,
        ) -> Result<WorkflowSessionCreateResponse, EmilyError> {
            self.service
                .create_workflow_session(self.host.as_ref(), request)
                .await
                .map_err(map_workflow_service_error)
        }

        async fn run_workflow_session(
            &self,
            request: WorkflowSessionRunRequest,
        ) -> Result<WorkflowRunResponse, EmilyError> {
            self.service
                .run_workflow_session(self.host.as_ref(), request)
                .await
                .map_err(map_workflow_service_error)
        }

        async fn workflow_get_session_status(
            &self,
            request: WorkflowSessionStatusRequest,
        ) -> Result<WorkflowSessionStatusResponse, EmilyError> {
            self.service
                .workflow_get_session_status(request)
                .await
                .map_err(map_workflow_service_error)
        }

        async fn workflow_list_session_queue(
            &self,
            request: WorkflowSessionQueueListRequest,
        ) -> Result<WorkflowSessionQueueListResponse, EmilyError> {
            self.service
                .workflow_list_session_queue(request)
                .await
                .map_err(map_workflow_service_error)
        }

        async fn workflow_set_session_keep_alive(
            &self,
            request: WorkflowSessionKeepAliveRequest,
        ) -> Result<(), EmilyError> {
            self.service
                .workflow_set_session_keep_alive(self.host.as_ref(), request)
                .await
                .map_err(map_workflow_service_error)?;
            Ok(())
        }

        async fn close_workflow_session(
            &self,
            request: WorkflowSessionCloseRequest,
        ) -> Result<(), EmilyError> {
            self.service
                .close_workflow_session(self.host.as_ref(), request)
                .await
                .map_err(map_workflow_service_error)?;
            Ok(())
        }
    }

    fn map_workflow_service_error(error: WorkflowServiceError) -> EmilyError {
        EmilyError::Embedding(format!(
            "workflow request failed: {}",
            error.to_envelope_json()
        ))
    }

    #[derive(Debug, Default, Clone)]
    struct ProviderRuntimeState {
        session_id: Option<String>,
        session_state: Option<WorkflowSessionState>,
        queued_runs: Option<usize>,
        queue_items: Option<usize>,
        keep_alive: bool,
        last_error: Option<String>,
    }

    /// Pantograph workflow-session embedding provider.
    pub struct PantographWorkflowEmbeddingProvider {
        client: Arc<dyn WorkflowSessionClient>,
        config: WorkflowEmbeddingConfig,
        state: Mutex<ProviderRuntimeState>,
    }

    impl PantographWorkflowEmbeddingProvider {
        pub fn new(
            client: Arc<dyn WorkflowSessionClient>,
            config: WorkflowEmbeddingConfig,
        ) -> Result<Self, EmilyError> {
            if matches!(config.timeout_ms, Some(0)) {
                return Err(EmilyError::InvalidRequest(
                    "timeout_ms must be greater than zero when provided".to_string(),
                ));
            }
            Ok(Self {
                client,
                config,
                state: Mutex::new(ProviderRuntimeState::default()),
            })
        }

        pub async fn validate(&self) -> Result<(), EmilyError> {
            let _ = self.ensure_session().await?;
            Ok(())
        }

        async fn ensure_session(&self) -> Result<String, EmilyError> {
            {
                let current = self.state.lock().await;
                if let Some(session_id) = current.session_id.as_ref() {
                    return Ok(session_id.clone());
                }
            }

            self.validate_io_and_preflight().await?;
            let session = self
                .client
                .create_workflow_session(WorkflowSessionCreateRequest {
                    workflow_id: self.config.workflow_id.clone(),
                    usage_profile: None,
                    keep_alive: true,
                })
                .await?;

            self.client
                .workflow_set_session_keep_alive(WorkflowSessionKeepAliveRequest {
                    session_id: session.session_id.clone(),
                    keep_alive: true,
                })
                .await?;

            let status = self
                .client
                .workflow_get_session_status(WorkflowSessionStatusRequest {
                    session_id: session.session_id.clone(),
                })
                .await
                .ok();

            let mut current = self.state.lock().await;
            if let Some(session_id) = current.session_id.as_ref() {
                return Ok(session_id.clone());
            }
            current.session_id = Some(session.session_id.clone());
            current.keep_alive = true;
            current.last_error = None;
            if let Some(status) = status {
                current.session_state = Some(status.session.state);
                current.queued_runs = Some(status.session.queued_runs);
            }
            Ok(session.session_id)
        }

        async fn refresh_runtime_status(&self, session_id: &str) {
            let session_status = self
                .client
                .workflow_get_session_status(WorkflowSessionStatusRequest {
                    session_id: session_id.to_string(),
                })
                .await;
            let queue_status = self
                .client
                .workflow_list_session_queue(WorkflowSessionQueueListRequest {
                    session_id: session_id.to_string(),
                })
                .await;

            let mut current = self.state.lock().await;
            if let Ok(status) = session_status {
                current.session_state = Some(status.session.state);
                current.queued_runs = Some(status.session.queued_runs);
                current.keep_alive = status.session.keep_alive;
                current.last_error = None;
            }
            match queue_status {
                Ok(status) => {
                    current.queue_items = Some(status.items.len());
                }
                Err(error) => {
                    current.last_error = Some(error.to_string());
                }
            }
        }

        async fn clear_session_with_error(&self, error: String) {
            let mut current = self.state.lock().await;
            current.session_id = None;
            current.session_state = None;
            current.queued_runs = None;
            current.queue_items = None;
            current.last_error = Some(error);
        }

        fn is_session_stale_error(error: &EmilyError) -> bool {
            let message = error.to_string();
            message.contains("session_not_found")
                || message.contains("session_evicted")
                || message.contains("queue_item_not_found")
        }

        async fn run_session_once(
            &self,
            session_id: String,
            text: &str,
        ) -> Result<WorkflowRunResponse, EmilyError> {
            self.client
                .workflow_set_session_keep_alive(WorkflowSessionKeepAliveRequest {
                    session_id: session_id.clone(),
                    keep_alive: true,
                })
                .await?;
            self.client
                .run_workflow_session(WorkflowSessionRunRequest {
                    session_id,
                    inputs: vec![WorkflowPortBinding {
                        node_id: self.config.text_input.node_id.clone(),
                        port_id: self.config.text_input.port_id.clone(),
                        value: serde_json::json!(text),
                    }],
                    output_targets: Some(vec![WorkflowOutputTarget {
                        node_id: self.config.vector_output.node_id.clone(),
                        port_id: self.config.vector_output.port_id.clone(),
                    }]),
                    timeout_ms: self.config.timeout_ms,
                    run_id: None,
                    priority: Some(0),
                })
                .await
        }

        async fn validate_io_and_preflight(&self) -> Result<(), EmilyError> {
            let io = self
                .client
                .workflow_get_io(WorkflowIoRequest {
                    workflow_id: self.config.workflow_id.clone(),
                })
                .await?;
            if !io_contains_binding(&io.inputs, &self.config.text_input) {
                return Err(EmilyError::Embedding(format!(
                    "workflow input binding '{}.{}' was not discovered",
                    self.config.text_input.node_id, self.config.text_input.port_id
                )));
            }
            if !io_contains_binding(&io.outputs, &self.config.vector_output) {
                return Err(EmilyError::Embedding(format!(
                    "workflow output binding '{}.{}' was not discovered",
                    self.config.vector_output.node_id, self.config.vector_output.port_id
                )));
            }

            let preflight = self
                .client
                .workflow_preflight(WorkflowPreflightRequest {
                    workflow_id: self.config.workflow_id.clone(),
                    inputs: vec![WorkflowPortBinding {
                        node_id: self.config.text_input.node_id.clone(),
                        port_id: self.config.text_input.port_id.clone(),
                        value: serde_json::json!("preflight-probe"),
                    }],
                    output_targets: Some(vec![WorkflowOutputTarget {
                        node_id: self.config.vector_output.node_id.clone(),
                        port_id: self.config.vector_output.port_id.clone(),
                    }]),
                })
                .await?;

            if !preflight.invalid_targets.is_empty() {
                return Err(EmilyError::Embedding(
                    "workflow preflight rejected configured output target".to_string(),
                ));
            }
            if !preflight.missing_required_inputs.is_empty() {
                return Err(EmilyError::Embedding(
                    "workflow preflight reported missing required inputs".to_string(),
                ));
            }
            if !preflight.can_run {
                return Err(EmilyError::Embedding(
                    "workflow preflight reported can_run=false".to_string(),
                ));
            }
            Ok(())
        }
    }

    #[async_trait]
    impl EmbeddingProvider for PantographWorkflowEmbeddingProvider {
        async fn embed_text(&self, text: &str) -> Result<Vec<f32>, EmilyError> {
            if text.trim().is_empty() {
                return Err(EmilyError::InvalidRequest(
                    "embedding input text cannot be empty".to_string(),
                ));
            }

            let session_id = self.ensure_session().await?;
            let (response, response_session_id) =
                match self.run_session_once(session_id.clone(), text).await {
                    Ok(response) => (response, session_id.clone()),
                    Err(error) => {
                        if Self::is_session_stale_error(&error) {
                            self.clear_session_with_error(error.to_string()).await;
                            let refreshed = self.ensure_session().await?;
                            let response = self
                                .run_session_once(refreshed.clone(), text)
                                .await
                                .map_err(|retry_error| {
                                    if let Ok(mut state) = self.state.try_lock() {
                                        state.last_error = Some(retry_error.to_string());
                                    }
                                    retry_error
                                })?;
                            (response, refreshed)
                        } else {
                            if let Ok(mut state) = self.state.try_lock() {
                                state.last_error = Some(error.to_string());
                            }
                            return Err(error);
                        }
                    }
                };

            let binding = response
                .outputs
                .iter()
                .find(|binding| {
                    binding.node_id == self.config.vector_output.node_id
                        && binding.port_id == self.config.vector_output.port_id
                })
                .ok_or_else(|| {
                    EmilyError::Embedding(format!(
                        "workflow output '{}.{}' missing from run response",
                        self.config.vector_output.node_id, self.config.vector_output.port_id
                    ))
                })?;

            let values = parse_vector_value(&binding.value)?;
            if values.len() != self.config.expected_dimensions {
                return Err(EmilyError::Embedding(format!(
                    "workflow returned embedding dimension {}, expected {}",
                    values.len(),
                    self.config.expected_dimensions
                )));
            }
            self.refresh_runtime_status(&response_session_id).await;
            Ok(values)
        }

        async fn status(&self) -> Option<EmbeddingProviderStatus> {
            let current = self.state.lock().await.clone();
            Some(EmbeddingProviderStatus {
                state: current
                    .session_state
                    .map(|state| format!("{state:?}"))
                    .unwrap_or_else(|| "uninitialized".to_string()),
                session_id: current.session_id,
                queued_runs: current.queued_runs,
                queue_items: current.queue_items,
                keep_alive: Some(current.keep_alive),
                last_error: current.last_error,
            })
        }

        async fn shutdown(&self) -> Result<(), EmilyError> {
            let session_id = { self.state.lock().await.session_id.clone() };
            if let Some(session_id) = session_id {
                self.client
                    .close_workflow_session(WorkflowSessionCloseRequest { session_id })
                    .await?;
            }
            let mut current = self.state.lock().await;
            current.session_id = None;
            current.session_state = None;
            current.queued_runs = None;
            current.queue_items = None;
            Ok(())
        }
    }

    fn io_contains_binding(nodes: &[WorkflowIoNode], binding: &WorkflowBinding) -> bool {
        nodes.iter().any(|node| {
            node.node_id == binding.node_id
                && node
                    .ports
                    .iter()
                    .any(|port| port.port_id == binding.port_id)
        })
    }

    fn parse_vector_value(value: &serde_json::Value) -> Result<Vec<f32>, EmilyError> {
        let array = value.as_array().ok_or_else(|| {
            EmilyError::Embedding("workflow output value is not an array".to_string())
        })?;
        let mut vector = Vec::with_capacity(array.len());
        for (index, item) in array.iter().enumerate() {
            let number = item.as_f64().ok_or_else(|| {
                EmilyError::Embedding(format!(
                    "workflow output vector item {} is not numeric",
                    index
                ))
            })?;
            if !number.is_finite() {
                return Err(EmilyError::Embedding(format!(
                    "workflow output vector item {} is not finite",
                    index
                )));
            }
            vector.push(number as f32);
        }
        Ok(vector)
    }

    pub use PantographWorkflowEmbeddingProvider as Provider;
    pub use WorkflowBinding as Binding;
    pub use WorkflowEmbeddingConfig as Config;
    pub use WorkflowServiceSessionClient as ServiceClient;
    pub use WorkflowSessionClient as SessionClient;

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::sync::Arc;

        use pantograph_workflow_service::{
            WorkflowInputTarget, WorkflowIoPort, WorkflowSessionCloseResponse,
            WorkflowSessionQueueListResponse, WorkflowSessionState, WorkflowSessionSummary,
        };

        struct MockClient {
            io: WorkflowIoResponse,
            preflight: WorkflowPreflightResponse,
            run_response: WorkflowRunResponse,
            create_calls: Mutex<u32>,
            run_calls: Mutex<u32>,
            close_calls: Mutex<u32>,
        }

        #[async_trait]
        impl WorkflowSessionClient for MockClient {
            async fn workflow_get_io(
                &self,
                _request: WorkflowIoRequest,
            ) -> Result<WorkflowIoResponse, EmilyError> {
                Ok(self.io.clone())
            }

            async fn workflow_preflight(
                &self,
                _request: WorkflowPreflightRequest,
            ) -> Result<WorkflowPreflightResponse, EmilyError> {
                Ok(self.preflight.clone())
            }

            async fn create_workflow_session(
                &self,
                _request: WorkflowSessionCreateRequest,
            ) -> Result<WorkflowSessionCreateResponse, EmilyError> {
                let mut calls = self.create_calls.lock().await;
                *calls += 1;
                Ok(WorkflowSessionCreateResponse {
                    session_id: "session-1".to_string(),
                })
            }

            async fn run_workflow_session(
                &self,
                _request: WorkflowSessionRunRequest,
            ) -> Result<WorkflowRunResponse, EmilyError> {
                let mut calls = self.run_calls.lock().await;
                *calls += 1;
                Ok(self.run_response.clone())
            }

            async fn workflow_get_session_status(
                &self,
                request: WorkflowSessionStatusRequest,
            ) -> Result<WorkflowSessionStatusResponse, EmilyError> {
                Ok(WorkflowSessionStatusResponse {
                    session: WorkflowSessionSummary {
                        session_id: request.session_id,
                        workflow_id: "wf-1".to_string(),
                        usage_profile: None,
                        keep_alive: true,
                        state: WorkflowSessionState::IdleLoaded,
                        queued_runs: 0,
                        run_count: 1,
                    },
                })
            }

            async fn workflow_list_session_queue(
                &self,
                request: WorkflowSessionQueueListRequest,
            ) -> Result<WorkflowSessionQueueListResponse, EmilyError> {
                Ok(WorkflowSessionQueueListResponse {
                    session_id: request.session_id,
                    items: Vec::new(),
                })
            }

            async fn workflow_set_session_keep_alive(
                &self,
                _request: WorkflowSessionKeepAliveRequest,
            ) -> Result<(), EmilyError> {
                Ok(())
            }

            async fn close_workflow_session(
                &self,
                _request: WorkflowSessionCloseRequest,
            ) -> Result<(), EmilyError> {
                let mut calls = self.close_calls.lock().await;
                *calls += 1;
                let _response = WorkflowSessionCloseResponse { ok: true };
                Ok(())
            }
        }

        fn mock_io() -> WorkflowIoResponse {
            WorkflowIoResponse {
                inputs: vec![WorkflowIoNode {
                    node_id: "text-input-1".to_string(),
                    node_type: "text-input".to_string(),
                    name: None,
                    description: None,
                    ports: vec![WorkflowIoPort {
                        port_id: "text".to_string(),
                        name: None,
                        description: None,
                        data_type: Some("string".to_string()),
                        required: Some(true),
                        multiple: Some(false),
                    }],
                }],
                outputs: vec![WorkflowIoNode {
                    node_id: "vector-output-1".to_string(),
                    node_type: "vector-output".to_string(),
                    name: None,
                    description: None,
                    ports: vec![WorkflowIoPort {
                        port_id: "vector".to_string(),
                        name: None,
                        description: None,
                        data_type: Some("embedding".to_string()),
                        required: Some(false),
                        multiple: Some(false),
                    }],
                }],
            }
        }

        fn mock_preflight() -> WorkflowPreflightResponse {
            WorkflowPreflightResponse {
                missing_required_inputs: Vec::<WorkflowInputTarget>::new(),
                invalid_targets: Vec::new(),
                warnings: Vec::new(),
                can_run: true,
            }
        }

        #[tokio::test]
        async fn workflow_provider_runs_session_and_extracts_vector() {
            let vector = vec![0.0_f32; 1024];
            let client = Arc::new(MockClient {
                io: mock_io(),
                preflight: mock_preflight(),
                run_response: WorkflowRunResponse {
                    run_id: "run-1".to_string(),
                    outputs: vec![WorkflowPortBinding {
                        node_id: "vector-output-1".to_string(),
                        port_id: "vector".to_string(),
                        value: serde_json::json!(vector),
                    }],
                    timing_ms: 1,
                },
                create_calls: Mutex::new(0),
                run_calls: Mutex::new(0),
                close_calls: Mutex::new(0),
            });

            let config = WorkflowEmbeddingConfig::new(
                "wf-1",
                WorkflowBinding::new("text-input-1", "text").expect("binding"),
                WorkflowBinding::new("vector-output-1", "vector").expect("binding"),
                Some(1_000),
                1024,
            )
            .expect("config");

            let provider =
                PantographWorkflowEmbeddingProvider::new(client.clone(), config).expect("provider");
            provider.validate().await.expect("validate");
            let embedded = provider.embed_text("hello").await.expect("embed");
            assert_eq!(embedded.len(), 1024);

            provider.shutdown().await.expect("shutdown");

            assert_eq!(*client.create_calls.lock().await, 1);
            assert_eq!(*client.run_calls.lock().await, 1);
            assert_eq!(*client.close_calls.lock().await, 1);
        }

        #[tokio::test]
        async fn workflow_provider_rejects_dimension_mismatch() {
            let client = Arc::new(MockClient {
                io: mock_io(),
                preflight: mock_preflight(),
                run_response: WorkflowRunResponse {
                    run_id: "run-1".to_string(),
                    outputs: vec![WorkflowPortBinding {
                        node_id: "vector-output-1".to_string(),
                        port_id: "vector".to_string(),
                        value: serde_json::json!([0.1, 0.2]),
                    }],
                    timing_ms: 1,
                },
                create_calls: Mutex::new(0),
                run_calls: Mutex::new(0),
                close_calls: Mutex::new(0),
            });

            let config = WorkflowEmbeddingConfig::new(
                "wf-1",
                WorkflowBinding::new("text-input-1", "text").expect("binding"),
                WorkflowBinding::new("vector-output-1", "vector").expect("binding"),
                None,
                1024,
            )
            .expect("config");
            let provider =
                PantographWorkflowEmbeddingProvider::new(client, config).expect("provider");
            let err = provider
                .embed_text("hello")
                .await
                .expect_err("dimension mismatch must fail");
            assert!(err.to_string().contains("expected 1024"));
        }
    }
}

#[cfg(feature = "pantograph")]
pub use pantograph::Provider as PantographEmbeddingProvider;
#[cfg(feature = "pantograph")]
pub use pantograph::{
    Binding as PantographWorkflowBinding, Config as PantographWorkflowEmbeddingConfig,
    ServiceClient as PantographWorkflowServiceClient,
    SessionClient as PantographWorkflowSessionClient,
};
