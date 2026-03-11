use super::RemoteExecutionRecord;
use serde::{Deserialize, Serialize};

/// Bounded request-scoped retry policy for remote membrane execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteRetryPolicy {
    pub max_attempts: u8,
    /// Defaults to `false` when omitted.
    #[serde(default)]
    pub retry_on_provider_error: bool,
    /// Defaults to `false` when omitted.
    #[serde(default)]
    pub retry_on_validation_review: bool,
    /// Defaults to `None` when omitted.
    #[serde(default)]
    pub mutation: RetryMutationStrategy,
}

/// Mutation strategy applied between bounded retry attempts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RetryMutationStrategy {
    #[default]
    None,
    AppendRetryHintV1,
}

/// Deterministic per-attempt persistence payload for request-scoped retries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteRetryAttemptPersistence {
    pub provider_request_id: String,
    pub remote_episode_id: String,
    pub remote_dispatched_at_unix_ms: i64,
    pub validation_id: String,
    pub validated_at_unix_ms: i64,
    pub retry_audit_id: Option<String>,
    pub retry_audit_at_unix_ms: Option<i64>,
    pub mutation_audit_id: Option<String>,
    pub mutation_audit_at_unix_ms: Option<i64>,
}

/// Top-level deterministic persistence payload for one retrying remote flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteRetryExecutionPersistence {
    pub route_decision_id: String,
    pub route_decided_at_unix_ms: i64,
    /// Defaults to an empty list when omitted.
    #[serde(default)]
    pub attempts: Vec<RemoteRetryAttemptPersistence>,
}

/// One bounded retry attempt and its observed outcome.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteRetryAttemptRecord {
    pub attempt_index: u8,
    pub provider_request_id: String,
    pub remote_episode_id: String,
    pub retry_reason: Option<RetryReason>,
    pub execution: Option<RemoteExecutionRecord>,
    pub provider_error: Option<String>,
}

/// Reason the membrane chose to issue another bounded retry attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetryReason {
    ProviderError,
    ValidationReview,
}

/// Aggregated result for a request-scoped retrying remote membrane flow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteRetryExecutionRecord {
    pub policy: RemoteRetryPolicy,
    /// Defaults to an empty list when omitted.
    #[serde(default)]
    pub attempts: Vec<RemoteRetryAttemptRecord>,
    pub final_execution: Option<RemoteExecutionRecord>,
    /// Defaults to `false` when omitted.
    #[serde(default)]
    pub exhausted: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{
        CompileResult, CompiledMembraneTask, DispatchResult, DispatchStatus,
        MembraneBoundaryMetadata, MembraneIr, MembraneIrRenderMode, MembraneRouteKind,
        MembraneTaskPayload, MembraneValidationDisposition, ReconstructionResult, RoutingPlan,
        ValidationAssessment, ValidationAssessmentStatus, ValidationCategory, ValidationEnvelope,
    };

    #[test]
    fn remote_retry_policy_roundtrip_preserves_defaults() {
        let policy = RemoteRetryPolicy {
            max_attempts: 3,
            retry_on_provider_error: true,
            retry_on_validation_review: true,
            mutation: RetryMutationStrategy::AppendRetryHintV1,
        };

        let text = serde_json::to_string(&policy).expect("serialize retry policy");
        let restored: RemoteRetryPolicy =
            serde_json::from_str(&text).expect("deserialize retry policy");
        assert_eq!(restored, policy);

        let restored_default: RemoteRetryPolicy = serde_json::from_str(r#"{"max_attempts":1}"#)
            .expect("deserialize retry policy defaults");
        assert!(!restored_default.retry_on_provider_error);
        assert!(!restored_default.retry_on_validation_review);
        assert_eq!(restored_default.mutation, RetryMutationStrategy::None);
    }

    #[test]
    fn remote_retry_execution_record_roundtrip() {
        let record = RemoteRetryExecutionRecord {
            policy: RemoteRetryPolicy {
                max_attempts: 2,
                retry_on_provider_error: true,
                retry_on_validation_review: true,
                mutation: RetryMutationStrategy::AppendRetryHintV1,
            },
            attempts: vec![RemoteRetryAttemptRecord {
                attempt_index: 1,
                provider_request_id: "provider-request-1".into(),
                remote_episode_id: "remote-1".into(),
                retry_reason: Some(RetryReason::ValidationReview),
                execution: Some(RemoteExecutionRecord {
                    compile: CompileResult {
                        compiled_task: CompiledMembraneTask {
                            task_id: "task-1".into(),
                            episode_id: "episode-1".into(),
                            membrane_ir: Some(MembraneIr {
                                task: MembraneTaskPayload {
                                    task_id: "task-1".into(),
                                    episode_id: "episode-1".into(),
                                    text: "retry task".into(),
                                },
                                context_handles: Vec::new(),
                                protected_references: Vec::new(),
                                boundary: MembraneBoundaryMetadata {
                                    remote_allowed: true,
                                    render_mode: MembraneIrRenderMode::PromptV1,
                                },
                                reconstruction: None,
                            }),
                            bounded_prompt: "retry task".into(),
                            context_fragment_ids: Vec::new(),
                        },
                        truncated: false,
                    },
                    route: RoutingPlan {
                        task_id: "task-1".into(),
                        decision: MembraneRouteKind::SingleRemote,
                        targets: Vec::new(),
                        rationale: Some("retry route".into()),
                    },
                    dispatch: DispatchResult {
                        task_id: "task-1".into(),
                        route: MembraneRouteKind::SingleRemote,
                        status: DispatchStatus::RemoteCompleted,
                        response_text: "REMOTE: retry task".into(),
                        remote_reference: Some("remote-1".into()),
                    },
                    validation: ValidationEnvelope {
                        task_id: "task-1".into(),
                        disposition: MembraneValidationDisposition::NeedsReview,
                        assessments: vec![ValidationAssessment {
                            category: ValidationCategory::Confidence,
                            status: ValidationAssessmentStatus::NeedsReview,
                            summary: "first attempt needs review".into(),
                        }],
                        findings: Vec::new(),
                        validated_text: Some("REMOTE: retry task".into()),
                    },
                    reconstruction: ReconstructionResult {
                        task_id: "task-1".into(),
                        output_text: "REMOTE: retry task".into(),
                        references: Vec::new(),
                        caution: true,
                    },
                    provider_request_id: "provider-request-1".into(),
                    route_decision_id: "route-1".into(),
                    remote_episode_id: "remote-1".into(),
                    validation_id: "validation-1".into(),
                }),
                provider_error: None,
            }],
            final_execution: None,
            exhausted: true,
        };

        let text = serde_json::to_string(&record).expect("serialize retry execution record");
        let restored: RemoteRetryExecutionRecord =
            serde_json::from_str(&text).expect("deserialize retry execution record");
        assert_eq!(restored, record);
    }
}
