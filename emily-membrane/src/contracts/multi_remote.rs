use super::{CompileResult, ReconstructionResult, RemoteExecutionRecord, RoutingPlan};
use crate::providers::ProviderTarget;
use serde::{Deserialize, Serialize};

/// Bounded request-scoped policy for one multi-target membrane execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiRemoteExecutionPolicy {
    pub max_targets: u8,
    /// Defaults to `ExhaustTargets` when omitted.
    #[serde(default)]
    pub stop_condition: MultiRemoteStopCondition,
    /// Defaults to `FirstAcceptedElseNeedsReview` when omitted.
    #[serde(default)]
    pub reconciliation: MultiRemoteReconciliationMode,
}

/// Explicit stop rule for bounded multi-target fanout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MultiRemoteStopCondition {
    #[default]
    ExhaustTargets,
    StopOnAccepted,
}

/// Deterministic local rule for reconciling multiple remote attempts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MultiRemoteReconciliationMode {
    #[default]
    FirstAcceptedElseNeedsReview,
}

/// Deterministic identifiers and timestamps used for one multi-target write
/// path under a shared route decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiRemoteAttemptPersistence {
    pub provider_request_id: String,
    pub remote_episode_id: String,
    pub remote_dispatched_at_unix_ms: i64,
    pub validation_id: String,
    pub validated_at_unix_ms: i64,
}

/// Top-level persistence payload for one multi-target membrane execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiRemoteExecutionPersistence {
    pub route_decision_id: String,
    pub route_decided_at_unix_ms: i64,
    /// Defaults to an empty list when omitted.
    #[serde(default)]
    pub attempts: Vec<MultiRemoteAttemptPersistence>,
    pub reconciliation_audit_id: String,
    pub reconciled_at_unix_ms: i64,
}

/// Attempt lifecycle status inside one multi-target execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MultiRemoteAttemptStatus {
    Executed,
    Skipped,
}

/// Explicit reason a remote attempt was skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MultiRemoteSkipReason {
    StopConditionSatisfied,
}

/// One target attempt inside a multi-target membrane execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MultiRemoteAttemptRecord {
    pub target: ProviderTarget,
    pub provider_request_id: String,
    pub remote_episode_id: String,
    pub validation_id: Option<String>,
    pub validation_disposition: Option<super::MembraneValidationDisposition>,
    pub status: MultiRemoteAttemptStatus,
    pub skip_reason: Option<MultiRemoteSkipReason>,
    pub execution: Option<RemoteExecutionRecord>,
    pub provider_error: Option<String>,
}

/// Final deterministic reconciliation decision for multi-target fanout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MultiRemoteReconciliationDecision {
    Accepted,
    NeedsReview,
    NoResult,
}

/// Reconciled local result chosen from one multi-target run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MultiRemoteReconciliationRecord {
    pub decision: MultiRemoteReconciliationDecision,
    pub selected_target_id: Option<String>,
    pub selected_remote_episode_id: Option<String>,
    pub selected_validation_id: Option<String>,
    pub reconstruction: Option<ReconstructionResult>,
    pub summary: String,
}

/// Aggregated result for one multi-target membrane execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MultiRemoteExecutionRecord {
    pub compile: CompileResult,
    pub route: RoutingPlan,
    pub policy: MultiRemoteExecutionPolicy,
    /// Defaults to an empty list when omitted.
    #[serde(default)]
    pub attempts: Vec<MultiRemoteAttemptRecord>,
    pub reconciliation: MultiRemoteReconciliationRecord,
    pub route_decision_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{
        CompileResult, CompiledMembraneTask, MembraneBoundaryMetadata, MembraneIr,
        MembraneIrRenderMode, MembraneRouteKind, MembraneTaskPayload,
        MembraneValidationDisposition, ReconstructionResult, RoutingPlan, RoutingTarget,
    };
    use serde_json::json;

    #[test]
    fn multi_remote_execution_policy_roundtrip_preserves_defaults() {
        let policy = MultiRemoteExecutionPolicy {
            max_targets: 3,
            stop_condition: MultiRemoteStopCondition::StopOnAccepted,
            reconciliation: MultiRemoteReconciliationMode::FirstAcceptedElseNeedsReview,
        };

        let text = serde_json::to_string(&policy).expect("serialize multi remote policy");
        let restored: MultiRemoteExecutionPolicy =
            serde_json::from_str(&text).expect("deserialize multi remote policy");
        assert_eq!(restored, policy);

        let restored_default: MultiRemoteExecutionPolicy =
            serde_json::from_str(r#"{"max_targets":2}"#)
                .expect("deserialize multi remote policy defaults");
        assert_eq!(
            restored_default.stop_condition,
            MultiRemoteStopCondition::ExhaustTargets
        );
        assert_eq!(
            restored_default.reconciliation,
            MultiRemoteReconciliationMode::FirstAcceptedElseNeedsReview
        );
    }

    #[test]
    fn multi_remote_execution_record_roundtrip() {
        let record = MultiRemoteExecutionRecord {
            compile: CompileResult {
                compiled_task: CompiledMembraneTask {
                    task_id: "task-1".into(),
                    episode_id: "episode-1".into(),
                    membrane_ir: Some(MembraneIr {
                        task: MembraneTaskPayload {
                            task_id: "task-1".into(),
                            episode_id: "episode-1".into(),
                            text: "run fanout".into(),
                        },
                        context_handles: vec![crate::contracts::MembraneContextHandle {
                            fragment_id: "ctx-1".into(),
                            text: "recent context".into(),
                        }],
                        boundary: MembraneBoundaryMetadata {
                            remote_allowed: true,
                            render_mode: MembraneIrRenderMode::PromptV1,
                        },
                        reconstruction: None,
                    }),
                    bounded_prompt: "run fanout".into(),
                    context_fragment_ids: vec!["ctx-1".into()],
                },
                truncated: false,
            },
            route: RoutingPlan {
                task_id: "task-1".into(),
                decision: MembraneRouteKind::MultiRemote,
                targets: vec![
                    RoutingTarget {
                        target_id: "provider-a:model-a".into(),
                        capability_tag: "analysis".into(),
                    },
                    RoutingTarget {
                        target_id: "provider-b:model-b".into(),
                        capability_tag: "analysis".into(),
                    },
                ],
                rationale: Some("fan out to two providers".into()),
            },
            policy: MultiRemoteExecutionPolicy {
                max_targets: 2,
                stop_condition: MultiRemoteStopCondition::ExhaustTargets,
                reconciliation: MultiRemoteReconciliationMode::FirstAcceptedElseNeedsReview,
            },
            attempts: vec![MultiRemoteAttemptRecord {
                target: ProviderTarget {
                    provider_id: "provider-a".into(),
                    model_id: Some("model-a".into()),
                    profile_id: Some("reasoning".into()),
                    capability_tags: vec!["analysis".into()],
                    metadata: json!({}),
                },
                provider_request_id: "provider-request-1".into(),
                remote_episode_id: "remote-1".into(),
                validation_id: Some("validation-1".into()),
                validation_disposition: Some(MembraneValidationDisposition::Accepted),
                status: MultiRemoteAttemptStatus::Executed,
                skip_reason: None,
                execution: None,
                provider_error: None,
            }],
            reconciliation: MultiRemoteReconciliationRecord {
                decision: MultiRemoteReconciliationDecision::Accepted,
                selected_target_id: Some("provider-a:model-a".into()),
                selected_remote_episode_id: Some("remote-1".into()),
                selected_validation_id: Some("validation-1".into()),
                reconstruction: Some(ReconstructionResult {
                    task_id: "task-1".into(),
                    output_text: "accepted output".into(),
                    references: Vec::new(),
                    caution: false,
                }),
                summary: "selected first accepted remote result".into(),
            },
            route_decision_id: "route-1".into(),
        };

        let text = serde_json::to_string(&record).expect("serialize multi remote execution record");
        let restored: MultiRemoteExecutionRecord =
            serde_json::from_str(&text).expect("deserialize multi remote execution record");
        assert_eq!(restored, record);
    }

    #[test]
    fn multi_remote_attempt_persistence_roundtrip() {
        let persistence = MultiRemoteExecutionPersistence {
            route_decision_id: "route-1".into(),
            route_decided_at_unix_ms: 10,
            attempts: vec![MultiRemoteAttemptPersistence {
                provider_request_id: "provider-request-1".into(),
                remote_episode_id: "remote-1".into(),
                remote_dispatched_at_unix_ms: 11,
                validation_id: "validation-1".into(),
                validated_at_unix_ms: 12,
            }],
            reconciliation_audit_id: "audit-reconcile-1".into(),
            reconciled_at_unix_ms: 13,
        };

        let text = serde_json::to_string(&persistence).expect("serialize multi remote persistence");
        let restored: MultiRemoteExecutionPersistence =
            serde_json::from_str(&text).expect("deserialize multi remote persistence");
        assert_eq!(restored, persistence);
    }
}
