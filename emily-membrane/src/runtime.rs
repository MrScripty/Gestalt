//! Membrane runtime facade for local-only Milestone 1 flows.

use crate::contracts::{
    CompileResult, CompiledMembraneTask, DispatchResult, DispatchStatus, LocalExecutionPersistence,
    LocalExecutionRecord, MembraneRouteKind, MembraneTaskRequest, MembraneValidationDisposition,
    ReconstructionResult, RoutingPlan, ValidationEnvelope, ValidationFinding,
};
use emily::EmilyApi;
use emily::error::EmilyError;
use emily::{
    RoutingDecision, RoutingDecisionKind, ValidationDecision, ValidationFinding as EmilyFinding,
    ValidationFindingSeverity, ValidationOutcome,
};
use serde_json::json;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::Arc;

/// Minimal membrane runtime error surface for Milestone 1.
#[derive(Debug)]
pub enum MembraneRuntimeError {
    Emily(EmilyError),
    InvalidRequest(String),
    InvalidState(String),
    Adapter(String),
}

impl Display for MembraneRuntimeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Emily(error) => write!(f, "emily runtime error: {error}"),
            Self::InvalidRequest(message) => write!(f, "invalid membrane request: {message}"),
            Self::InvalidState(message) => write!(f, "invalid membrane state: {message}"),
            Self::Adapter(message) => write!(f, "local adapter error: {message}"),
        }
    }
}

impl Error for MembraneRuntimeError {}

impl From<EmilyError> for MembraneRuntimeError {
    fn from(value: EmilyError) -> Self {
        Self::Emily(value)
    }
}

/// Runtime facade for the sibling membrane crate.
///
/// Milestone 1 keeps this runtime local-only and deterministic. The Emily API
/// dependency is injected now so later slices can persist sovereign artifacts
/// through Emily without changing the membrane ownership model.
pub struct MembraneRuntime<A>
where
    A: EmilyApi + ?Sized,
{
    emily: Arc<A>,
    adapter: DeterministicLocalAdapter,
}

impl<A> MembraneRuntime<A>
where
    A: EmilyApi + ?Sized,
{
    /// Construct a new membrane runtime with the default local-only adapter.
    pub fn new(emily: Arc<A>) -> Self {
        Self {
            emily,
            adapter: DeterministicLocalAdapter,
        }
    }

    /// Return the injected Emily dependency.
    pub fn emily(&self) -> &Arc<A> {
        &self.emily
    }

    /// Compile a host task into a bounded membrane task.
    pub async fn compile(
        &self,
        request: MembraneTaskRequest,
    ) -> Result<CompileResult, MembraneRuntimeError> {
        if request.task_id.trim().is_empty() {
            return Err(MembraneRuntimeError::InvalidRequest(
                "task_id must not be empty".to_string(),
            ));
        }
        if request.episode_id.trim().is_empty() {
            return Err(MembraneRuntimeError::InvalidRequest(
                "episode_id must not be empty".to_string(),
            ));
        }
        if request.task_text.trim().is_empty() {
            return Err(MembraneRuntimeError::InvalidRequest(
                "task_text must not be empty".to_string(),
            ));
        }

        let bounded_prompt = bounded_prompt(&request);
        let context_fragment_ids = request
            .context_fragments
            .iter()
            .map(|fragment| fragment.fragment_id.clone())
            .collect();

        Ok(CompileResult {
            compiled_task: CompiledMembraneTask {
                task_id: request.task_id,
                episode_id: request.episode_id,
                bounded_prompt,
                context_fragment_ids,
            },
            truncated: false,
        })
    }

    /// Produce a deterministic local-only route for the compiled task.
    pub async fn route(
        &self,
        compiled: &CompileResult,
    ) -> Result<RoutingPlan, MembraneRuntimeError> {
        Ok(RoutingPlan {
            task_id: compiled.compiled_task.task_id.clone(),
            decision: MembraneRouteKind::LocalOnly,
            targets: Vec::new(),
            rationale: Some(
                "Milestone 1 runtime is local-only until provider adapters land".to_string(),
            ),
        })
    }

    /// Execute one compiled task through the internal deterministic local path.
    pub async fn dispatch_local(
        &self,
        compiled: &CompileResult,
        plan: &RoutingPlan,
    ) -> Result<DispatchResult, MembraneRuntimeError> {
        if plan.task_id != compiled.compiled_task.task_id {
            return Err(MembraneRuntimeError::InvalidRequest(
                "routing plan task_id must match compiled task".to_string(),
            ));
        }
        if plan.decision != MembraneRouteKind::LocalOnly {
            return Err(MembraneRuntimeError::InvalidRequest(
                "dispatch_local requires a LocalOnly routing decision".to_string(),
            ));
        }
        if !plan.targets.is_empty() {
            return Err(MembraneRuntimeError::InvalidRequest(
                "local-only routing plans must not include remote targets".to_string(),
            ));
        }

        let response_text = self.adapter.execute(&compiled.compiled_task)?;
        Ok(DispatchResult {
            task_id: compiled.compiled_task.task_id.clone(),
            route: MembraneRouteKind::LocalOnly,
            status: DispatchStatus::LocalCompleted,
            response_text,
            remote_reference: None,
        })
    }

    /// Validate one local dispatch result before reconstruction.
    pub async fn validate(
        &self,
        dispatch: &DispatchResult,
    ) -> Result<ValidationEnvelope, MembraneRuntimeError> {
        if dispatch.status != DispatchStatus::LocalCompleted {
            return Err(MembraneRuntimeError::InvalidState(
                "validate currently requires a completed local dispatch".to_string(),
            ));
        }

        if dispatch.response_text.trim().is_empty() {
            return Ok(ValidationEnvelope {
                task_id: dispatch.task_id.clone(),
                disposition: MembraneValidationDisposition::Rejected,
                findings: vec![ValidationFinding {
                    code: "empty-local-response".to_string(),
                    detail: "local dispatch returned an empty response".to_string(),
                }],
                validated_text: None,
            });
        }

        Ok(ValidationEnvelope {
            task_id: dispatch.task_id.clone(),
            disposition: MembraneValidationDisposition::Accepted,
            findings: Vec::new(),
            validated_text: Some(dispatch.response_text.clone()),
        })
    }

    /// Reconstruct the final host-facing output from a validated result.
    pub async fn reconstruct(
        &self,
        validation: &ValidationEnvelope,
    ) -> Result<ReconstructionResult, MembraneRuntimeError> {
        match validation.disposition {
            MembraneValidationDisposition::Rejected => Err(MembraneRuntimeError::InvalidState(
                "cannot reconstruct from a rejected validation result".to_string(),
            )),
            MembraneValidationDisposition::Accepted
            | MembraneValidationDisposition::NeedsReview => {
                let output_text = validation.validated_text.clone().ok_or_else(|| {
                    MembraneRuntimeError::InvalidState(
                        "validated_text is required for reconstruction".to_string(),
                    )
                })?;

                Ok(ReconstructionResult {
                    task_id: validation.task_id.clone(),
                    output_text,
                    references: Vec::new(),
                    caution: validation.disposition == MembraneValidationDisposition::NeedsReview,
                })
            }
        }
    }

    /// Execute the full local-only membrane path and persist the resulting
    /// sovereign artifacts through Emily's public API.
    pub async fn execute_local_only_and_record(
        &self,
        request: MembraneTaskRequest,
        persistence: LocalExecutionPersistence,
    ) -> Result<LocalExecutionRecord, MembraneRuntimeError> {
        validate_local_persistence(&persistence)?;

        let compile = self.compile(request).await?;
        let route = self.route(&compile).await?;
        let dispatch = self.dispatch_local(&compile, &route).await?;
        let validation = self.validate(&dispatch).await?;
        let reconstruction = self.reconstruct(&validation).await?;

        let expected_routing_decision =
            build_local_routing_decision(&compile, &route, &persistence);
        let routing_decision = match self
            .emily
            .routing_decision(&expected_routing_decision.decision_id)
            .await?
        {
            Some(existing) if existing == expected_routing_decision => existing,
            Some(_) => {
                return Err(MembraneRuntimeError::InvalidState(
                    "existing routing decision does not match expected local-only shape"
                        .to_string(),
                ));
            }
            None => {
                self.emily
                    .record_routing_decision(expected_routing_decision)
                    .await?
            }
        };

        let expected_validation_outcome =
            build_local_validation_outcome(&compile, &validation, &persistence);
        let validation_outcome = match self
            .emily
            .validation_outcome(&expected_validation_outcome.validation_id)
            .await?
        {
            Some(existing) if existing == expected_validation_outcome => existing,
            Some(_) => {
                return Err(MembraneRuntimeError::InvalidState(
                    "existing validation outcome does not match expected local-only shape"
                        .to_string(),
                ));
            }
            None => {
                self.emily
                    .record_validation_outcome(expected_validation_outcome)
                    .await?
            }
        };

        Ok(LocalExecutionRecord {
            compile,
            route,
            dispatch,
            validation,
            reconstruction,
            route_decision_id: routing_decision.decision_id,
            validation_id: validation_outcome.validation_id,
        })
    }
}

fn bounded_prompt(request: &MembraneTaskRequest) -> String {
    if request.context_fragments.is_empty() {
        return request.task_text.clone();
    }

    let context = request
        .context_fragments
        .iter()
        .map(|fragment| format!("[{}] {}", fragment.fragment_id, fragment.text))
        .collect::<Vec<_>>()
        .join("\n");
    format!("{}\n\nContext:\n{}", request.task_text, context)
}

fn validate_local_persistence(
    persistence: &LocalExecutionPersistence,
) -> Result<(), MembraneRuntimeError> {
    if persistence.route_decision_id.trim().is_empty() {
        return Err(MembraneRuntimeError::InvalidRequest(
            "route_decision_id must not be empty".to_string(),
        ));
    }
    if persistence.validation_id.trim().is_empty() {
        return Err(MembraneRuntimeError::InvalidRequest(
            "validation_id must not be empty".to_string(),
        ));
    }
    if persistence.validated_at_unix_ms < persistence.route_decided_at_unix_ms {
        return Err(MembraneRuntimeError::InvalidRequest(
            "validated_at_unix_ms must be greater than or equal to route_decided_at_unix_ms"
                .to_string(),
        ));
    }
    Ok(())
}

fn build_local_routing_decision(
    compile: &CompileResult,
    route: &RoutingPlan,
    persistence: &LocalExecutionPersistence,
) -> RoutingDecision {
    RoutingDecision {
        decision_id: persistence.route_decision_id.clone(),
        episode_id: compile.compiled_task.episode_id.clone(),
        kind: RoutingDecisionKind::LocalOnly,
        decided_at_unix_ms: persistence.route_decided_at_unix_ms,
        rationale: route.rationale.clone(),
        targets: Vec::new(),
        metadata: json!({
            "source": "emily-membrane",
            "mode": "local-only",
            "task_id": compile.compiled_task.task_id.clone(),
        }),
    }
}

fn build_local_validation_outcome(
    compile: &CompileResult,
    validation: &ValidationEnvelope,
    persistence: &LocalExecutionPersistence,
) -> ValidationOutcome {
    ValidationOutcome {
        validation_id: persistence.validation_id.clone(),
        episode_id: compile.compiled_task.episode_id.clone(),
        remote_episode_id: None,
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
            "mode": "local-only",
            "task_id": validation.task_id.clone(),
            "validated_text": validation.validated_text.clone(),
        }),
    }
}

fn to_emily_validation_decision(disposition: MembraneValidationDisposition) -> ValidationDecision {
    match disposition {
        MembraneValidationDisposition::Accepted => ValidationDecision::Accepted,
        MembraneValidationDisposition::NeedsReview => ValidationDecision::NeedsReview,
        MembraneValidationDisposition::Rejected => ValidationDecision::Rejected,
    }
}

fn to_emily_finding_severity(
    disposition: MembraneValidationDisposition,
) -> ValidationFindingSeverity {
    match disposition {
        MembraneValidationDisposition::Accepted => ValidationFindingSeverity::Info,
        MembraneValidationDisposition::NeedsReview => ValidationFindingSeverity::Warning,
        MembraneValidationDisposition::Rejected => ValidationFindingSeverity::Error,
    }
}

/// Internal deterministic local adapter used to prove the runtime shape before
/// provider-backed dispatch exists.
struct DeterministicLocalAdapter;

impl DeterministicLocalAdapter {
    fn execute(&self, task: &CompiledMembraneTask) -> Result<String, MembraneRuntimeError> {
        if task.bounded_prompt.trim().is_empty() {
            return Err(MembraneRuntimeError::Adapter(
                "bounded prompt must not be empty".to_string(),
            ));
        }

        Ok(format!("LOCAL: {}", task.bounded_prompt))
    }
}
