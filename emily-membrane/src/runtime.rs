//! Membrane runtime facade for local-only Milestone 1 flows.

use crate::contracts::{
    CompileResult, CompiledMembraneTask, DispatchResult, DispatchStatus, MembraneRouteKind,
    MembraneTaskRequest, MembraneValidationDisposition, ReconstructionResult, RoutingPlan,
    ValidationEnvelope, ValidationFinding,
};
use emily::EmilyApi;
use emily::error::EmilyError;
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
