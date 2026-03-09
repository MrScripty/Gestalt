use crate::emily_bridge::EmilyBridge;
use crate::local_agent_context::PreparedLocalAgentCommand;
use crate::pantograph_host::build_membrane_provider_registry_from_env;
use emily_membrane::contracts::{
    ContextFragment, LocalExecutionPersistence, MembraneValidationDisposition,
    PolicyExecutionPersistence, RemoteExecutionPersistence, RoutingPolicyOutcome,
    RoutingPolicyRequest, RoutingSensitivity,
};
use emily_membrane::providers::MembraneProviderRegistry;
use emily_membrane::runtime::MembraneRuntime;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const LOCAL_AGENT_MEMBRANE_TOGGLE_ENV: &str = "GESTALT_ENABLE_LOCAL_AGENT_MEMBRANE";
const LOCAL_AGENT_REMOTE_MEMBRANE_TOGGLE_ENV: &str = "GESTALT_ENABLE_LOCAL_AGENT_REMOTE_MEMBRANE";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalAgentMembraneStatus {
    pub policy_outcome: RoutingPolicyOutcome,
    pub validation_disposition: Option<MembraneValidationDisposition>,
    pub caution: bool,
    pub executed_remote: bool,
    pub reference_count: usize,
    pub route_decision_id: Option<String>,
    pub validation_id: Option<String>,
    pub remote_episode_id: Option<String>,
    pub fallback_reason: Option<String>,
}

impl LocalAgentMembraneStatus {
    pub fn feedback_suffix(&self) -> String {
        if let Some(reason) = self.fallback_reason.as_ref() {
            return match self.validation_disposition {
                Some(MembraneValidationDisposition::Accepted) => format!(
                    " Remote membrane fallback to local-only after {reason}. Local-only membrane validation accepted with {} provenance references.",
                    self.reference_count
                ),
                Some(MembraneValidationDisposition::NeedsReview) => format!(
                    " Remote membrane fallback to local-only after {reason}. Local-only membrane review required with {} provenance references.",
                    self.reference_count
                ),
                Some(MembraneValidationDisposition::Rejected) => format!(
                    " Remote membrane fallback to local-only after {reason}. Local-only membrane rejected the reconstructed output after {} provenance references.",
                    self.reference_count
                ),
                None => format!(" Remote membrane fallback to local-only after {reason}."),
            };
        }

        if self.executed_remote {
            return match self.validation_disposition {
                Some(MembraneValidationDisposition::Accepted) => format!(
                    " Remote membrane validation accepted for episode {} with {} provenance references.",
                    self.remote_episode_id.as_deref().unwrap_or("unknown"),
                    self.reference_count
                ),
                Some(MembraneValidationDisposition::NeedsReview) => format!(
                    " Remote membrane review required for episode {} with {} provenance references.",
                    self.remote_episode_id.as_deref().unwrap_or("unknown"),
                    self.reference_count
                ),
                Some(MembraneValidationDisposition::Rejected) => format!(
                    " Remote membrane rejected the output for episode {}.",
                    self.remote_episode_id.as_deref().unwrap_or("unknown"),
                ),
                None => " Remote membrane executed without a validation result.".to_string(),
            };
        }

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
                " Local-agent membrane policy rejected the task.".to_string()
            }
            (RoutingPolicyOutcome::SingleRemote, _) => {
                " Local-agent membrane selected a remote route without executing it.".to_string()
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

pub fn local_agent_remote_membrane_enabled() -> bool {
    matches!(
        std::env::var(LOCAL_AGENT_REMOTE_MEMBRANE_TOGGLE_ENV).as_deref(),
        Ok("1")
    )
}

pub fn local_agent_remote_membrane_toggle_env() -> &'static str {
    LOCAL_AGENT_REMOTE_MEMBRANE_TOGGLE_ENV
}

pub async fn run_local_agent_membrane_pass(
    emily_bridge: Arc<EmilyBridge>,
    episode_id: &str,
    prepared: &PreparedLocalAgentCommand,
) -> Result<LocalAgentMembraneStatus, String> {
    if !local_agent_remote_membrane_enabled() {
        return execute_local_only(emily_bridge, episode_id, prepared, None).await;
    }

    let registry = tokio::task::spawn_blocking(build_membrane_provider_registry_from_env)
        .await
        .map_err(|error| format!("failed joining membrane provider registry bootstrap: {error}"))?
        .map_err(|error| format!("Pantograph membrane registry bootstrap failed: {error}"))?;

    match registry {
        Some(registry) => {
            execute_with_registry(emily_bridge, episode_id, prepared, registry, true).await
        }
        None => {
            execute_local_only(
                emily_bridge,
                episode_id,
                prepared,
                Some("Pantograph reasoning registry is unavailable".to_string()),
            )
            .await
        }
    }
}

pub async fn run_local_agent_membrane_pass_with_registry(
    emily_bridge: Arc<EmilyBridge>,
    episode_id: &str,
    prepared: &PreparedLocalAgentCommand,
    provider_registry: Option<Arc<dyn MembraneProviderRegistry>>,
    allow_remote: bool,
) -> Result<LocalAgentMembraneStatus, String> {
    match (allow_remote, provider_registry) {
        (true, Some(provider_registry)) => {
            execute_with_registry(emily_bridge, episode_id, prepared, provider_registry, true).await
        }
        (true, None) => {
            execute_local_only(
                emily_bridge,
                episode_id,
                prepared,
                Some("remote membrane requested without a provider registry".to_string()),
            )
            .await
        }
        (false, _) => execute_local_only(emily_bridge, episode_id, prepared, None).await,
    }
}

async fn execute_with_registry(
    emily_bridge: Arc<EmilyBridge>,
    episode_id: &str,
    prepared: &PreparedLocalAgentCommand,
    provider_registry: Arc<dyn MembraneProviderRegistry>,
    allow_remote: bool,
) -> Result<LocalAgentMembraneStatus, String> {
    let task_id = membrane_task_id(episode_id);
    let runtime = MembraneRuntime::with_provider_registry(emily_bridge.clone(), provider_registry);
    let request = task_request(&task_id, episode_id, prepared, allow_remote);
    let policy_request = policy_request(&task_id, episode_id, allow_remote);
    let persistence = policy_persistence(episode_id, allow_remote, false);

    match runtime
        .execute_with_policy_and_record(request, policy_request, persistence)
        .await
    {
        Ok(execution) => Ok(status_from_execution(execution, None)),
        Err(error) if allow_remote => {
            let fallback_reason = error.to_string();
            execute_local_only(emily_bridge, episode_id, prepared, Some(fallback_reason)).await
        }
        Err(error) => Err(format!("Emily membrane execution failed: {error}")),
    }
}

async fn execute_local_only(
    emily_bridge: Arc<EmilyBridge>,
    episode_id: &str,
    prepared: &PreparedLocalAgentCommand,
    fallback_reason: Option<String>,
) -> Result<LocalAgentMembraneStatus, String> {
    let task_id = membrane_task_id(episode_id);
    let runtime = MembraneRuntime::new(emily_bridge);
    let execution = runtime
        .execute_with_policy_and_record(
            task_request(&task_id, episode_id, prepared, false),
            policy_request(&task_id, episode_id, false),
            policy_persistence(episode_id, false, fallback_reason.is_some()),
        )
        .await
        .map_err(|error| format!("Emily membrane execution failed: {error}"))?;
    Ok(status_from_execution(execution, fallback_reason))
}

fn status_from_execution(
    execution: emily_membrane::contracts::PolicySelectedExecution,
    fallback_reason: Option<String>,
) -> LocalAgentMembraneStatus {
    let local_execution = execution.local_execution.as_ref();
    let remote_execution = execution.remote_execution.as_ref();
    let active_validation = remote_execution
        .map(|record| &record.validation)
        .or_else(|| local_execution.map(|record| &record.validation));
    let active_reconstruction = remote_execution
        .map(|record| &record.reconstruction)
        .or_else(|| local_execution.map(|record| &record.reconstruction));

    LocalAgentMembraneStatus {
        policy_outcome: execution.policy.outcome,
        validation_disposition: active_validation.map(|validation| validation.disposition),
        caution: active_reconstruction.is_some_and(|reconstruction| reconstruction.caution),
        executed_remote: remote_execution.is_some(),
        reference_count: active_reconstruction
            .map(|reconstruction| reconstruction.references.len())
            .unwrap_or(0),
        route_decision_id: remote_execution
            .map(|record| record.route_decision_id.clone())
            .or_else(|| local_execution.map(|record| record.route_decision_id.clone())),
        validation_id: remote_execution
            .map(|record| record.validation_id.clone())
            .or_else(|| local_execution.map(|record| record.validation_id.clone())),
        remote_episode_id: remote_execution.map(|record| record.remote_episode_id.clone()),
        fallback_reason,
    }
}

fn task_request(
    task_id: &str,
    episode_id: &str,
    prepared: &PreparedLocalAgentCommand,
    allow_remote: bool,
) -> emily_membrane::contracts::MembraneTaskRequest {
    emily_membrane::contracts::MembraneTaskRequest {
        task_id: task_id.to_string(),
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
        allow_remote,
    }
}

fn policy_request(task_id: &str, episode_id: &str, allow_remote: bool) -> RoutingPolicyRequest {
    RoutingPolicyRequest {
        task_id: task_id.to_string(),
        episode_id: episode_id.to_string(),
        allow_remote,
        sensitivity: RoutingSensitivity::Normal,
        preference: emily_membrane::contracts::RemoteRoutingPreference {
            provider_id: None,
            profile_id: None,
            required_capability_tags: vec!["analysis".to_string()],
            preferred_provider_classes: Vec::new(),
            max_latency_class: None,
            max_cost_class: None,
            minimum_validation_compatibility: None,
        },
    }
}

fn policy_persistence(
    episode_id: &str,
    allow_remote: bool,
    fallback: bool,
) -> PolicyExecutionPersistence {
    let now = current_unix_ms();
    let mode_prefix = if fallback {
        "local-fallback"
    } else if allow_remote {
        "remote"
    } else {
        "local"
    };

    PolicyExecutionPersistence {
        local: Some(LocalExecutionPersistence {
            route_decision_id: format!("{episode_id}:local-agent-membrane:{mode_prefix}:route"),
            route_decided_at_unix_ms: now,
            validation_id: format!("{episode_id}:local-agent-membrane:{mode_prefix}:validation"),
            validated_at_unix_ms: now.saturating_add(1),
        }),
        remote: allow_remote.then_some(RemoteExecutionPersistence {
            route_decision_id: format!("{episode_id}:local-agent-membrane:remote:route"),
            route_decided_at_unix_ms: now,
            provider_request_id: format!(
                "{episode_id}:local-agent-membrane:remote:provider-request"
            ),
            remote_episode_id: format!("{episode_id}:local-agent-membrane:remote:episode"),
            remote_dispatched_at_unix_ms: now.saturating_add(1),
            validation_id: format!("{episode_id}:local-agent-membrane:remote:validation"),
            validated_at_unix_ms: now.saturating_add(2),
        }),
    }
}

fn membrane_task_id(episode_id: &str) -> String {
    format!("{episode_id}:local-agent-membrane")
}

fn current_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as i64)
}

#[cfg(test)]
mod tests {
    use super::{
        LocalAgentMembraneStatus, local_agent_membrane_enabled, local_agent_membrane_toggle_env,
        local_agent_remote_membrane_enabled, local_agent_remote_membrane_toggle_env,
    };
    use emily_membrane::contracts::{MembraneValidationDisposition, RoutingPolicyOutcome};

    #[test]
    fn membrane_feedback_mentions_review_when_needed() {
        let status = LocalAgentMembraneStatus {
            policy_outcome: RoutingPolicyOutcome::LocalOnly,
            validation_disposition: Some(MembraneValidationDisposition::NeedsReview),
            caution: true,
            executed_remote: false,
            reference_count: 3,
            route_decision_id: Some("route".to_string()),
            validation_id: Some("validation".to_string()),
            remote_episode_id: None,
            fallback_reason: None,
        };
        assert_eq!(
            status.feedback_suffix(),
            " Local-only membrane review required with 3 provenance references."
        );
    }

    #[test]
    fn membrane_feedback_mentions_remote_fallback() {
        let status = LocalAgentMembraneStatus {
            policy_outcome: RoutingPolicyOutcome::SingleRemote,
            validation_disposition: Some(MembraneValidationDisposition::Accepted),
            caution: false,
            executed_remote: false,
            reference_count: 2,
            route_decision_id: Some("route".to_string()),
            validation_id: Some("validation".to_string()),
            remote_episode_id: None,
            fallback_reason: Some("runtime timeout".to_string()),
        };
        assert_eq!(
            status.feedback_suffix(),
            " Remote membrane fallback to local-only after runtime timeout. Local-only membrane validation accepted with 2 provenance references."
        );
    }

    #[test]
    fn toggles_default_to_disabled() {
        unsafe {
            std::env::remove_var(local_agent_membrane_toggle_env());
            std::env::remove_var(local_agent_remote_membrane_toggle_env());
        }
        assert!(!local_agent_membrane_enabled());
        assert!(!local_agent_remote_membrane_enabled());
    }
}
