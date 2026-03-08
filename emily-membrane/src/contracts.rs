//! Executable membrane boundary contracts.

use serde::{Deserialize, Serialize};

/// Host-provided context fragment already deemed safe for membrane use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextFragment {
    pub fragment_id: String,
    pub text: String,
}

/// Input task given to the membrane runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembraneTaskRequest {
    pub task_id: String,
    pub episode_id: String,
    pub task_text: String,
    /// Defaults to an empty list when omitted.
    #[serde(default)]
    pub context_fragments: Vec<ContextFragment>,
    /// Defaults to `false` when omitted.
    #[serde(default)]
    pub allow_remote: bool,
}

/// Bounded task prepared for local or remote execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledMembraneTask {
    pub task_id: String,
    pub episode_id: String,
    pub bounded_prompt: String,
    /// Defaults to an empty list when omitted.
    #[serde(default)]
    pub context_fragment_ids: Vec<String>,
}

/// Result of compiling a membrane task into a bounded execution unit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompileResult {
    pub compiled_task: CompiledMembraneTask,
    /// Defaults to `false` when omitted.
    #[serde(default)]
    pub truncated: bool,
}

/// Membrane routing decision for one compiled task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingPlan {
    pub task_id: String,
    pub decision: MembraneRouteKind,
    /// Defaults to an empty list when omitted.
    #[serde(default)]
    pub targets: Vec<RoutingTarget>,
    pub rationale: Option<String>,
}

/// Routing kind chosen by the membrane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MembraneRouteKind {
    LocalOnly,
    SingleRemote,
    MultiRemote,
    Rejected,
}

/// One potential execution target for a routing plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingTarget {
    pub target_id: String,
    pub capability_tag: String,
}

/// Host-facing routing preference for registry-backed remote target selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteRoutingPreference {
    pub provider_id: Option<String>,
    pub profile_id: Option<String>,
    /// Defaults to an empty list when omitted.
    #[serde(default)]
    pub required_capability_tags: Vec<String>,
}

/// Result of executing a routing plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DispatchResult {
    pub task_id: String,
    pub route: MembraneRouteKind,
    pub status: DispatchStatus,
    pub response_text: String,
    pub remote_reference: Option<String>,
}

/// Execution status for one dispatch attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DispatchStatus {
    LocalCompleted,
    RemoteDispatched,
    RemoteCompleted,
    Blocked,
}

/// Validation output produced by the membrane before local reconstruction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationEnvelope {
    pub task_id: String,
    pub disposition: MembraneValidationDisposition,
    /// Defaults to an empty list when omitted.
    #[serde(default)]
    pub findings: Vec<ValidationFinding>,
    pub validated_text: Option<String>,
}

/// High-level validation result for one membrane output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MembraneValidationDisposition {
    Accepted,
    NeedsReview,
    Rejected,
}

/// Human-readable validation finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationFinding {
    pub code: String,
    pub detail: String,
}

/// Final local reconstruction result returned to the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconstructionResult {
    pub task_id: String,
    pub output_text: String,
    /// Defaults to an empty list when omitted.
    #[serde(default)]
    pub references: Vec<ReconstructionReference>,
    /// Defaults to `false` when omitted.
    #[serde(default)]
    pub caution: bool,
}

/// Provenance reference captured during reconstruction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconstructionReference {
    pub source: ReconstructionSource,
    pub reference_id: String,
}

/// Source category for one reconstruction reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReconstructionSource {
    LocalContext,
    RemoteResult,
    ValidationPolicy,
}

/// Deterministic identifiers and timestamps used for local-only Emily writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalExecutionPersistence {
    pub route_decision_id: String,
    pub route_decided_at_unix_ms: i64,
    pub validation_id: String,
    pub validated_at_unix_ms: i64,
}

/// Combined local-only membrane execution result with Emily persistence ids.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalExecutionRecord {
    pub compile: CompileResult,
    pub route: RoutingPlan,
    pub dispatch: DispatchResult,
    pub validation: ValidationEnvelope,
    pub reconstruction: ReconstructionResult,
    pub route_decision_id: String,
    pub validation_id: String,
}

/// Deterministic identifiers and timestamps used for remote Emily writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteExecutionPersistence {
    pub route_decision_id: String,
    pub route_decided_at_unix_ms: i64,
    pub provider_request_id: String,
    pub remote_episode_id: String,
    pub remote_dispatched_at_unix_ms: i64,
    pub validation_id: String,
    pub validated_at_unix_ms: i64,
}

/// Combined remote membrane execution result with Emily persistence ids.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteExecutionRecord {
    pub compile: CompileResult,
    pub route: RoutingPlan,
    pub dispatch: DispatchResult,
    pub validation: ValidationEnvelope,
    pub reconstruction: ReconstructionResult,
    pub provider_request_id: String,
    pub route_decision_id: String,
    pub remote_episode_id: String,
    pub validation_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn membrane_task_request_roundtrip_preserves_defaults() {
        let request = MembraneTaskRequest {
            task_id: "task-1".into(),
            episode_id: "episode-1".into(),
            task_text: "Summarize the recent outcome.".into(),
            context_fragments: vec![ContextFragment {
                fragment_id: "ctx-1".into(),
                text: "recent context".into(),
            }],
            allow_remote: false,
        };

        let text = serde_json::to_string(&request).expect("serialize membrane task request");
        let restored: MembraneTaskRequest =
            serde_json::from_str(&text).expect("deserialize membrane task request");

        assert_eq!(restored, request);

        let restored_default: MembraneTaskRequest = serde_json::from_str(
            r#"{"task_id":"task-2","episode_id":"episode-2","task_text":"local only"}"#,
        )
        .expect("deserialize membrane task request defaults");
        assert!(restored_default.context_fragments.is_empty());
        assert!(!restored_default.allow_remote);
    }

    #[test]
    fn compile_result_roundtrip_preserves_defaults() {
        let result = CompileResult {
            compiled_task: CompiledMembraneTask {
                task_id: "task-1".into(),
                episode_id: "episode-1".into(),
                bounded_prompt: "bounded prompt".into(),
                context_fragment_ids: vec!["ctx-1".into()],
            },
            truncated: true,
        };

        let text = serde_json::to_string(&result).expect("serialize compile result");
        let restored: CompileResult =
            serde_json::from_str(&text).expect("deserialize compile result");
        assert_eq!(restored, result);

        let restored_default: CompileResult = serde_json::from_str(
            r#"{"compiled_task":{"task_id":"task-2","episode_id":"episode-2","bounded_prompt":"prompt"}}"#,
        )
        .expect("deserialize compile result defaults");
        assert!(
            restored_default
                .compiled_task
                .context_fragment_ids
                .is_empty()
        );
        assert!(!restored_default.truncated);
    }

    #[test]
    fn routing_plan_roundtrip_preserves_targets() {
        let plan = RoutingPlan {
            task_id: "task-1".into(),
            decision: MembraneRouteKind::SingleRemote,
            targets: vec![RoutingTarget {
                target_id: "provider-a".into(),
                capability_tag: "reasoning".into(),
            }],
            rationale: Some("remote allowed for reasoning task".into()),
        };

        let text = serde_json::to_string(&plan).expect("serialize routing plan");
        let restored: RoutingPlan = serde_json::from_str(&text).expect("deserialize routing plan");
        assert_eq!(restored, plan);
    }

    #[test]
    fn remote_routing_preference_roundtrip_preserves_defaults() {
        let preference = RemoteRoutingPreference {
            provider_id: Some("provider-a".into()),
            profile_id: Some("reasoning".into()),
            required_capability_tags: vec!["analysis".into()],
        };

        let text = serde_json::to_string(&preference).expect("serialize remote routing preference");
        let restored: RemoteRoutingPreference =
            serde_json::from_str(&text).expect("deserialize remote routing preference");
        assert_eq!(restored, preference);

        let restored_default: RemoteRoutingPreference =
            serde_json::from_str(r#"{"provider_id":"provider-b","profile_id":null}"#)
                .expect("deserialize remote routing preference defaults");
        assert!(restored_default.required_capability_tags.is_empty());
    }

    #[test]
    fn dispatch_result_roundtrip_preserves_remote_reference() {
        let result = DispatchResult {
            task_id: "task-1".into(),
            route: MembraneRouteKind::SingleRemote,
            status: DispatchStatus::RemoteDispatched,
            response_text: "accepted for remote execution".into(),
            remote_reference: Some("remote-episode-1".into()),
        };

        let text = serde_json::to_string(&result).expect("serialize dispatch result");
        let restored: DispatchResult =
            serde_json::from_str(&text).expect("deserialize dispatch result");
        assert_eq!(restored, result);
    }

    #[test]
    fn validation_envelope_roundtrip_preserves_defaults() {
        let envelope = ValidationEnvelope {
            task_id: "task-1".into(),
            disposition: MembraneValidationDisposition::NeedsReview,
            findings: vec![ValidationFinding {
                code: "review-required".into(),
                detail: "local review required before host output".into(),
            }],
            validated_text: None,
        };

        let text = serde_json::to_string(&envelope).expect("serialize validation envelope");
        let restored: ValidationEnvelope =
            serde_json::from_str(&text).expect("deserialize validation envelope");
        assert_eq!(restored, envelope);

        let restored_default: ValidationEnvelope = serde_json::from_str(
            r#"{"task_id":"task-2","disposition":"Accepted","validated_text":"safe"}"#,
        )
        .expect("deserialize validation defaults");
        assert!(restored_default.findings.is_empty());
    }

    #[test]
    fn reconstruction_result_roundtrip_preserves_defaults() {
        let result = ReconstructionResult {
            task_id: "task-1".into(),
            output_text: "final response".into(),
            references: vec![ReconstructionReference {
                source: ReconstructionSource::LocalContext,
                reference_id: "ctx-1".into(),
            }],
            caution: true,
        };

        let text = serde_json::to_string(&result).expect("serialize reconstruction result");
        let restored: ReconstructionResult =
            serde_json::from_str(&text).expect("deserialize reconstruction result");
        assert_eq!(restored, result);

        let restored_default: ReconstructionResult =
            serde_json::from_str(r#"{"task_id":"task-2","output_text":"plain response"}"#)
                .expect("deserialize reconstruction defaults");
        assert!(restored_default.references.is_empty());
        assert!(!restored_default.caution);
    }

    #[test]
    fn local_execution_contracts_roundtrip() {
        let record = LocalExecutionRecord {
            compile: CompileResult {
                compiled_task: CompiledMembraneTask {
                    task_id: "task-1".into(),
                    episode_id: "episode-1".into(),
                    bounded_prompt: "bounded prompt".into(),
                    context_fragment_ids: vec!["ctx-1".into()],
                },
                truncated: false,
            },
            route: RoutingPlan {
                task_id: "task-1".into(),
                decision: MembraneRouteKind::LocalOnly,
                targets: Vec::new(),
                rationale: Some("local runtime".into()),
            },
            dispatch: DispatchResult {
                task_id: "task-1".into(),
                route: MembraneRouteKind::LocalOnly,
                status: DispatchStatus::LocalCompleted,
                response_text: "LOCAL: bounded prompt".into(),
                remote_reference: None,
            },
            validation: ValidationEnvelope {
                task_id: "task-1".into(),
                disposition: MembraneValidationDisposition::Accepted,
                findings: Vec::new(),
                validated_text: Some("LOCAL: bounded prompt".into()),
            },
            reconstruction: ReconstructionResult {
                task_id: "task-1".into(),
                output_text: "LOCAL: bounded prompt".into(),
                references: Vec::new(),
                caution: false,
            },
            route_decision_id: "route-1".into(),
            validation_id: "validation-1".into(),
        };

        let text = serde_json::to_string(&record).expect("serialize local execution record");
        let restored: LocalExecutionRecord =
            serde_json::from_str(&text).expect("deserialize local execution record");
        assert_eq!(restored, record);

        let persistence = LocalExecutionPersistence {
            route_decision_id: "route-1".into(),
            route_decided_at_unix_ms: 10,
            validation_id: "validation-1".into(),
            validated_at_unix_ms: 11,
        };
        let text =
            serde_json::to_string(&persistence).expect("serialize local execution persistence");
        let restored: LocalExecutionPersistence =
            serde_json::from_str(&text).expect("deserialize local execution persistence");
        assert_eq!(restored, persistence);
    }

    #[test]
    fn remote_execution_contracts_roundtrip() {
        let record = RemoteExecutionRecord {
            compile: CompileResult {
                compiled_task: CompiledMembraneTask {
                    task_id: "task-1".into(),
                    episode_id: "episode-1".into(),
                    bounded_prompt: "bounded prompt".into(),
                    context_fragment_ids: vec!["ctx-1".into()],
                },
                truncated: false,
            },
            route: RoutingPlan {
                task_id: "task-1".into(),
                decision: MembraneRouteKind::SingleRemote,
                targets: vec![RoutingTarget {
                    target_id: "provider-a".into(),
                    capability_tag: "analysis".into(),
                }],
                rationale: Some("remote route".into()),
            },
            dispatch: DispatchResult {
                task_id: "task-1".into(),
                route: MembraneRouteKind::SingleRemote,
                status: DispatchStatus::RemoteCompleted,
                response_text: "REMOTE: bounded prompt".into(),
                remote_reference: Some("remote-1".into()),
            },
            validation: ValidationEnvelope {
                task_id: "task-1".into(),
                disposition: MembraneValidationDisposition::Accepted,
                findings: Vec::new(),
                validated_text: Some("REMOTE: bounded prompt".into()),
            },
            reconstruction: ReconstructionResult {
                task_id: "task-1".into(),
                output_text: "REMOTE: bounded prompt".into(),
                references: Vec::new(),
                caution: false,
            },
            provider_request_id: "provider-request-1".into(),
            route_decision_id: "route-1".into(),
            remote_episode_id: "remote-1".into(),
            validation_id: "validation-1".into(),
        };

        let text = serde_json::to_string(&record).expect("serialize remote execution record");
        let restored: RemoteExecutionRecord =
            serde_json::from_str(&text).expect("deserialize remote execution record");
        assert_eq!(restored, record);

        let persistence = RemoteExecutionPersistence {
            route_decision_id: "route-1".into(),
            route_decided_at_unix_ms: 10,
            provider_request_id: "provider-request-1".into(),
            remote_episode_id: "remote-1".into(),
            remote_dispatched_at_unix_ms: 11,
            validation_id: "validation-1".into(),
            validated_at_unix_ms: 12,
        };
        let text =
            serde_json::to_string(&persistence).expect("serialize remote execution persistence");
        let restored: RemoteExecutionPersistence =
            serde_json::from_str(&text).expect("deserialize remote execution persistence");
        assert_eq!(restored, persistence);
    }
}
