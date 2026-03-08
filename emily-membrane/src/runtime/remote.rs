use super::{
    MembraneRuntime, MembraneRuntimeError, to_emily_finding_severity,
    to_emily_validation_decision,
};
use crate::contracts::{
    CompileResult, DispatchResult, DispatchStatus, MembraneRouteKind, MembraneTaskRequest,
    RemoteExecutionPersistence, RemoteExecutionRecord, RoutingPlan, RoutingTarget,
    ValidationEnvelope, ValidationFinding,
};
use crate::providers::{
    ProviderDispatchKind, ProviderDispatchRequest, ProviderDispatchResult, ProviderDispatchStatus,
    ProviderTarget,
};
use emily::{
    RemoteEpisodeRecord, RemoteEpisodeRequest, RoutingDecision, RoutingDecisionKind,
    RoutingTarget as EmilyRoutingTarget, UpdateRemoteEpisodeStateRequest,
    ValidationFinding as EmilyFinding, ValidationOutcome,
};
use serde_json::json;

impl<A> MembraneRuntime<A>
where
    A: emily::EmilyApi + ?Sized,
{
    /// Produce a deterministic single-remote route for the compiled task.
    pub async fn route_remote(
        &self,
        compiled: &CompileResult,
        target: &ProviderTarget,
    ) -> Result<RoutingPlan, MembraneRuntimeError> {
        validate_provider_target(target)?;

        Ok(RoutingPlan {
            task_id: compiled.compiled_task.task_id.clone(),
            decision: MembraneRouteKind::SingleRemote,
            targets: vec![RoutingTarget {
                target_id: build_membrane_target_id(target),
                capability_tag: target
                    .capability_tags
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "remote".to_string()),
            }],
            rationale: Some(format!(
                "remote provider '{}' selected for single-provider dispatch",
                target.provider_id
            )),
        })
    }

    /// Execute the first provider-backed remote path and persist the resulting
    /// sovereign artifacts through Emily's public APIs.
    pub async fn execute_remote_and_record(
        &self,
        request: MembraneTaskRequest,
        target: ProviderTarget,
        persistence: RemoteExecutionPersistence,
    ) -> Result<RemoteExecutionRecord, MembraneRuntimeError> {
        validate_remote_persistence(&persistence)?;
        validate_provider_target(&target)?;

        let Some(provider) = self.provider.as_ref() else {
            return Err(MembraneRuntimeError::InvalidState(
                "remote execution requires an injected provider".to_string(),
            ));
        };

        let compile = self.compile(request).await?;
        let route = self.route_remote(&compile, &target).await?;

        let expected_routing_decision =
            build_remote_routing_decision(&compile, &route, &target, &persistence);
        let routing_decision = match self
            .emily
            .routing_decision(&expected_routing_decision.decision_id)
            .await?
        {
            Some(existing) if existing == expected_routing_decision => existing,
            Some(_) => {
                return Err(MembraneRuntimeError::InvalidState(
                    "existing remote routing decision does not match expected shape".to_string(),
                ));
            }
            None => {
                self.emily
                    .record_routing_decision(expected_routing_decision)
                    .await?
            }
        };

        let expected_remote_request =
            build_remote_episode_request(&compile, &routing_decision, &target, &persistence);
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
                    "existing remote episode does not match expected dispatch shape".to_string(),
                ));
            }
            None => {
                self.emily
                    .create_remote_episode(expected_remote_request)
                    .await?
            }
        };

        let provider_request = build_provider_dispatch_request(&compile, &target, &persistence);
        let provider_result = match provider.dispatch(provider_request).await {
            Ok(result) => result,
            Err(error) => {
                self.fail_remote_episode(
                    &remote_episode.id,
                    persistence.validated_at_unix_ms,
                    error.to_string(),
                )
                .await?;
                return Err(error.into());
            }
        };

        let dispatch = build_remote_dispatch_result(&compile, &remote_episode, &provider_result);
        let validation = build_remote_validation_envelope(&dispatch, &provider_result);
        let expected_validation =
            build_remote_validation_outcome(&compile, &validation, &persistence, &remote_episode);
        let validation_outcome = match self
            .emily
            .validation_outcome(&expected_validation.validation_id)
            .await?
        {
            Some(existing) if existing == expected_validation => existing,
            Some(_) => {
                return Err(MembraneRuntimeError::InvalidState(
                    "existing remote validation outcome does not match expected shape".to_string(),
                ));
            }
            None => {
                self.emily
                    .record_validation_outcome(expected_validation)
                    .await?
            }
        };

        let reconstruction = match validation.disposition {
            crate::contracts::MembraneValidationDisposition::Rejected => {
                return Err(MembraneRuntimeError::InvalidState(
                    "remote execution was rejected during validation".to_string(),
                ));
            }
            _ => self.reconstruct(&validation).await?,
        };

        Ok(RemoteExecutionRecord {
            compile,
            route,
            dispatch,
            validation,
            reconstruction,
            provider_request_id: persistence.provider_request_id,
            route_decision_id: routing_decision.decision_id,
            remote_episode_id: remote_episode.id,
            validation_id: validation_outcome.validation_id,
        })
    }

    async fn fail_remote_episode(
        &self,
        remote_episode_id: &str,
        transitioned_at_unix_ms: i64,
        summary: String,
    ) -> Result<(), MembraneRuntimeError> {
        self.emily
            .update_remote_episode_state(UpdateRemoteEpisodeStateRequest {
                remote_episode_id: remote_episode_id.to_string(),
                next_state: emily::RemoteEpisodeState::Failed,
                transitioned_at_unix_ms,
                summary: Some(summary),
                metadata: json!({"source": "emily-membrane", "mode": "single-remote"}),
            })
            .await?;
        Ok(())
    }
}

fn validate_remote_persistence(
    persistence: &RemoteExecutionPersistence,
) -> Result<(), MembraneRuntimeError> {
    for (field, value) in [
        ("route_decision_id", persistence.route_decision_id.as_str()),
        (
            "provider_request_id",
            persistence.provider_request_id.as_str(),
        ),
        ("remote_episode_id", persistence.remote_episode_id.as_str()),
        ("validation_id", persistence.validation_id.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(MembraneRuntimeError::InvalidRequest(format!(
                "{field} must not be empty"
            )));
        }
    }
    if persistence.remote_dispatched_at_unix_ms < persistence.route_decided_at_unix_ms {
        return Err(MembraneRuntimeError::InvalidRequest(
            "remote_dispatched_at_unix_ms must be greater than or equal to route_decided_at_unix_ms"
                .to_string(),
        ));
    }
    if persistence.validated_at_unix_ms < persistence.remote_dispatched_at_unix_ms {
        return Err(MembraneRuntimeError::InvalidRequest(
            "validated_at_unix_ms must be greater than or equal to remote_dispatched_at_unix_ms"
                .to_string(),
        ));
    }
    Ok(())
}

fn validate_provider_target(target: &ProviderTarget) -> Result<(), MembraneRuntimeError> {
    if target.provider_id.trim().is_empty() {
        return Err(MembraneRuntimeError::InvalidRequest(
            "provider_id must not be empty".to_string(),
        ));
    }
    for tag in &target.capability_tags {
        if tag.trim().is_empty() {
            return Err(MembraneRuntimeError::InvalidRequest(
                "capability_tags must not contain empty values".to_string(),
            ));
        }
    }
    Ok(())
}

fn build_membrane_target_id(target: &ProviderTarget) -> String {
    match target.model_id.as_deref() {
        Some(model_id) if !model_id.trim().is_empty() => {
            format!("{}:{model_id}", target.provider_id)
        }
        _ => target.provider_id.clone(),
    }
}

fn build_remote_routing_decision(
    compile: &CompileResult,
    route: &RoutingPlan,
    target: &ProviderTarget,
    persistence: &RemoteExecutionPersistence,
) -> RoutingDecision {
    RoutingDecision {
        decision_id: persistence.route_decision_id.clone(),
        episode_id: compile.compiled_task.episode_id.clone(),
        kind: RoutingDecisionKind::SingleRemote,
        decided_at_unix_ms: persistence.route_decided_at_unix_ms,
        rationale: route.rationale.clone(),
        targets: vec![EmilyRoutingTarget {
            provider_id: target.provider_id.clone(),
            model_id: target.model_id.clone(),
            profile_id: target.profile_id.clone(),
            capability_tags: target.capability_tags.clone(),
            metadata: target.metadata.clone(),
        }],
        metadata: json!({
            "source": "emily-membrane",
            "mode": "single-remote",
            "task_id": compile.compiled_task.task_id.clone(),
        }),
    }
}

fn build_remote_episode_request(
    compile: &CompileResult,
    routing_decision: &RoutingDecision,
    target: &ProviderTarget,
    persistence: &RemoteExecutionPersistence,
) -> RemoteEpisodeRequest {
    RemoteEpisodeRequest {
        remote_episode_id: persistence.remote_episode_id.clone(),
        episode_id: compile.compiled_task.episode_id.clone(),
        route_decision_id: Some(routing_decision.decision_id.clone()),
        dispatch_kind: "provider_dispatch".to_string(),
        dispatched_at_unix_ms: persistence.remote_dispatched_at_unix_ms,
        metadata: json!({
            "source": "emily-membrane",
            "provider_request_id": persistence.provider_request_id,
            "provider_id": target.provider_id,
            "model_id": target.model_id,
        }),
    }
}

fn remote_episode_matches_request(
    existing: &RemoteEpisodeRecord,
    request: &RemoteEpisodeRequest,
) -> bool {
    existing.id == request.remote_episode_id
        && existing.episode_id == request.episode_id
        && existing.route_decision_id == request.route_decision_id
        && existing.dispatch_kind == request.dispatch_kind
        && existing.dispatched_at_unix_ms == request.dispatched_at_unix_ms
        && existing.metadata == request.metadata
}

fn build_provider_dispatch_request(
    compile: &CompileResult,
    target: &ProviderTarget,
    persistence: &RemoteExecutionPersistence,
) -> ProviderDispatchRequest {
    ProviderDispatchRequest {
        provider_request_id: persistence.provider_request_id.clone(),
        task_id: compile.compiled_task.task_id.clone(),
        episode_id: compile.compiled_task.episode_id.clone(),
        target: target.clone(),
        dispatch_kind: ProviderDispatchKind::Prompt,
        bounded_payload: compile.compiled_task.bounded_prompt.clone(),
        context_fragment_ids: compile.compiled_task.context_fragment_ids.clone(),
        metadata: json!({
            "source": "emily-membrane",
            "mode": "single-remote",
        }),
    }
}

fn build_remote_dispatch_result(
    compile: &CompileResult,
    remote_episode: &RemoteEpisodeRecord,
    provider_result: &ProviderDispatchResult,
) -> DispatchResult {
    DispatchResult {
        task_id: compile.compiled_task.task_id.clone(),
        route: MembraneRouteKind::SingleRemote,
        status: match provider_result.status {
            ProviderDispatchStatus::Completed => DispatchStatus::RemoteCompleted,
            ProviderDispatchStatus::Failed | ProviderDispatchStatus::Rejected => {
                DispatchStatus::Blocked
            }
        },
        response_text: provider_result.output_text.clone(),
        remote_reference: Some(remote_episode.id.clone()),
    }
}

fn build_remote_validation_envelope(
    dispatch: &DispatchResult,
    provider_result: &ProviderDispatchResult,
) -> ValidationEnvelope {
    match provider_result.status {
        ProviderDispatchStatus::Completed => ValidationEnvelope {
            task_id: dispatch.task_id.clone(),
            disposition: crate::contracts::MembraneValidationDisposition::Accepted,
            findings: Vec::new(),
            validated_text: Some(provider_result.output_text.clone()),
        },
        ProviderDispatchStatus::Failed => ValidationEnvelope {
            task_id: dispatch.task_id.clone(),
            disposition: crate::contracts::MembraneValidationDisposition::NeedsReview,
            findings: vec![ValidationFinding {
                code: "provider-failed".to_string(),
                detail: fallback_provider_message(provider_result),
            }],
            validated_text: Some(fallback_provider_message(provider_result)),
        },
        ProviderDispatchStatus::Rejected => ValidationEnvelope {
            task_id: dispatch.task_id.clone(),
            disposition: crate::contracts::MembraneValidationDisposition::Rejected,
            findings: vec![ValidationFinding {
                code: "provider-rejected".to_string(),
                detail: fallback_provider_message(provider_result),
            }],
            validated_text: None,
        },
    }
}

fn fallback_provider_message(provider_result: &ProviderDispatchResult) -> String {
    if provider_result.output_text.trim().is_empty() {
        format!(
            "provider '{}' returned status '{:?}' without output",
            provider_result.provider_id, provider_result.status
        )
    } else {
        provider_result.output_text.clone()
    }
}

fn build_remote_validation_outcome(
    compile: &CompileResult,
    validation: &ValidationEnvelope,
    persistence: &RemoteExecutionPersistence,
    remote_episode: &RemoteEpisodeRecord,
) -> ValidationOutcome {
    ValidationOutcome {
        validation_id: persistence.validation_id.clone(),
        episode_id: compile.compiled_task.episode_id.clone(),
        remote_episode_id: Some(remote_episode.id.clone()),
        decision: to_emily_validation_decision(validation.disposition),
        validated_at_unix_ms: persistence.validated_at_unix_ms,
        findings: validation
            .findings
            .iter()
            .map(|finding| EmilyFinding {
                code: finding.code.clone(),
                severity: to_emily_finding_severity(validation.disposition),
                message: finding.detail.clone(),
            })
            .collect(),
        metadata: json!({
            "source": "emily-membrane",
            "mode": "single-remote",
            "task_id": validation.task_id.clone(),
            "validated_text": validation.validated_text.clone(),
        }),
    }
}
