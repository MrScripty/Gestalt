use crate::error::EmilyError;
use crate::inference::{EmbeddingProvider, EmbeddingProviderStatus};
use async_trait::async_trait;
use pantograph_workflow_service::{
    WorkflowIoNode, WorkflowIoRequest, WorkflowOutputTarget, WorkflowPortBinding,
    WorkflowPreflightRequest, WorkflowRunResponse, WorkflowSessionCloseRequest,
    WorkflowSessionCreateRequest, WorkflowSessionKeepAliveRequest, WorkflowSessionQueueListRequest,
    WorkflowSessionStatusRequest,
};
use std::sync::Arc;
use tokio::sync::Mutex;

use super::client::{
    WorkflowBinding, WorkflowEmbeddingConfig, WorkflowSessionClient, WorkflowState,
};

#[derive(Debug, Default, Clone)]
struct ProviderRuntimeState {
    session_id: Option<String>,
    session_state: Option<WorkflowState>,
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
            .run_workflow_session(pantograph_workflow_service::WorkflowSessionRunRequest {
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
