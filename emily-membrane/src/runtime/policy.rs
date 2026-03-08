use super::MembraneRuntimeError;
use crate::contracts::{
    RoutingPolicyFinding, RoutingPolicyFindingSeverity, RoutingPolicyOutcome, RoutingPolicyRequest,
    RoutingPolicyResult, RoutingSensitivity,
};
use crate::providers::{ProviderTarget, RegisteredProviderTarget};
use emily::{EarlDecision, EarlEvaluationRecord, EpisodeRecord, EpisodeState};

const SCORE_PROVIDER_HINT_MATCH: i32 = 100;
const SCORE_PROFILE_HINT_MATCH: i32 = 50;
const SCORE_REQUIRED_CAPABILITY_TAG: i32 = 20;
const SCORE_ADDITIONAL_CAPABILITY_TAG: i32 = 2;
const SCORE_MODEL_PRESENT: i32 = 1;

#[derive(Debug, Clone)]
struct EmilyRoutingPolicySnapshot {
    episode: EpisodeRecord,
    latest_earl: Option<EarlEvaluationRecord>,
}

impl<A> super::MembraneRuntime<A>
where
    A: emily::EmilyApi + ?Sized,
{
    /// Evaluate deterministic routing policy against the injected provider
    /// registry without executing any provider dispatch.
    pub async fn evaluate_routing_policy(
        &self,
        request: RoutingPolicyRequest,
    ) -> Result<RoutingPolicyResult, MembraneRuntimeError> {
        validate_routing_policy_request(&request)?;

        if !request.allow_remote {
            return Ok(build_policy_result(
                &request,
                RoutingPolicyOutcome::LocalOnly,
                false,
                None,
                vec![RoutingPolicyFinding {
                    code: "remote-disabled".to_string(),
                    severity: RoutingPolicyFindingSeverity::Info,
                    detail: "task contract does not allow remote dispatch".to_string(),
                }],
                Some("remote dispatch disabled by task contract".to_string()),
            ));
        }

        if request.sensitivity == RoutingSensitivity::Critical {
            return Ok(build_policy_result(
                &request,
                RoutingPolicyOutcome::Rejected,
                false,
                None,
                vec![RoutingPolicyFinding {
                    code: "critical-sensitivity".to_string(),
                    severity: RoutingPolicyFindingSeverity::Block,
                    detail: "critical-sensitivity tasks are not eligible for remote dispatch in the first policy slice".to_string(),
                }],
                Some("critical sensitivity blocks remote dispatch".to_string()),
            ));
        }

        let snapshot = load_emily_policy_snapshot(self, &request).await?;
        if let Some(gated_result) = apply_emily_policy_gates(&request, snapshot.as_ref()) {
            return Ok(gated_result);
        }

        let Some(provider_registry) = self.provider_registry.as_ref() else {
            return Err(MembraneRuntimeError::InvalidState(
                "routing policy evaluation requires an injected provider registry".to_string(),
            ));
        };

        let mut ranked_targets = provider_registry
            .targets()
            .into_iter()
            .filter_map(|candidate| {
                score_registered_target(&candidate, &request).map(|score| (score, candidate))
            })
            .collect::<Vec<_>>();

        if ranked_targets.is_empty() {
            return Ok(build_policy_result(
                &request,
                RoutingPolicyOutcome::LocalOnly,
                false,
                None,
                vec![RoutingPolicyFinding {
                    code: "no-matching-provider".to_string(),
                    severity: RoutingPolicyFindingSeverity::Info,
                    detail: "no registered provider matches the requested routing preference"
                        .to_string(),
                }],
                Some("no registered providers satisfied the routing preference".to_string()),
            ));
        }

        ranked_targets.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| left.1.target.provider_id.cmp(&right.1.target.provider_id))
                .then_with(|| left.1.target.profile_id.cmp(&right.1.target.profile_id))
                .then_with(|| left.1.target.model_id.cmp(&right.1.target.model_id))
        });

        let selected_target = ranked_targets
            .first()
            .map(|(_, candidate)| candidate.target.clone())
            .ok_or_else(|| {
                MembraneRuntimeError::InvalidState(
                    "ranked targets unexpectedly empty after evaluation".to_string(),
                )
            })?;

        let mut findings = Vec::new();
        if let Some(snapshot) = snapshot.as_ref() {
            findings.extend(emily_caution_findings(snapshot));
        }
        if request.sensitivity == RoutingSensitivity::High {
            findings.push(RoutingPolicyFinding {
                code: "high-sensitivity-caution".to_string(),
                severity: RoutingPolicyFindingSeverity::Caution,
                detail:
                    "high-sensitivity task is allowed to route remotely with caution in the first policy slice"
                        .to_string(),
            });
        }

        findings.push(RoutingPolicyFinding {
            code: "provider-selected".to_string(),
            severity: RoutingPolicyFindingSeverity::Info,
            detail: format!(
                "selected provider '{}' for deterministic single-remote routing",
                selected_target.provider_id
            ),
        });

        Ok(build_policy_result(
            &request,
            RoutingPolicyOutcome::SingleRemote,
            request.sensitivity == RoutingSensitivity::High
                || snapshot.as_ref().is_some_and(snapshot_requires_caution),
            Some(selected_target.clone()),
            findings,
            Some(format!(
                "selected '{}' through deterministic routing-policy scoring",
                selected_target.provider_id
            )),
        ))
    }
}

async fn load_emily_policy_snapshot<A>(
    runtime: &super::MembraneRuntime<A>,
    request: &RoutingPolicyRequest,
) -> Result<Option<EmilyRoutingPolicySnapshot>, MembraneRuntimeError>
where
    A: emily::EmilyApi + ?Sized,
{
    let Some(episode) = runtime.emily().episode(&request.episode_id).await? else {
        return Ok(None);
    };
    let latest_earl = runtime
        .emily()
        .latest_earl_evaluation_for_episode(&request.episode_id)
        .await?;
    Ok(Some(EmilyRoutingPolicySnapshot {
        episode,
        latest_earl,
    }))
}

fn apply_emily_policy_gates(
    request: &RoutingPolicyRequest,
    snapshot: Option<&EmilyRoutingPolicySnapshot>,
) -> Option<RoutingPolicyResult> {
    let snapshot = match snapshot {
        Some(snapshot) => snapshot,
        None => {
            return Some(build_policy_result(
                request,
                RoutingPolicyOutcome::Rejected,
                false,
                None,
                vec![RoutingPolicyFinding {
                    code: "episode-missing".to_string(),
                    severity: RoutingPolicyFindingSeverity::Block,
                    detail: "routing policy requires an existing Emily episode anchor before remote dispatch".to_string(),
                }],
                Some("missing Emily episode blocks remote dispatch".to_string()),
            ));
        }
    };

    if matches!(
        snapshot.episode.state,
        EpisodeState::Completed | EpisodeState::Cancelled
    ) {
        return Some(build_policy_result(
            request,
            RoutingPolicyOutcome::Rejected,
            false,
            None,
            vec![RoutingPolicyFinding {
                code: "episode-closed".to_string(),
                severity: RoutingPolicyFindingSeverity::Block,
                detail: format!(
                    "episode '{}' is already closed and cannot route remotely",
                    snapshot.episode.id
                ),
            }],
            Some("closed episodes are not eligible for remote dispatch".to_string()),
        ));
    }

    if let Some(latest_earl) = snapshot.latest_earl.as_ref()
        && latest_earl.decision == EarlDecision::Reflex
    {
        return Some(build_policy_result(
            request,
            RoutingPolicyOutcome::Rejected,
            false,
            None,
            vec![RoutingPolicyFinding {
                code: "earl-reflex-gate".to_string(),
                severity: RoutingPolicyFindingSeverity::Block,
                detail: format!(
                    "EARL evaluation '{}' reflex-gated episode '{}'",
                    latest_earl.id, latest_earl.episode_id
                ),
            }],
            Some("EARL reflex state blocks remote dispatch".to_string()),
        ));
    }

    if snapshot.episode.state == EpisodeState::Blocked {
        return Some(build_policy_result(
            request,
            RoutingPolicyOutcome::Rejected,
            false,
            None,
            vec![RoutingPolicyFinding {
                code: "episode-blocked".to_string(),
                severity: RoutingPolicyFindingSeverity::Block,
                detail: format!(
                    "episode '{}' is blocked inside Emily and cannot route remotely",
                    snapshot.episode.id
                ),
            }],
            Some("blocked episodes are not eligible for remote dispatch".to_string()),
        ));
    }

    None
}

fn snapshot_requires_caution(snapshot: &EmilyRoutingPolicySnapshot) -> bool {
    snapshot.episode.state == EpisodeState::Cautioned
        || snapshot
            .latest_earl
            .as_ref()
            .is_some_and(|evaluation| evaluation.decision == EarlDecision::Caution)
}

fn emily_caution_findings(snapshot: &EmilyRoutingPolicySnapshot) -> Vec<RoutingPolicyFinding> {
    let mut findings = Vec::new();

    if let Some(latest_earl) = snapshot.latest_earl.as_ref()
        && latest_earl.decision == EarlDecision::Caution
    {
        findings.push(RoutingPolicyFinding {
            code: "earl-caution-gate".to_string(),
            severity: RoutingPolicyFindingSeverity::Caution,
            detail: format!(
                "EARL evaluation '{}' requires caution before remote dispatch",
                latest_earl.id
            ),
        });
    }

    if snapshot.episode.state == EpisodeState::Cautioned {
        findings.push(RoutingPolicyFinding {
            code: "episode-cautioned".to_string(),
            severity: RoutingPolicyFindingSeverity::Caution,
            detail: format!(
                "episode '{}' is already cautioned inside Emily",
                snapshot.episode.id
            ),
        });
    }

    findings
}

fn validate_routing_policy_request(
    request: &RoutingPolicyRequest,
) -> Result<(), MembraneRuntimeError> {
    if request.task_id.trim().is_empty() {
        return Err(MembraneRuntimeError::InvalidRequest(
            "routing policy task_id must not be empty".to_string(),
        ));
    }
    if request.episode_id.trim().is_empty() {
        return Err(MembraneRuntimeError::InvalidRequest(
            "routing policy episode_id must not be empty".to_string(),
        ));
    }
    if matches!(request.preference.provider_id.as_deref(), Some(value) if value.trim().is_empty()) {
        return Err(MembraneRuntimeError::InvalidRequest(
            "routing policy provider_id preference must not be empty when provided".to_string(),
        ));
    }
    if matches!(request.preference.profile_id.as_deref(), Some(value) if value.trim().is_empty()) {
        return Err(MembraneRuntimeError::InvalidRequest(
            "routing policy profile_id preference must not be empty when provided".to_string(),
        ));
    }
    for tag in &request.preference.required_capability_tags {
        if tag.trim().is_empty() {
            return Err(MembraneRuntimeError::InvalidRequest(
                "routing policy required_capability_tags must not contain empty values".to_string(),
            ));
        }
    }
    Ok(())
}

fn score_registered_target(
    candidate: &RegisteredProviderTarget,
    request: &RoutingPolicyRequest,
) -> Option<i32> {
    if let Some(provider_id) = request.preference.provider_id.as_deref()
        && candidate.target.provider_id != provider_id
    {
        return None;
    }

    if let Some(profile_id) = request.preference.profile_id.as_deref()
        && candidate.target.profile_id.as_deref() != Some(profile_id)
    {
        return None;
    }

    if request
        .preference
        .required_capability_tags
        .iter()
        .any(|required| {
            !candidate
                .target
                .capability_tags
                .iter()
                .any(|tag| tag == required)
        })
    {
        return None;
    }

    let mut score = 0;

    if request.preference.provider_id.is_some() {
        score += SCORE_PROVIDER_HINT_MATCH;
    }
    if request.preference.profile_id.is_some() {
        score += SCORE_PROFILE_HINT_MATCH;
    }

    score +=
        request.preference.required_capability_tags.len() as i32 * SCORE_REQUIRED_CAPABILITY_TAG;

    let additional_capability_tags = candidate
        .target
        .capability_tags
        .iter()
        .filter(|tag| {
            !request
                .preference
                .required_capability_tags
                .iter()
                .any(|required| required == *tag)
        })
        .count() as i32;
    score += additional_capability_tags * SCORE_ADDITIONAL_CAPABILITY_TAG;

    if candidate.target.model_id.is_some() {
        score += SCORE_MODEL_PRESENT;
    }

    Some(score)
}

fn build_policy_result(
    request: &RoutingPolicyRequest,
    outcome: RoutingPolicyOutcome,
    caution: bool,
    selected_target: Option<ProviderTarget>,
    findings: Vec<RoutingPolicyFinding>,
    rationale: Option<String>,
) -> RoutingPolicyResult {
    RoutingPolicyResult {
        task_id: request.task_id.clone(),
        outcome,
        caution,
        selected_target,
        findings,
        rationale,
    }
}
