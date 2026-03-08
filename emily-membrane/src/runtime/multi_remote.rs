use super::{MembraneRuntime, MembraneRuntimeError};
use crate::contracts::{
    CompileResult, MembraneRouteKind, MembraneTaskRequest, MultiRemoteAttemptPersistence,
    MultiRemoteAttemptRecord, MultiRemoteAttemptStatus, MultiRemoteExecutionPersistence,
    MultiRemoteExecutionPolicy, MultiRemoteExecutionRecord, MultiRemoteReconciliationDecision,
    MultiRemoteReconciliationMode, MultiRemoteReconciliationRecord, MultiRemoteSkipReason,
    MultiRemoteStopCondition, RemoteExecutionPersistence, RemoteExecutionRecord, RoutingPlan,
    RoutingTarget,
};
use crate::providers::ProviderTarget;
use crate::runtime::remote::{
    build_membrane_target_id, build_provider_dispatch_request, build_remote_dispatch_result,
    build_remote_episode_request, build_remote_routing_decision, build_remote_validation_envelope,
    build_remote_validation_outcome, remote_episode_matches_request, validate_provider_target,
    validate_remote_persistence,
};
use emily::{
    AppendSovereignAuditRecordRequest, AuditRecordKind, RoutingDecision, RoutingDecisionKind,
    SovereignAuditMetadata,
};
use serde_json::json;

impl<A> MembraneRuntime<A>
where
    A: emily::EmilyApi + ?Sized,
{
    /// Produce a deterministic multi-remote route for the compiled task.
    pub async fn route_multi_remote(
        &self,
        compiled: &CompileResult,
        targets: &[ProviderTarget],
    ) -> Result<RoutingPlan, MembraneRuntimeError> {
        validate_multi_remote_targets(targets)?;

        Ok(RoutingPlan {
            task_id: compiled.compiled_task.task_id.clone(),
            decision: MembraneRouteKind::MultiRemote,
            targets: build_membrane_routing_targets(targets),
            rationale: Some(format!(
                "fan out to {} remote providers for bounded reconciliation",
                targets.len()
            )),
        })
    }

    /// Execute one bounded sequential multi-target membrane flow and persist
    /// the resulting sovereign artifacts through Emily's public APIs.
    pub async fn execute_multi_remote_and_record(
        &self,
        request: MembraneTaskRequest,
        targets: Vec<ProviderTarget>,
        policy: MultiRemoteExecutionPolicy,
        persistence: MultiRemoteExecutionPersistence,
    ) -> Result<MultiRemoteExecutionRecord, MembraneRuntimeError> {
        validate_multi_remote_policy(&policy)?;
        validate_multi_remote_persistence(&persistence, &policy, targets.len())?;
        validate_multi_remote_targets(&targets)?;

        let Some(provider_registry) = self.provider_registry.as_ref() else {
            return Err(MembraneRuntimeError::InvalidState(
                "multi-remote execution requires an injected provider registry".to_string(),
            ));
        };

        let compile = self.compile(request).await?;
        let route = self.route_multi_remote(&compile, &targets).await?;
        let expected_routing_decision =
            build_multi_remote_routing_decision(&compile, &route, &targets, &persistence);
        let routing_decision = match self
            .emily
            .routing_decision(&expected_routing_decision.decision_id)
            .await?
        {
            Some(existing) if existing == expected_routing_decision => existing,
            Some(_) => {
                return Err(MembraneRuntimeError::InvalidState(
                    "existing multi-remote routing decision does not match expected shape"
                        .to_string(),
                ));
            }
            None => {
                self.emily
                    .record_routing_decision(expected_routing_decision)
                    .await?
            }
        };

        let mut attempts = Vec::new();
        let mut accepted_selected = false;

        for (target, attempt_persistence) in targets.iter().cloned().zip(&persistence.attempts) {
            if accepted_selected
                && policy.stop_condition == MultiRemoteStopCondition::StopOnAccepted
            {
                attempts.push(MultiRemoteAttemptRecord {
                    target,
                    provider_request_id: attempt_persistence.provider_request_id.clone(),
                    remote_episode_id: attempt_persistence.remote_episode_id.clone(),
                    validation_id: None,
                    validation_disposition: None,
                    status: MultiRemoteAttemptStatus::Skipped,
                    skip_reason: Some(MultiRemoteSkipReason::StopConditionSatisfied),
                    execution: None,
                    provider_error: None,
                });
                continue;
            }

            let Some(provider) = provider_registry.provider(&target.provider_id) else {
                return Err(MembraneRuntimeError::InvalidRequest(format!(
                    "no provider registered for target '{}'",
                    target.provider_id
                )));
            };

            let single_persistence =
                build_remote_execution_persistence(&persistence, attempt_persistence);
            let expected_remote_request = build_remote_episode_request(
                &compile,
                &routing_decision,
                &target,
                &single_persistence,
                "multi-remote",
            );
            let remote_episode = match self
                .emily
                .remote_episode(&expected_remote_request.remote_episode_id)
                .await?
            {
                Some(existing)
                    if remote_episode_matches_request(&existing, &expected_remote_request) =>
                {
                    existing
                }
                Some(_) => {
                    return Err(MembraneRuntimeError::InvalidState(
                        "existing multi-remote episode does not match expected dispatch shape"
                            .to_string(),
                    ));
                }
                None => {
                    self.emily
                        .create_remote_episode(expected_remote_request)
                        .await?
                }
            };

            let provider_request = build_provider_dispatch_request(
                &compile,
                &target,
                &single_persistence,
                "multi-remote",
            );
            let provider_result = match provider.dispatch(provider_request).await {
                Ok(result) => result,
                Err(error) => {
                    self.fail_remote_episode(
                        &remote_episode.id,
                        single_persistence.validated_at_unix_ms,
                        error.to_string(),
                        "multi-remote",
                    )
                    .await?;
                    attempts.push(MultiRemoteAttemptRecord {
                        target,
                        provider_request_id: attempt_persistence.provider_request_id.clone(),
                        remote_episode_id: attempt_persistence.remote_episode_id.clone(),
                        validation_id: None,
                        validation_disposition: None,
                        status: MultiRemoteAttemptStatus::Executed,
                        skip_reason: None,
                        execution: None,
                        provider_error: Some(error.to_string()),
                    });
                    continue;
                }
            };

            let dispatch = build_remote_dispatch_result(
                &compile,
                &remote_episode,
                &provider_result,
                MembraneRouteKind::MultiRemote,
            );
            let validation = build_remote_validation_envelope(&dispatch, &provider_result);
            let expected_validation = build_remote_validation_outcome(
                &compile,
                &validation,
                &single_persistence,
                &remote_episode,
                "multi-remote",
            );
            let validation_outcome = match self
                .emily
                .validation_outcome(&expected_validation.validation_id)
                .await?
            {
                Some(existing) if existing == expected_validation => existing,
                Some(_) => {
                    return Err(MembraneRuntimeError::InvalidState(
                        "existing multi-remote validation outcome does not match expected shape"
                            .to_string(),
                    ));
                }
                None => {
                    self.emily
                        .record_validation_outcome(expected_validation)
                        .await?
                }
            };

            let attempt_execution = match validation.disposition {
                crate::contracts::MembraneValidationDisposition::Rejected => None,
                _ => Some(RemoteExecutionRecord {
                    compile: compile.clone(),
                    route: route.clone(),
                    dispatch,
                    validation: validation.clone(),
                    reconstruction: self.reconstruct(&validation).await?,
                    provider_request_id: attempt_persistence.provider_request_id.clone(),
                    route_decision_id: routing_decision.decision_id.clone(),
                    remote_episode_id: remote_episode.id.clone(),
                    validation_id: validation_outcome.validation_id.clone(),
                }),
            };

            if validation.disposition == crate::contracts::MembraneValidationDisposition::Accepted {
                accepted_selected = true;
            }

            attempts.push(MultiRemoteAttemptRecord {
                target,
                provider_request_id: attempt_persistence.provider_request_id.clone(),
                remote_episode_id: remote_episode.id.clone(),
                validation_id: Some(validation_outcome.validation_id.clone()),
                validation_disposition: Some(validation.disposition),
                status: MultiRemoteAttemptStatus::Executed,
                skip_reason: None,
                execution: attempt_execution,
                provider_error: None,
            });
        }

        let reconciliation = reconcile_multi_remote_attempts(&attempts, policy.reconciliation);
        append_multi_remote_reconciliation_audit(
            self,
            &compile,
            &routing_decision,
            &reconciliation,
            &persistence,
        )
        .await?;

        Ok(MultiRemoteExecutionRecord {
            compile,
            route,
            policy,
            attempts,
            reconciliation,
            route_decision_id: routing_decision.decision_id,
        })
    }
}

fn validate_multi_remote_policy(
    policy: &MultiRemoteExecutionPolicy,
) -> Result<(), MembraneRuntimeError> {
    if policy.max_targets < 2 {
        return Err(MembraneRuntimeError::InvalidRequest(
            "multi-remote policy max_targets must be at least two".to_string(),
        ));
    }
    Ok(())
}

fn validate_multi_remote_targets(targets: &[ProviderTarget]) -> Result<(), MembraneRuntimeError> {
    if targets.len() < 2 {
        return Err(MembraneRuntimeError::InvalidRequest(
            "multi-remote execution requires at least two targets".to_string(),
        ));
    }
    for target in targets {
        validate_provider_target(target)?;
    }
    Ok(())
}

fn validate_multi_remote_persistence(
    persistence: &MultiRemoteExecutionPersistence,
    policy: &MultiRemoteExecutionPolicy,
    target_count: usize,
) -> Result<(), MembraneRuntimeError> {
    if persistence.route_decision_id.trim().is_empty() {
        return Err(MembraneRuntimeError::InvalidRequest(
            "multi-remote route_decision_id must not be empty".to_string(),
        ));
    }
    if persistence.reconciliation_audit_id.trim().is_empty() {
        return Err(MembraneRuntimeError::InvalidRequest(
            "multi-remote reconciliation_audit_id must not be empty".to_string(),
        ));
    }
    if target_count > usize::from(policy.max_targets) {
        return Err(MembraneRuntimeError::InvalidRequest(format!(
            "multi-remote target count {} exceeds max_targets {}",
            target_count, policy.max_targets
        )));
    }
    if persistence.attempts.len() < target_count {
        return Err(MembraneRuntimeError::InvalidRequest(format!(
            "multi-remote persistence must include at least {target_count} attempt entries"
        )));
    }
    for attempt in persistence.attempts.iter().take(target_count) {
        let single = build_remote_execution_persistence(persistence, attempt);
        validate_remote_persistence(&single)?;
    }
    if persistence.reconciled_at_unix_ms < persistence.route_decided_at_unix_ms {
        return Err(MembraneRuntimeError::InvalidRequest(
            "reconciled_at_unix_ms must be greater than or equal to route_decided_at_unix_ms"
                .to_string(),
        ));
    }
    Ok(())
}

fn build_membrane_routing_targets(targets: &[ProviderTarget]) -> Vec<RoutingTarget> {
    targets
        .iter()
        .map(|target| RoutingTarget {
            target_id: build_membrane_target_id(target),
            capability_tag: target
                .capability_tags
                .first()
                .cloned()
                .unwrap_or_else(|| "remote".to_string()),
        })
        .collect()
}

fn build_multi_remote_routing_decision(
    compile: &CompileResult,
    route: &RoutingPlan,
    targets: &[ProviderTarget],
    persistence: &MultiRemoteExecutionPersistence,
) -> RoutingDecision {
    build_remote_routing_decision(
        compile,
        route,
        targets,
        persistence.route_decision_id.clone(),
        persistence.route_decided_at_unix_ms,
        RoutingDecisionKind::MultiRemote,
        "multi-remote",
    )
}

fn build_remote_execution_persistence(
    persistence: &MultiRemoteExecutionPersistence,
    attempt: &MultiRemoteAttemptPersistence,
) -> RemoteExecutionPersistence {
    RemoteExecutionPersistence {
        route_decision_id: persistence.route_decision_id.clone(),
        route_decided_at_unix_ms: persistence.route_decided_at_unix_ms,
        provider_request_id: attempt.provider_request_id.clone(),
        remote_episode_id: attempt.remote_episode_id.clone(),
        remote_dispatched_at_unix_ms: attempt.remote_dispatched_at_unix_ms,
        validation_id: attempt.validation_id.clone(),
        validated_at_unix_ms: attempt.validated_at_unix_ms,
    }
}

fn reconcile_multi_remote_attempts(
    attempts: &[MultiRemoteAttemptRecord],
    mode: MultiRemoteReconciliationMode,
) -> MultiRemoteReconciliationRecord {
    match mode {
        MultiRemoteReconciliationMode::FirstAcceptedElseNeedsReview => {
            if let Some(attempt) = attempts.iter().find(|attempt| {
                attempt.validation_disposition
                    == Some(crate::contracts::MembraneValidationDisposition::Accepted)
            }) {
                return MultiRemoteReconciliationRecord {
                    decision: MultiRemoteReconciliationDecision::Accepted,
                    selected_target_id: Some(build_membrane_target_id(&attempt.target)),
                    selected_remote_episode_id: Some(attempt.remote_episode_id.clone()),
                    selected_validation_id: attempt.validation_id.clone(),
                    reconstruction: attempt
                        .execution
                        .as_ref()
                        .map(|execution| execution.reconstruction.clone()),
                    summary: "selected first accepted remote result".to_string(),
                };
            }

            if let Some(attempt) = attempts.iter().find(|attempt| {
                attempt.validation_disposition
                    == Some(crate::contracts::MembraneValidationDisposition::NeedsReview)
            }) {
                return MultiRemoteReconciliationRecord {
                    decision: MultiRemoteReconciliationDecision::NeedsReview,
                    selected_target_id: Some(build_membrane_target_id(&attempt.target)),
                    selected_remote_episode_id: Some(attempt.remote_episode_id.clone()),
                    selected_validation_id: attempt.validation_id.clone(),
                    reconstruction: attempt
                        .execution
                        .as_ref()
                        .map(|execution| execution.reconstruction.clone()),
                    summary: "selected first reviewable remote result after no accepted result"
                        .to_string(),
                };
            }

            MultiRemoteReconciliationRecord {
                decision: MultiRemoteReconciliationDecision::NoResult,
                selected_target_id: None,
                selected_remote_episode_id: None,
                selected_validation_id: None,
                reconstruction: None,
                summary: "no multi-remote attempt produced a usable reconciled result".to_string(),
            }
        }
    }
}

async fn append_multi_remote_reconciliation_audit<A>(
    runtime: &MembraneRuntime<A>,
    compile: &CompileResult,
    routing_decision: &RoutingDecision,
    reconciliation: &MultiRemoteReconciliationRecord,
    persistence: &MultiRemoteExecutionPersistence,
) -> Result<(), MembraneRuntimeError>
where
    A: emily::EmilyApi + ?Sized,
{
    runtime
        .emily()
        .append_sovereign_audit_record(AppendSovereignAuditRecordRequest {
            audit_id: persistence.reconciliation_audit_id.clone(),
            episode_id: compile.compiled_task.episode_id.clone(),
            kind: AuditRecordKind::BoundaryEvent,
            ts_unix_ms: persistence.reconciled_at_unix_ms,
            summary: reconciliation.summary.clone(),
            metadata: SovereignAuditMetadata {
                remote_episode_id: reconciliation.selected_remote_episode_id.clone(),
                route_decision_id: Some(routing_decision.decision_id.clone()),
                validation_id: reconciliation.selected_validation_id.clone(),
                boundary_profile: Some("multi-remote-reconciliation".to_string()),
                metadata: json!({
                    "task_id": compile.compiled_task.task_id,
                    "decision": reconciliation.decision,
                    "selected_target_id": reconciliation.selected_target_id,
                }),
            },
        })
        .await?;
    Ok(())
}
