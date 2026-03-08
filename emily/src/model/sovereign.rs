use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Request to record one bounded remote reasoning episode under a host episode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteEpisodeRequest {
    pub remote_episode_id: String,
    pub episode_id: String,
    pub route_decision_id: Option<String>,
    pub dispatch_kind: String,
    pub dispatched_at_unix_ms: i64,
    pub metadata: Value,
}

/// Durable remote reasoning episode reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteEpisodeRecord {
    pub id: String,
    pub episode_id: String,
    pub route_decision_id: Option<String>,
    pub dispatch_kind: String,
    pub state: RemoteEpisodeState,
    pub dispatched_at_unix_ms: i64,
    pub completed_at_unix_ms: Option<i64>,
    pub metadata: Value,
}

/// Lifecycle state for one remote reasoning episode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoteEpisodeState {
    Planned,
    Dispatched,
    Succeeded,
    Failed,
    Cancelled,
    Rejected,
}

/// Host-agnostic route target used for later sovereign dispatch policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutingTarget {
    pub provider_id: String,
    pub model_id: Option<String>,
    pub profile_id: Option<String>,
    pub capability_tags: Vec<String>,
    pub metadata: Value,
}

/// Durable routing decision for one host episode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutingDecision {
    pub decision_id: String,
    pub episode_id: String,
    pub kind: RoutingDecisionKind,
    pub decided_at_unix_ms: i64,
    pub rationale: Option<String>,
    pub targets: Vec<RoutingTarget>,
    pub metadata: Value,
}

/// Route-shape decision without committing to one membrane or provider runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoutingDecisionKind {
    LocalOnly,
    SingleRemote,
    MultiRemote,
    Rejected,
}

/// Result of validating one remote or reconstructed output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationOutcome {
    pub validation_id: String,
    pub episode_id: String,
    pub remote_episode_id: Option<String>,
    pub decision: ValidationDecision,
    pub validated_at_unix_ms: i64,
    pub findings: Vec<ValidationFinding>,
    pub metadata: Value,
}

/// Host-agnostic validation disposition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationDecision {
    Accepted,
    AcceptedWithCaution,
    NeedsReview,
    Rejected,
}

/// One validation finding attached to a validation outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationFinding {
    pub code: String,
    pub severity: ValidationFindingSeverity,
    pub message: String,
}

/// Severity label for a validation finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationFindingSeverity {
    Info,
    Warning,
    Error,
}

/// Structured audit metadata for sovereign-preparation records.
///
/// This stays intentionally generic and avoids defining membrane IR or provider
/// transport contracts inside the core crate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SovereignAuditMetadata {
    pub remote_episode_id: Option<String>,
    pub route_decision_id: Option<String>,
    pub validation_id: Option<String>,
    pub boundary_profile: Option<String>,
    pub metadata: Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sovereign_contracts_roundtrip_json() {
        let route = RoutingDecision {
            decision_id: "route-1".to_string(),
            episode_id: "ep-1".to_string(),
            kind: RoutingDecisionKind::SingleRemote,
            decided_at_unix_ms: 10,
            rationale: Some("specialized code reasoning".to_string()),
            targets: vec![RoutingTarget {
                provider_id: "provider-a".to_string(),
                model_id: Some("model-x".to_string()),
                profile_id: Some("reasoning".to_string()),
                capability_tags: vec!["code".to_string(), "analysis".to_string()],
                metadata: json!({"priority": 1}),
            }],
            metadata: json!({"source": "planner"}),
        };
        let text = serde_json::to_string(&route).expect("serialize routing decision");
        let restored: RoutingDecision =
            serde_json::from_str(&text).expect("deserialize routing decision");
        assert_eq!(route, restored);

        let validation = ValidationOutcome {
            validation_id: "val-1".to_string(),
            episode_id: "ep-1".to_string(),
            remote_episode_id: Some("remote-1".to_string()),
            decision: ValidationDecision::AcceptedWithCaution,
            validated_at_unix_ms: 12,
            findings: vec![ValidationFinding {
                code: "uncertain_reference".to_string(),
                severity: ValidationFindingSeverity::Warning,
                message: "cross-check before integration".to_string(),
            }],
            metadata: json!({"checker": "eccr"}),
        };
        let text = serde_json::to_string(&validation).expect("serialize validation outcome");
        let restored: ValidationOutcome =
            serde_json::from_str(&text).expect("deserialize validation outcome");
        assert_eq!(validation, restored);
    }
}
