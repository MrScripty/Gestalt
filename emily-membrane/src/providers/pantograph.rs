use super::{
    MembraneProvider, MembraneProviderError, ProviderDispatchRequest, ProviderDispatchResult,
    ProviderDispatchStatus,
};
use async_trait::async_trait;
use pantograph_workflow_service::{
    WorkflowHost, WorkflowOutputTarget, WorkflowPortBinding, WorkflowRunRequest, WorkflowService,
    WorkflowServiceError,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::sync::Arc;

/// One bindable Pantograph node/port pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PantographWorkflowBinding {
    pub node_id: String,
    pub port_id: String,
}

impl PantographWorkflowBinding {
    pub fn new(
        node_id: impl Into<String>,
        port_id: impl Into<String>,
    ) -> Result<Self, MembraneProviderError> {
        let node_id = node_id.into();
        let port_id = port_id.into();
        if node_id.trim().is_empty() {
            return Err(MembraneProviderError::InvalidRequest(
                "pantograph binding node_id cannot be empty".to_string(),
            ));
        }
        if port_id.trim().is_empty() {
            return Err(MembraneProviderError::InvalidRequest(
                "pantograph binding port_id cannot be empty".to_string(),
            ));
        }
        Ok(Self { node_id, port_id })
    }
}

/// Static configuration for one-shot Pantograph workflow dispatch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PantographProviderConfig {
    pub provider_id: String,
    pub workflow_id: String,
    pub text_input: PantographWorkflowBinding,
    pub default_output_targets: Vec<PantographWorkflowBinding>,
    pub timeout_ms: Option<u64>,
}

impl PantographProviderConfig {
    pub fn new(
        provider_id: impl Into<String>,
        workflow_id: impl Into<String>,
        text_input: PantographWorkflowBinding,
        default_output_targets: Vec<PantographWorkflowBinding>,
        timeout_ms: Option<u64>,
    ) -> Result<Self, MembraneProviderError> {
        let provider_id = provider_id.into();
        let workflow_id = workflow_id.into();
        if provider_id.trim().is_empty() {
            return Err(MembraneProviderError::InvalidRequest(
                "provider_id cannot be empty".to_string(),
            ));
        }
        if workflow_id.trim().is_empty() {
            return Err(MembraneProviderError::InvalidRequest(
                "workflow_id cannot be empty".to_string(),
            ));
        }
        if default_output_targets.is_empty() {
            return Err(MembraneProviderError::InvalidRequest(
                "default_output_targets cannot be empty".to_string(),
            ));
        }
        if matches!(timeout_ms, Some(0)) {
            return Err(MembraneProviderError::InvalidRequest(
                "timeout_ms must be greater than zero when provided".to_string(),
            ));
        }
        Ok(Self {
            provider_id,
            workflow_id,
            text_input,
            default_output_targets,
            timeout_ms,
        })
    }
}

/// One-shot Pantograph provider adapter over `WorkflowService`.
pub struct PantographWorkflowProvider<H: WorkflowHost> {
    service: WorkflowService,
    host: Arc<H>,
    config: PantographProviderConfig,
}

impl<H: WorkflowHost> PantographWorkflowProvider<H> {
    pub fn new(
        host: Arc<H>,
        config: PantographProviderConfig,
    ) -> Result<Self, MembraneProviderError> {
        if config.provider_id.trim().is_empty() {
            return Err(MembraneProviderError::InvalidRequest(
                "provider_id cannot be empty".to_string(),
            ));
        }
        Ok(Self {
            service: WorkflowService::new(),
            host,
            config,
        })
    }
}

#[async_trait]
impl<H: WorkflowHost> MembraneProvider for PantographWorkflowProvider<H> {
    fn provider_id(&self) -> &str {
        &self.config.provider_id
    }

    async fn dispatch(
        &self,
        request: ProviderDispatchRequest,
    ) -> Result<ProviderDispatchResult, MembraneProviderError> {
        validate_dispatch_request(&request, self.provider_id())?;
        let settings = resolve_request_settings(&request, &self.config)?;
        let response = self
            .service
            .workflow_run(
                self.host.as_ref(),
                WorkflowRunRequest {
                    workflow_id: settings.workflow_id.clone(),
                    inputs: vec![WorkflowPortBinding {
                        node_id: self.config.text_input.node_id.clone(),
                        port_id: self.config.text_input.port_id.clone(),
                        value: json!(request.bounded_payload),
                    }],
                    output_targets: Some(
                        settings
                            .output_targets
                            .iter()
                            .map(|binding| WorkflowOutputTarget {
                                node_id: binding.node_id.clone(),
                                port_id: binding.port_id.clone(),
                            })
                            .collect(),
                    ),
                    timeout_ms: settings.timeout_ms,
                    run_id: Some(request.provider_request_id.clone()),
                },
            )
            .await
            .map_err(map_workflow_service_error)?;

        let output_text = response_output_text(&response.outputs)?;
        Ok(ProviderDispatchResult {
            provider_request_id: request.provider_request_id,
            provider_id: self.provider_id().to_string(),
            status: ProviderDispatchStatus::Completed,
            output_text,
            metadata: build_result_metadata(&response, &settings),
        })
    }
}

#[derive(Debug, Clone)]
struct PantographRequestSettings {
    workflow_id: String,
    output_targets: Vec<PantographWorkflowBinding>,
    timeout_ms: Option<u64>,
    requested_priority: Option<i32>,
}

fn validate_dispatch_request(
    request: &ProviderDispatchRequest,
    expected_provider_id: &str,
) -> Result<(), MembraneProviderError> {
    if request.provider_request_id.trim().is_empty() {
        return Err(MembraneProviderError::InvalidRequest(
            "provider_request_id cannot be empty".to_string(),
        ));
    }
    if request.bounded_payload.trim().is_empty() {
        return Err(MembraneProviderError::InvalidRequest(
            "bounded_payload cannot be empty".to_string(),
        ));
    }
    if request.target.provider_id != expected_provider_id {
        return Err(MembraneProviderError::InvalidRequest(format!(
            "provider target '{}' does not match adapter '{}'",
            request.target.provider_id, expected_provider_id
        )));
    }
    Ok(())
}

fn resolve_request_settings(
    request: &ProviderDispatchRequest,
    config: &PantographProviderConfig,
) -> Result<PantographRequestSettings, MembraneProviderError> {
    let metadata = request.metadata.get("pantograph");
    let workflow_id = metadata
        .and_then(|value| value.get("workflow_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(config.workflow_id.as_str())
        .to_string();

    let timeout_ms = match metadata.and_then(|value| value.get("timeout_ms")) {
        Some(Value::Number(number)) => {
            let timeout_ms = number.as_u64().ok_or_else(|| {
                MembraneProviderError::InvalidRequest(
                    "pantograph.timeout_ms must be an unsigned integer".to_string(),
                )
            })?;
            if timeout_ms == 0 {
                return Err(MembraneProviderError::InvalidRequest(
                    "pantograph.timeout_ms must be greater than zero".to_string(),
                ));
            }
            Some(timeout_ms)
        }
        Some(_) => {
            return Err(MembraneProviderError::InvalidRequest(
                "pantograph.timeout_ms must be a number".to_string(),
            ));
        }
        None => config.timeout_ms,
    };

    let requested_priority = match metadata.and_then(|value| value.get("priority")) {
        Some(Value::Number(number)) => Some(number.as_i64().ok_or_else(|| {
            MembraneProviderError::InvalidRequest(
                "pantograph.priority must be an integer".to_string(),
            )
        })? as i32),
        Some(_) => {
            return Err(MembraneProviderError::InvalidRequest(
                "pantograph.priority must be a number".to_string(),
            ));
        }
        None => None,
    };

    if let Some(priority) = requested_priority
        && priority != 0
    {
        return Err(MembraneProviderError::InvalidRequest(
            "pantograph.priority requires a future session-backed adapter; one-shot dispatch only supports 0".to_string(),
        ));
    }

    let output_targets = match metadata.and_then(|value| value.get("output_targets")) {
        Some(Value::Array(items)) => {
            if items.is_empty() {
                return Err(MembraneProviderError::InvalidRequest(
                    "pantograph.output_targets cannot be empty".to_string(),
                ));
            }
            let mut bindings = Vec::with_capacity(items.len());
            for item in items {
                let node_id = item.get("node_id").and_then(Value::as_str).ok_or_else(|| {
                    MembraneProviderError::InvalidRequest(
                        "pantograph.output_targets items require string node_id".to_string(),
                    )
                })?;
                let port_id = item.get("port_id").and_then(Value::as_str).ok_or_else(|| {
                    MembraneProviderError::InvalidRequest(
                        "pantograph.output_targets items require string port_id".to_string(),
                    )
                })?;
                bindings.push(PantographWorkflowBinding::new(node_id, port_id)?);
            }
            bindings
        }
        Some(_) => {
            return Err(MembraneProviderError::InvalidRequest(
                "pantograph.output_targets must be an array".to_string(),
            ));
        }
        None => config.default_output_targets.clone(),
    };

    Ok(PantographRequestSettings {
        workflow_id,
        output_targets,
        timeout_ms,
        requested_priority,
    })
}

fn response_output_text(outputs: &[WorkflowPortBinding]) -> Result<String, MembraneProviderError> {
    if outputs.is_empty() {
        return Err(MembraneProviderError::Execution(
            "workflow run returned no outputs".to_string(),
        ));
    }
    if outputs.len() == 1 {
        return binding_value_text(&outputs[0].value);
    }

    let mut mapped = Map::new();
    for output in outputs {
        mapped.insert(
            format!("{}:{}", output.node_id, output.port_id),
            output.value.clone(),
        );
    }
    serde_json::to_string(&Value::Object(mapped)).map_err(|error| {
        MembraneProviderError::Execution(format!(
            "failed to serialize multi-output workflow result: {error}"
        ))
    })
}

fn binding_value_text(value: &Value) -> Result<String, MembraneProviderError> {
    match value {
        Value::String(text) => Ok(text.clone()),
        other => serde_json::to_string(other).map_err(|error| {
            MembraneProviderError::Execution(format!(
                "failed to serialize workflow output value: {error}"
            ))
        }),
    }
}

fn build_result_metadata(
    response: &pantograph_workflow_service::WorkflowRunResponse,
    settings: &PantographRequestSettings,
) -> Value {
    json!({
        "pantograph": {
            "run_id": response.run_id,
            "timing_ms": response.timing_ms,
            "workflow_id": settings.workflow_id,
            "timeout_ms": settings.timeout_ms,
            "requested_priority": settings.requested_priority,
            "priority_applied": settings.requested_priority.unwrap_or(0) == 0,
            "output_targets": settings.output_targets.iter().map(|binding| json!({
                "node_id": binding.node_id,
                "port_id": binding.port_id,
            })).collect::<Vec<_>>(),
        }
    })
}

fn map_workflow_service_error(error: WorkflowServiceError) -> MembraneProviderError {
    match error {
        WorkflowServiceError::InvalidRequest(message)
        | WorkflowServiceError::CapabilityViolation(message)
        | WorkflowServiceError::OutputNotProduced(message) => {
            MembraneProviderError::InvalidRequest(message)
        }
        WorkflowServiceError::RuntimeNotReady(message)
        | WorkflowServiceError::SchedulerBusy(message) => {
            MembraneProviderError::Unavailable(message)
        }
        WorkflowServiceError::WorkflowNotFound(message)
        | WorkflowServiceError::SessionNotFound(message)
        | WorkflowServiceError::SessionEvicted(message)
        | WorkflowServiceError::QueueItemNotFound(message)
        | WorkflowServiceError::RuntimeTimeout(message)
        | WorkflowServiceError::Internal(message) => MembraneProviderError::Execution(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pantograph_workflow_service::{
        WorkflowCapabilityModel, WorkflowHostCapabilities, WorkflowIoNode, WorkflowIoPort,
        WorkflowIoResponse, WorkflowRunHandle, WorkflowRunOptions, WorkflowRuntimeRequirements,
    };

    struct ExampleHost;

    #[async_trait]
    impl WorkflowHost for ExampleHost {
        async fn validate_workflow(&self, workflow_id: &str) -> Result<(), WorkflowServiceError> {
            if workflow_id.trim().is_empty() {
                return Err(WorkflowServiceError::WorkflowNotFound(
                    "workflow_id is empty".to_string(),
                ));
            }
            Ok(())
        }

        async fn workflow_capabilities(
            &self,
            _workflow_id: &str,
        ) -> Result<WorkflowHostCapabilities, WorkflowServiceError> {
            Ok(WorkflowHostCapabilities {
                max_input_bindings: 8,
                max_output_targets: 8,
                max_value_bytes: 4096,
                runtime_requirements: WorkflowRuntimeRequirements {
                    estimation_confidence: "estimated".to_string(),
                    ..WorkflowRuntimeRequirements::default()
                },
                models: vec![WorkflowCapabilityModel {
                    model_id: "demo-model".to_string(),
                    model_revision_or_hash: None,
                    model_type: Some("generation".to_string()),
                    node_ids: vec!["text-output-1".to_string()],
                    roles: vec!["generation".to_string()],
                }],
            })
        }

        async fn workflow_io(
            &self,
            _workflow_id: &str,
        ) -> Result<WorkflowIoResponse, WorkflowServiceError> {
            Ok(WorkflowIoResponse {
                inputs: vec![WorkflowIoNode {
                    node_id: "text-input-1".to_string(),
                    node_type: "text-input".to_string(),
                    name: Some("Prompt".to_string()),
                    description: Some("Prompt text input".to_string()),
                    ports: vec![WorkflowIoPort {
                        port_id: "text".to_string(),
                        name: Some("Text".to_string()),
                        description: None,
                        data_type: Some("string".to_string()),
                        required: Some(true),
                        multiple: Some(false),
                    }],
                }],
                outputs: vec![
                    WorkflowIoNode {
                        node_id: "text-output-1".to_string(),
                        node_type: "text-output".to_string(),
                        name: Some("Answer".to_string()),
                        description: Some("Text answer output".to_string()),
                        ports: vec![WorkflowIoPort {
                            port_id: "text".to_string(),
                            name: Some("Text".to_string()),
                            description: None,
                            data_type: Some("string".to_string()),
                            required: Some(false),
                            multiple: Some(false),
                        }],
                    },
                    WorkflowIoNode {
                        node_id: "custom-output".to_string(),
                        node_type: "text-output".to_string(),
                        name: Some("Custom Answer".to_string()),
                        description: Some("Custom text answer output".to_string()),
                        ports: vec![WorkflowIoPort {
                            port_id: "answer".to_string(),
                            name: Some("Answer".to_string()),
                            description: None,
                            data_type: Some("string".to_string()),
                            required: Some(false),
                            multiple: Some(false),
                        }],
                    },
                ],
            })
        }

        async fn run_workflow(
            &self,
            _workflow_id: &str,
            inputs: &[WorkflowPortBinding],
            output_targets: Option<&[WorkflowOutputTarget]>,
            _run_options: WorkflowRunOptions,
            _run_handle: WorkflowRunHandle,
        ) -> Result<Vec<WorkflowPortBinding>, WorkflowServiceError> {
            let input_text = inputs
                .first()
                .and_then(|binding| binding.value.as_str())
                .unwrap_or_default();
            let target = output_targets
                .and_then(|targets| targets.first())
                .ok_or_else(|| {
                    WorkflowServiceError::InvalidRequest("missing output target".to_string())
                })?;

            Ok(vec![WorkflowPortBinding {
                node_id: target.node_id.clone(),
                port_id: target.port_id.clone(),
                value: json!(format!("PANTOGRAPH: {input_text}")),
            }])
        }
    }

    fn config() -> PantographProviderConfig {
        PantographProviderConfig::new(
            "pantograph",
            "workflow-a",
            PantographWorkflowBinding::new("text-input-1", "text").expect("binding"),
            vec![PantographWorkflowBinding::new("text-output-1", "text").expect("binding")],
            Some(5000),
        )
        .expect("config")
    }

    fn request() -> ProviderDispatchRequest {
        ProviderDispatchRequest {
            provider_request_id: "provider-request-1".to_string(),
            task_id: "task-1".to_string(),
            episode_id: "episode-1".to_string(),
            target: crate::providers::ProviderTarget {
                provider_id: "pantograph".to_string(),
                model_id: Some("demo-model".to_string()),
                profile_id: Some("reasoning".to_string()),
                capability_tags: vec!["generation".to_string()],
                metadata: json!({}),
            },
            dispatch_kind: crate::providers::ProviderDispatchKind::Prompt,
            membrane_ir: Some(crate::contracts::MembraneIr {
                task: crate::contracts::MembraneTaskPayload {
                    task_id: "task-1".to_string(),
                    episode_id: "episode-1".to_string(),
                    text: "hello".to_string(),
                },
                context_handles: vec![crate::contracts::MembraneContextHandle {
                    fragment_id: "ctx-1".to_string(),
                    text: "provider context".to_string(),
                }],
                boundary: crate::contracts::MembraneBoundaryMetadata {
                    remote_allowed: true,
                    render_mode: crate::contracts::MembraneIrRenderMode::PromptV1,
                },
                reconstruction: None,
            }),
            bounded_payload: "hello".to_string(),
            context_fragment_ids: vec!["ctx-1".to_string()],
            metadata: json!({}),
        }
    }

    #[tokio::test]
    async fn pantograph_provider_dispatches_one_shot_workflow() {
        let provider =
            PantographWorkflowProvider::new(Arc::new(ExampleHost), config()).expect("provider");
        let result = provider.dispatch(request()).await.expect("dispatch");

        assert_eq!(result.provider_id, "pantograph");
        assert_eq!(result.status, ProviderDispatchStatus::Completed);
        assert_eq!(result.output_text, "PANTOGRAPH: hello");
        assert_eq!(result.metadata["pantograph"]["workflow_id"], "workflow-a");
    }

    #[tokio::test]
    async fn pantograph_provider_supports_output_target_override_metadata() {
        let provider =
            PantographWorkflowProvider::new(Arc::new(ExampleHost), config()).expect("provider");
        let mut request = request();
        request.metadata = json!({
            "pantograph": {
                "output_targets": [
                    {"node_id": "custom-output", "port_id": "answer"}
                ],
                "timeout_ms": 7000,
            }
        });

        let result = provider.dispatch(request).await.expect("dispatch");
        assert_eq!(result.metadata["pantograph"]["timeout_ms"], 7000);
        assert_eq!(
            result.metadata["pantograph"]["output_targets"][0]["node_id"],
            "custom-output"
        );
    }

    #[tokio::test]
    async fn pantograph_provider_rejects_nonzero_priority_for_one_shot_path() {
        let provider =
            PantographWorkflowProvider::new(Arc::new(ExampleHost), config()).expect("provider");
        let mut request = request();
        request.metadata = json!({
            "pantograph": {
                "priority": 1
            }
        });

        let error = provider
            .dispatch(request)
            .await
            .expect_err("priority should fail");
        assert!(matches!(error, MembraneProviderError::InvalidRequest(_)));
    }
}
