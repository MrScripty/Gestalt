use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Minimum viable host-provided EARL signal vector for pre-cognitive gating.
///
/// These fields are normalized local risk proxies in the range `[0.0, 1.0]`.
/// They intentionally avoid Gestalt-specific UI assumptions while giving the
/// runtime enough structure to make deterministic `OK / CAUTION / REFLEX`
/// decisions before a learned manifold exists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EarlSignalVector {
    pub uncertainty: f32,
    pub conflict: f32,
    pub continuity_drift: f32,
    pub constraint_pressure: f32,
    pub tool_instability: f32,
    pub novelty_spike: f32,
}

/// Request to evaluate one episode through the current EARL runtime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EarlEvaluationRequest {
    pub evaluation_id: String,
    pub episode_id: String,
    pub evaluated_at_unix_ms: i64,
    pub signals: EarlSignalVector,
    pub metadata: Value,
}

/// Persisted EARL evaluation result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EarlEvaluationRecord {
    pub id: String,
    pub episode_id: String,
    pub evaluated_at_unix_ms: i64,
    pub signals: EarlSignalVector,
    pub risk_score: f32,
    pub decision: EarlDecision,
    pub host_action: EarlHostAction,
    pub retryable: bool,
    pub rationale: String,
    pub metadata: Value,
}

/// EARL gate decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EarlDecision {
    Ok,
    Caution,
    Reflex,
}

/// Host-facing next step implied by one EARL decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EarlHostAction {
    Proceed,
    Clarify,
    Abort,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn earl_contracts_roundtrip_json() {
        let request = EarlEvaluationRequest {
            evaluation_id: "earl-1".to_string(),
            episode_id: "ep-1".to_string(),
            evaluated_at_unix_ms: 1,
            signals: EarlSignalVector {
                uncertainty: 0.2,
                conflict: 0.1,
                continuity_drift: 0.0,
                constraint_pressure: 0.3,
                tool_instability: 0.2,
                novelty_spike: 0.4,
            },
            metadata: json!({"origin": "test"}),
        };
        let text = serde_json::to_string(&request).expect("serialize earl request");
        let restored: EarlEvaluationRequest =
            serde_json::from_str(&text).expect("deserialize earl request");
        assert_eq!(request, restored);
    }
}
