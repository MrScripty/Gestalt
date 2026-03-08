use super::{MembraneRuntime, MembraneRuntimeError};
use crate::contracts::{
    MembraneTaskRequest, MembraneValidationDisposition, RemoteExecutionPersistence,
    RemoteRetryAttemptPersistence, RemoteRetryAttemptRecord, RemoteRetryExecutionPersistence,
    RemoteRetryExecutionRecord, RemoteRetryPolicy, RemoteRoutingPreference, RetryMutationStrategy,
    RetryReason,
};
use crate::providers::ProviderTarget;
use emily::{AppendSovereignAuditRecordRequest, AuditRecordKind, SovereignAuditMetadata};
use serde_json::json;

#[derive(Clone, Copy)]
struct PendingRetryContext<'a> {
    reason: RetryReason,
    summary: &'a str,
    previous_remote_episode_id: Option<&'a str>,
    previous_validation_id: Option<&'a str>,
    attempt_number: u8,
}

impl<A> MembraneRuntime<A>
where
    A: emily::EmilyApi + ?Sized,
{
    /// Resolve a target from the registry, then execute a bounded retrying
    /// remote membrane flow.
    pub async fn execute_remote_with_registry_retry_and_record(
        &self,
        request: MembraneTaskRequest,
        preference: RemoteRoutingPreference,
        policy: RemoteRetryPolicy,
        persistence: RemoteRetryExecutionPersistence,
    ) -> Result<RemoteRetryExecutionRecord, MembraneRuntimeError> {
        let target = self.select_remote_target(&preference).await?;
        self.execute_remote_with_retry_and_record(request, target, policy, persistence)
            .await
    }

    /// Execute one bounded request-scoped retrying remote membrane flow.
    pub async fn execute_remote_with_retry_and_record(
        &self,
        request: MembraneTaskRequest,
        target: ProviderTarget,
        policy: RemoteRetryPolicy,
        persistence: RemoteRetryExecutionPersistence,
    ) -> Result<RemoteRetryExecutionRecord, MembraneRuntimeError> {
        validate_retry_policy(&policy)?;
        validate_retry_execution_persistence(&persistence, &policy)?;

        let mut attempts = Vec::new();
        let original_request = request.clone();
        let mut next_request = request;
        let mut pending_reason: Option<RetryReason> = None;
        let mut pending_summary: Option<String> = None;
        let mut previous_validation_id: Option<String> = None;
        let mut previous_remote_episode_id: Option<String> = None;

        for attempt_index in 0..usize::from(policy.max_attempts) {
            let attempt_number = u8::try_from(attempt_index + 1).map_err(|_| {
                MembraneRuntimeError::InvalidRequest(
                    "retry attempt index overflowed u8".to_string(),
                )
            })?;
            let attempt_persistence = persistence.attempts.get(attempt_index).ok_or_else(|| {
                MembraneRuntimeError::InvalidRequest(format!(
                    "missing retry attempt persistence for attempt {attempt_number}"
                ))
            })?;

            if let Some(reason) = pending_reason {
                let retry_context = PendingRetryContext {
                    reason,
                    summary: pending_summary.as_deref().unwrap_or("retry requested"),
                    previous_remote_episode_id: previous_remote_episode_id.as_deref(),
                    previous_validation_id: previous_validation_id.as_deref(),
                    attempt_number,
                };
                self.append_retry_audit(
                    &original_request.episode_id,
                    attempt_persistence,
                    &persistence,
                    retry_context,
                )
                .await?;

                if policy.mutation != RetryMutationStrategy::None {
                    let (mutated_request, mutation_summary) = mutate_retry_request(
                        &original_request,
                        &policy,
                        &retry_context.reason,
                        retry_context.summary,
                        attempt_number,
                    );
                    self.append_mutation_audit(
                        &original_request.episode_id,
                        attempt_persistence,
                        &persistence,
                        mutation_summary,
                        retry_context,
                    )
                    .await?;
                    next_request = mutated_request;
                } else {
                    next_request = original_request.clone();
                }
            }

            let single_persistence = attempt_remote_persistence(&persistence, attempt_persistence);

            match self
                .execute_remote_and_record(next_request.clone(), target.clone(), single_persistence)
                .await
            {
                Ok(execution) => {
                    previous_remote_episode_id = Some(execution.remote_episode_id.clone());
                    previous_validation_id = Some(execution.validation_id.clone());

                    let retry_reason = match execution.validation.disposition {
                        MembraneValidationDisposition::NeedsReview
                            if policy.retry_on_validation_review
                                && attempt_index + 1 < usize::from(policy.max_attempts) =>
                        {
                            Some(RetryReason::ValidationReview)
                        }
                        _ => None,
                    };

                    let should_retry = retry_reason.is_some();
                    let exhausted = execution.validation.disposition
                        == MembraneValidationDisposition::NeedsReview
                        && policy.retry_on_validation_review
                        && attempt_index + 1 == usize::from(policy.max_attempts);

                    attempts.push(RemoteRetryAttemptRecord {
                        attempt_index: attempt_number,
                        provider_request_id: execution.provider_request_id.clone(),
                        remote_episode_id: execution.remote_episode_id.clone(),
                        retry_reason,
                        execution: Some(execution.clone()),
                        provider_error: None,
                    });

                    if should_retry {
                        pending_reason = Some(RetryReason::ValidationReview);
                        pending_summary = Some(review_summary(&execution));
                        continue;
                    }

                    return Ok(RemoteRetryExecutionRecord {
                        policy,
                        attempts,
                        final_execution: Some(execution),
                        exhausted,
                    });
                }
                Err(MembraneRuntimeError::Provider(error)) => {
                    let retry_reason = if policy.retry_on_provider_error
                        && attempt_index + 1 < usize::from(policy.max_attempts)
                    {
                        Some(RetryReason::ProviderError)
                    } else {
                        None
                    };
                    let exhausted = policy.retry_on_provider_error
                        && attempt_index + 1 == usize::from(policy.max_attempts);
                    let provider_error = error.to_string();

                    attempts.push(RemoteRetryAttemptRecord {
                        attempt_index: attempt_number,
                        provider_request_id: attempt_persistence.provider_request_id.clone(),
                        remote_episode_id: attempt_persistence.remote_episode_id.clone(),
                        retry_reason,
                        execution: None,
                        provider_error: Some(provider_error.clone()),
                    });

                    if retry_reason.is_some() {
                        pending_reason = Some(RetryReason::ProviderError);
                        pending_summary = Some(provider_error);
                        previous_remote_episode_id =
                            Some(attempt_persistence.remote_episode_id.clone());
                        previous_validation_id = None;
                        continue;
                    }

                    return Ok(RemoteRetryExecutionRecord {
                        policy,
                        attempts,
                        final_execution: None,
                        exhausted,
                    });
                }
                Err(error) => return Err(error),
            }
        }

        Ok(RemoteRetryExecutionRecord {
            policy,
            attempts,
            final_execution: None,
            exhausted: true,
        })
    }

    async fn append_retry_audit(
        &self,
        episode_id: &str,
        attempt: &RemoteRetryAttemptPersistence,
        persistence: &RemoteRetryExecutionPersistence,
        retry_context: PendingRetryContext<'_>,
    ) -> Result<(), MembraneRuntimeError> {
        let audit_id = attempt.retry_audit_id.as_deref().ok_or_else(|| {
            MembraneRuntimeError::InvalidRequest(format!(
                "retry_audit_id is required for retry attempt {}",
                retry_context.attempt_number
            ))
        })?;
        let ts_unix_ms = attempt.retry_audit_at_unix_ms.ok_or_else(|| {
            MembraneRuntimeError::InvalidRequest(format!(
                "retry_audit_at_unix_ms is required when retry_audit_id is set for attempt {}",
                retry_context.attempt_number
            ))
        })?;
        self.emily
            .append_sovereign_audit_record(AppendSovereignAuditRecordRequest {
                audit_id: audit_id.to_string(),
                episode_id: episode_id.to_string(),
                kind: AuditRecordKind::BoundaryEvent,
                ts_unix_ms,
                summary: format!(
                    "retry attempt {} after {}: {}",
                    retry_context.attempt_number,
                    retry_reason_label(retry_context.reason),
                    retry_context.summary
                ),
                metadata: SovereignAuditMetadata {
                    remote_episode_id: retry_context
                        .previous_remote_episode_id
                        .map(ToString::to_string),
                    route_decision_id: Some(persistence.route_decision_id.clone()),
                    validation_id: retry_context
                        .previous_validation_id
                        .map(ToString::to_string),
                    boundary_profile: Some("retry".to_string()),
                    metadata: json!({
                        "attempt_index": retry_context.attempt_number,
                        "reason": retry_reason_label(retry_context.reason),
                    }),
                },
            })
            .await?;
        Ok(())
    }

    async fn append_mutation_audit(
        &self,
        episode_id: &str,
        attempt: &RemoteRetryAttemptPersistence,
        persistence: &RemoteRetryExecutionPersistence,
        summary: String,
        retry_context: PendingRetryContext<'_>,
    ) -> Result<(), MembraneRuntimeError> {
        let audit_id = attempt.mutation_audit_id.as_deref().ok_or_else(|| {
            MembraneRuntimeError::InvalidRequest(format!(
                "mutation_audit_id is required for retry mutation on attempt {}",
                retry_context.attempt_number
            ))
        })?;
        let ts_unix_ms = attempt.mutation_audit_at_unix_ms.ok_or_else(|| {
            MembraneRuntimeError::InvalidRequest(format!(
                "mutation_audit_at_unix_ms is required when mutation_audit_id is set for attempt {}",
                retry_context.attempt_number
            ))
        })?;
        self.emily
            .append_sovereign_audit_record(AppendSovereignAuditRecordRequest {
                audit_id: audit_id.to_string(),
                episode_id: episode_id.to_string(),
                kind: AuditRecordKind::BoundaryEvent,
                ts_unix_ms,
                summary,
                metadata: SovereignAuditMetadata {
                    remote_episode_id: retry_context
                        .previous_remote_episode_id
                        .map(ToString::to_string),
                    route_decision_id: Some(persistence.route_decision_id.clone()),
                    validation_id: retry_context
                        .previous_validation_id
                        .map(ToString::to_string),
                    boundary_profile: Some("mutation".to_string()),
                    metadata: json!({
                        "attempt_index": retry_context.attempt_number,
                        "strategy": "append-retry-hint-v1",
                    }),
                },
            })
            .await?;
        Ok(())
    }
}

fn validate_retry_policy(policy: &RemoteRetryPolicy) -> Result<(), MembraneRuntimeError> {
    if policy.max_attempts == 0 {
        return Err(MembraneRuntimeError::InvalidRequest(
            "retry policy max_attempts must be greater than zero".to_string(),
        ));
    }
    Ok(())
}

fn validate_retry_execution_persistence(
    persistence: &RemoteRetryExecutionPersistence,
    policy: &RemoteRetryPolicy,
) -> Result<(), MembraneRuntimeError> {
    if persistence.route_decision_id.trim().is_empty() {
        return Err(MembraneRuntimeError::InvalidRequest(
            "retry persistence route_decision_id must not be empty".to_string(),
        ));
    }
    if persistence.attempts.len() < usize::from(policy.max_attempts) {
        return Err(MembraneRuntimeError::InvalidRequest(format!(
            "retry persistence must include at least {} attempt entries",
            policy.max_attempts
        )));
    }
    Ok(())
}

fn attempt_remote_persistence(
    persistence: &RemoteRetryExecutionPersistence,
    attempt: &RemoteRetryAttemptPersistence,
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

fn mutate_retry_request(
    original_request: &MembraneTaskRequest,
    policy: &RemoteRetryPolicy,
    reason: &RetryReason,
    summary: &str,
    attempt_number: u8,
) -> (MembraneTaskRequest, String) {
    match policy.mutation {
        RetryMutationStrategy::None => (
            original_request.clone(),
            "no retry mutation applied".to_string(),
        ),
        RetryMutationStrategy::AppendRetryHintV1 => {
            let retry_note = format!(
                "{}\n\nRetry note:\nAttempt {attempt_number} follows {}. {}\nProvide a clearer direct answer with enough detail to improve confidence.",
                original_request.task_text,
                retry_reason_label(*reason),
                summary
            );
            let mut request = original_request.clone();
            request.task_text = retry_note;
            (
                request,
                format!("applied append-retry-hint-v1 before retry attempt {attempt_number}"),
            )
        }
    }
}

fn review_summary(record: &crate::contracts::RemoteExecutionRecord) -> String {
    if let Some(first) = record.validation.findings.first() {
        first.detail.clone()
    } else {
        "previous attempt required review".to_string()
    }
}

fn retry_reason_label(reason: RetryReason) -> &'static str {
    match reason {
        RetryReason::ProviderError => "provider error",
        RetryReason::ValidationReview => "validation review",
    }
}
