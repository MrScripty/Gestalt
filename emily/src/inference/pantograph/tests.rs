use super::client::{WorkflowBinding, WorkflowEmbeddingConfig, WorkflowSessionClient};
use super::provider::PantographWorkflowEmbeddingProvider;
use crate::error::EmilyError;
use async_trait::async_trait;
use pantograph_workflow_service::{
    WorkflowInputTarget, WorkflowIoNode, WorkflowIoPort, WorkflowIoRequest, WorkflowIoResponse,
    WorkflowPortBinding, WorkflowPreflightRequest, WorkflowPreflightResponse, WorkflowRunResponse,
    WorkflowSessionCloseRequest, WorkflowSessionCloseResponse, WorkflowSessionCreateRequest,
    WorkflowSessionCreateResponse, WorkflowSessionKeepAliveRequest,
    WorkflowSessionQueueListRequest, WorkflowSessionQueueListResponse, WorkflowSessionRunRequest,
    WorkflowSessionState, WorkflowSessionStatusRequest, WorkflowSessionStatusResponse,
    WorkflowSessionSummary,
};
use std::sync::Arc;
use tokio::sync::Mutex;

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
    let provider = PantographWorkflowEmbeddingProvider::new(client, config).expect("provider");
    let err = provider
        .embed_text("hello")
        .await
        .expect_err("dimension mismatch must fail");
    assert!(err.to_string().contains("expected 1024"));
}
