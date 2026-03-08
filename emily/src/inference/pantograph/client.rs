use crate::error::EmilyError;
use async_trait::async_trait;
use pantograph_workflow_service::{
    WorkflowHost, WorkflowIoRequest, WorkflowIoResponse, WorkflowPreflightRequest,
    WorkflowPreflightResponse, WorkflowRunResponse, WorkflowService, WorkflowServiceError,
    WorkflowSessionCloseRequest, WorkflowSessionCreateRequest, WorkflowSessionCreateResponse,
    WorkflowSessionKeepAliveRequest, WorkflowSessionQueueListRequest,
    WorkflowSessionQueueListResponse, WorkflowSessionRunRequest, WorkflowSessionState,
    WorkflowSessionStatusRequest, WorkflowSessionStatusResponse,
};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowBinding {
    pub node_id: String,
    pub port_id: String,
}

impl WorkflowBinding {
    pub fn new(node_id: impl Into<String>, port_id: impl Into<String>) -> Result<Self, EmilyError> {
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

pub(crate) type WorkflowState = WorkflowSessionState;
