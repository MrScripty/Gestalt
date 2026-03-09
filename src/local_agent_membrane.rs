use crate::emily_bridge::EmilyBridge;
use crate::local_agent_context::PreparedLocalAgentCommand;
use emily_membrane::contracts::{
    ContextFragment, LocalExecutionPersistence, MembraneValidationDisposition,
    PolicyExecutionPersistence, RoutingPolicyOutcome, RoutingPolicyRequest, RoutingSensitivity,
};
use emily_membrane::runtime::MembraneRuntime;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const LOCAL_AGENT_MEMBRANE_TOGGLE_ENV: &str = "GESTALT_ENABLE_LOCAL_AGENT_MEMBRANE";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalAgentMembraneStatus {
    pub policy_outcome: RoutingPolicyOutcome,
    pub validation_disposition: Option<MembraneValidationDisposition>,
    pub caution: bool,
    pub reference_count: usize,
    pub route_decision_id: Option<String>,
    pub validation_id: Option<String>,
}

impl LocalAgentMembraneStatus {
    pub fn feedback_suffix(&self) -> String {
        match (self.policy_outcome, self.validation_disposition) {
            (RoutingPolicyOutcome::LocalOnly, Some(MembraneValidationDisposition::Accepted)) => {
                format!(
                    " Local-only membrane validation accepted with {} provenance references.",
                    self.reference_count
                )
            }
            (RoutingPolicyOutcome::LocalOnly, Some(MembraneValidationDisposition::NeedsReview)) => {
                format!(
                    " Local-only membrane review required with {} provenance references.",
                    self.reference_count
                )
            }
            (RoutingPolicyOutcome::LocalOnly, Some(MembraneValidationDisposition::Rejected)) => {
                format!(
                    " Local-only membrane rejected the reconstructed output after {} provenance references.",
                    self.reference_count
                )
            }
            (RoutingPolicyOutcome::LocalOnly, None) => {
                " Local-only membrane executed without a validation result.".to_string()
            }
            (RoutingPolicyOutcome::Rejected, _) => {
                " Local-only membrane policy rejected the task.".to_string()
            }
            (RoutingPolicyOutcome::SingleRemote, _) => {
                " Local-agent membrane unexpectedly selected a remote route.".to_string()
            }
        }
    }
}

pub fn local_agent_membrane_enabled() -> bool {
    matches!(
        std::env::var(LOCAL_AGENT_MEMBRANE_TOGGLE_ENV).as_deref(),
        Ok("1")
    )
}

pub fn local_agent_membrane_toggle_env() -> &'static str {
    LOCAL_AGENT_MEMBRANE_TOGGLE_ENV
}

pub async fn run_local_agent_membrane_pass(
    emily_bridge: Arc<EmilyBridge>,
    episode_id: &str,
    prepared: &PreparedLocalAgentCommand,
) -> Result<LocalAgentMembraneStatus, String> {
    let task_id = format!("{episode_id}:local-agent-membrane");
    let now = current_unix_ms();
    let runtime = MembraneRuntime::new(emily_bridge);
    let execution = runtime
        .execute_with_policy_and_record(
            emily_membrane::contracts::MembraneTaskRequest {
                task_id: task_id.clone(),
                episode_id: episode_id.to_string(),
                task_text: prepared.display_command.clone(),
                context_fragments: prepared
                    .context_fragments
                    .iter()
                    .map(|fragment| ContextFragment {
                        fragment_id: fragment.object_id.clone(),
                        text: fragment.text.clone(),
                    })
                    .collect(),
                allow_remote: false,
            },
            RoutingPolicyRequest {
                task_id,
                episode_id: episode_id.to_string(),
                allow_remote: false,
                sensitivity: RoutingSensitivity::Normal,
                preference: emily_membrane::contracts::RemoteRoutingPreference {
                    provider_id: None,
                    profile_id: None,
                    required_capability_tags: Vec::new(),
                    preferred_provider_classes: Vec::new(),
                    max_latency_class: None,
                    max_cost_class: None,
                    minimum_validation_compatibility: None,
                },
            },
            PolicyExecutionPersistence {
                local: Some(LocalExecutionPersistence {
                    route_decision_id: format!("{episode_id}:local-agent-membrane:route"),
                    route_decided_at_unix_ms: now,
                    validation_id: format!("{episode_id}:local-agent-membrane:validation"),
                    validated_at_unix_ms: now.saturating_add(1),
                }),
                remote: None,
            },
        )
        .await
        .map_err(|error| format!("Emily membrane execution failed: {error}"))?;

    let local_execution = execution.local_execution.as_ref();
    Ok(LocalAgentMembraneStatus {
        policy_outcome: execution.policy.outcome,
        validation_disposition: local_execution.map(|record| record.validation.disposition),
        caution: local_execution.is_some_and(|record| record.reconstruction.caution),
        reference_count: local_execution
            .map(|record| record.reconstruction.references.len())
            .unwrap_or(0),
        route_decision_id: local_execution.map(|record| record.route_decision_id.clone()),
        validation_id: local_execution.map(|record| record.validation_id.clone()),
    })
}

fn current_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as i64)
}

#[cfg(test)]
mod tests {
    use super::{LocalAgentMembraneStatus, local_agent_membrane_enabled};
    use emily_membrane::contracts::{MembraneValidationDisposition, RoutingPolicyOutcome};

    #[test]
    fn membrane_feedback_mentions_review_when_needed() {
        let status = LocalAgentMembraneStatus {
            policy_outcome: RoutingPolicyOutcome::LocalOnly,
            validation_disposition: Some(MembraneValidationDisposition::NeedsReview),
            caution: true,
            reference_count: 3,
            route_decision_id: Some("route".to_string()),
            validation_id: Some("validation".to_string()),
        };
        assert_eq!(
            status.feedback_suffix(),
            " Local-only membrane review required with 3 provenance references."
        );
    }

    #[test]
    fn toggle_defaults_to_disabled() {
        unsafe {
            std::env::remove_var("GESTALT_ENABLE_LOCAL_AGENT_MEMBRANE");
        }
        assert!(!local_agent_membrane_enabled());
    }
}
