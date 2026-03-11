//! Membrane runtime facade for local-only Milestone 1 flows.

use crate::contracts::{
    CompileResult, CompiledMembraneTask, DispatchResult, DispatchStatus, LocalExecutionPersistence,
    LocalExecutionRecord, MembraneBoundaryMetadata, MembraneContextHandle, MembraneIr,
    MembraneIrRenderMode, MembraneProtectionDisposition, MembraneRouteKind, MembraneTaskPayload,
    MembraneTaskRequest, MembraneValidationDisposition, PolicyExecutionPersistence,
    PolicyReflexPersistence, PolicySelectedExecution, RoutingPlan, RoutingPolicyOutcome,
    RoutingPolicyReflexReason, RoutingPolicyRequest, RoutingPolicyResult, ValidationEnvelope,
};
use crate::providers::{
    InMemoryProviderRegistry, MembraneProvider, MembraneProviderError, MembraneProviderRegistry,
};
use emily::EmilyApi;
use emily::error::EmilyError;
use emily::{
    AppendSovereignAuditRecordRequest, AuditRecordKind, RoutingDecision, RoutingDecisionKind,
    SovereignAuditMetadata, ValidationDecision, ValidationFinding as EmilyFinding,
    ValidationFindingSeverity as EmilyValidationFindingSeverity, ValidationOutcome,
};
use serde_json::json;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::Arc;

mod multi_remote;
mod policy;
mod reconstruction;
mod remote;
mod retry;
mod sensitivity;
mod validation;

/// Minimal membrane runtime error surface for Milestone 1.
#[derive(Debug)]
pub enum MembraneRuntimeError {
    Emily(EmilyError),
    Provider(MembraneProviderError),
    InvalidRequest(String),
    InvalidState(String),
    Adapter(String),
}

impl Display for MembraneRuntimeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Emily(error) => write!(f, "emily runtime error: {error}"),
            Self::Provider(error) => write!(f, "membrane provider error: {error}"),
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

impl From<MembraneProviderError> for MembraneRuntimeError {
    fn from(value: MembraneProviderError) -> Self {
        Self::Provider(value)
    }
}

/// Runtime facade for the sibling membrane crate.
///
/// Milestone 1 keeps this runtime local-only and deterministic. The Emily API
/// dependency is injected now so later slices can persist sovereign artifacts
/// through Emily without changing the membrane ownership model. Remote
/// execution can be enabled with either one injected provider or a
/// host-supplied provider registry.
pub struct MembraneRuntime<A>
where
    A: EmilyApi + ?Sized,
{
    emily: Arc<A>,
    provider: Option<Arc<dyn MembraneProvider>>,
    provider_registry: Option<Arc<dyn MembraneProviderRegistry>>,
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
            provider: None,
            provider_registry: None,
            adapter: DeterministicLocalAdapter,
        }
    }

    /// Construct a new membrane runtime with an injected remote provider.
    pub fn with_provider(emily: Arc<A>, provider: Arc<dyn MembraneProvider>) -> Self {
        let registry = Arc::new(InMemoryProviderRegistry::single(provider.clone()))
            as Arc<dyn MembraneProviderRegistry>;
        Self {
            emily,
            provider: Some(provider),
            provider_registry: Some(registry),
            adapter: DeterministicLocalAdapter,
        }
    }

    /// Construct a new membrane runtime with a host-supplied provider registry.
    pub fn with_provider_registry(
        emily: Arc<A>,
        provider_registry: Arc<dyn MembraneProviderRegistry>,
    ) -> Self {
        Self {
            emily,
            provider: None,
            provider_registry: Some(provider_registry),
            adapter: DeterministicLocalAdapter,
        }
    }

    /// Return the injected Emily dependency.
    pub fn emily(&self) -> &Arc<A> {
        &self.emily
    }

    /// Return the injected provider dependency when remote execution is enabled.
    pub fn provider(&self) -> Option<&Arc<dyn MembraneProvider>> {
        self.provider.as_ref()
    }

    /// Return the injected provider registry when remote execution is enabled.
    pub fn provider_registry(&self) -> Option<&Arc<dyn MembraneProviderRegistry>> {
        self.provider_registry.as_ref()
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

        let membrane_ir = build_membrane_ir(&request);
        let bounded_prompt = render_prompt_from_ir(&membrane_ir);
        let context_fragment_ids = membrane_ir
            .context_handles
            .iter()
            .map(|fragment| fragment.fragment_id.clone())
            .collect();

        Ok(CompileResult {
            compiled_task: CompiledMembraneTask {
                task_id: request.task_id,
                episode_id: request.episode_id,
                membrane_ir: Some(membrane_ir),
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
        let reconstruction = self
            .reconstruct_with_context(&compile, &dispatch, &validation)
            .await?;

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

    /// Evaluate routing policy and execute the selected local or remote path
    /// through the existing write flows.
    pub async fn execute_with_policy_and_record(
        &self,
        request: MembraneTaskRequest,
        policy_request: RoutingPolicyRequest,
        persistence: PolicyExecutionPersistence,
    ) -> Result<PolicySelectedExecution, MembraneRuntimeError> {
        validate_policy_task_alignment(&request, &policy_request)?;

        let policy = self.evaluate_routing_policy(policy_request).await?;

        match policy.outcome {
            RoutingPolicyOutcome::LocalOnly => {
                let local_persistence = persistence.local.ok_or_else(|| {
                    MembraneRuntimeError::InvalidRequest(
                        "policy-selected local execution requires local persistence".to_string(),
                    )
                })?;
                let local_execution = self
                    .execute_local_only_and_record(request, local_persistence)
                    .await?;
                Ok(PolicySelectedExecution {
                    policy,
                    reflex_audit_id: None,
                    local_execution: Some(local_execution),
                    remote_execution: None,
                })
            }
            RoutingPolicyOutcome::SingleRemote => {
                let compiled = self.compile(request.clone()).await?;
                if compile_has_blocked_protected_content(&compiled) {
                    let policy = elevate_policy_to_protected_content_reflex(policy, &compiled);
                    let reflex_persistence = persistence.reflex.ok_or_else(|| {
                        MembraneRuntimeError::InvalidRequest(
                            "protected-content reflex fallback requires reflex persistence"
                                .to_string(),
                        )
                    })?;
                    let local_persistence = persistence.local.ok_or_else(|| {
                        MembraneRuntimeError::InvalidRequest(
                            "protected-content reflex fallback requires local persistence"
                                .to_string(),
                        )
                    })?;
                    let reflex_audit_id = self
                        .append_reflex_audit(
                            &request.episode_id,
                            &policy,
                            reflex_persistence,
                            "protected-content-fallback",
                        )
                        .await?;
                    let local_execution = self
                        .execute_local_only_and_record(request, local_persistence)
                        .await?;
                    return Ok(PolicySelectedExecution {
                        policy,
                        reflex_audit_id: Some(reflex_audit_id),
                        local_execution: Some(local_execution),
                        remote_execution: None,
                    });
                }

                let remote_persistence = persistence.remote.ok_or_else(|| {
                    MembraneRuntimeError::InvalidRequest(
                        "policy-selected remote execution requires remote persistence".to_string(),
                    )
                })?;
                let target = policy.selected_target.clone().ok_or_else(|| {
                    MembraneRuntimeError::InvalidState(
                        "policy-selected remote execution requires a selected target".to_string(),
                    )
                })?;
                let remote_execution = self
                    .execute_remote_and_record(request, target, remote_persistence)
                    .await?;
                Ok(PolicySelectedExecution {
                    policy,
                    reflex_audit_id: None,
                    local_execution: None,
                    remote_execution: Some(remote_execution),
                })
            }
            RoutingPolicyOutcome::Reflex => {
                let reflex_persistence = persistence.reflex.ok_or_else(|| {
                    MembraneRuntimeError::InvalidRequest(
                        "policy-selected reflex handling requires reflex persistence".to_string(),
                    )
                })?;
                let reflex_audit_id = self
                    .append_reflex_audit(
                        &request.episode_id,
                        &policy,
                        reflex_persistence,
                        "policy-selected",
                    )
                    .await?;
                Ok(PolicySelectedExecution {
                    policy,
                    reflex_audit_id: Some(reflex_audit_id),
                    local_execution: None,
                    remote_execution: None,
                })
            }
            RoutingPolicyOutcome::Rejected => Ok(PolicySelectedExecution {
                policy,
                reflex_audit_id: None,
                local_execution: None,
                remote_execution: None,
            }),
        }
    }
}

fn build_membrane_ir(request: &MembraneTaskRequest) -> MembraneIr {
    let task_report = sensitivity::protect_outbound_text(
        &format!("task:{}", request.task_id),
        &request.task_text,
    );
    let context_reports = request
        .context_fragments
        .iter()
        .map(|fragment| {
            (
                fragment,
                sensitivity::protect_outbound_text(&fragment.fragment_id, &fragment.text),
            )
        })
        .collect::<Vec<_>>();

    MembraneIr {
        task: MembraneTaskPayload {
            task_id: request.task_id.clone(),
            episode_id: request.episode_id.clone(),
            text: request.task_text.clone(),
        },
        context_handles: request
            .context_fragments
            .iter()
            .map(|fragment| MembraneContextHandle {
                fragment_id: fragment.fragment_id.clone(),
                text: fragment.text.clone(),
            })
            .collect(),
        protected_references: task_report
            .protected_references
            .into_iter()
            .chain(
                context_reports
                    .iter()
                    .flat_map(|(_, report)| report.protected_references.clone()),
            )
            .collect(),
        boundary: MembraneBoundaryMetadata {
            remote_allowed: request.allow_remote,
            render_mode: MembraneIrRenderMode::PromptV1,
        },
        reconstruction: Some(crate::contracts::MembraneReconstructionHandle {
            handle_id: format!("reconstruct:{}:inline-text-v1", request.task_id),
            strategy: crate::contracts::MembraneReconstructionStrategy::InlineText,
        }),
    }
}

fn render_prompt_from_ir(ir: &MembraneIr) -> String {
    if ir.context_handles.is_empty() {
        return ir.task.text.clone();
    }

    let context = ir
        .context_handles
        .iter()
        .map(|fragment| format!("[{}] {}", fragment.fragment_id, fragment.text))
        .collect::<Vec<_>>()
        .join("\n");
    format!("{}\n\nContext:\n{}", ir.task.text, context)
}

pub(super) fn render_remote_prompt_from_ir(ir: &MembraneIr) -> String {
    let task_text =
        sensitivity::protect_outbound_text(&format!("task:{}", ir.task.task_id), &ir.task.text)
            .outbound_text;

    if ir.context_handles.is_empty() {
        return task_text;
    }

    let context = ir
        .context_handles
        .iter()
        .map(|fragment| {
            let outbound =
                sensitivity::protect_outbound_text(&fragment.fragment_id, &fragment.text)
                    .outbound_text;
            format!("[{}] {}", fragment.fragment_id, outbound)
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("{task_text}\n\nContext:\n{context}")
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

fn validate_policy_task_alignment(
    request: &MembraneTaskRequest,
    policy_request: &RoutingPolicyRequest,
) -> Result<(), MembraneRuntimeError> {
    if request.task_id != policy_request.task_id {
        return Err(MembraneRuntimeError::InvalidRequest(
            "policy-selected execution requires matching task_id values".to_string(),
        ));
    }
    if request.episode_id != policy_request.episode_id {
        return Err(MembraneRuntimeError::InvalidRequest(
            "policy-selected execution requires matching episode_id values".to_string(),
        ));
    }
    if request.allow_remote != policy_request.allow_remote {
        return Err(MembraneRuntimeError::InvalidRequest(
            "policy-selected execution requires matching allow_remote values".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn compile_has_blocked_protected_content(compile: &CompileResult) -> bool {
    compile
        .compiled_task
        .membrane_ir
        .as_ref()
        .is_some_and(|ir| {
            ir.protected_references
                .iter()
                .any(|reference| reference.disposition == MembraneProtectionDisposition::Blocked)
        })
}

pub(super) fn elevate_policy_to_protected_content_reflex(
    mut policy: RoutingPolicyResult,
    compile: &CompileResult,
) -> RoutingPolicyResult {
    let blocked_count = compile
        .compiled_task
        .membrane_ir
        .as_ref()
        .map(|ir| {
            ir.protected_references
                .iter()
                .filter(|reference| reference.disposition == MembraneProtectionDisposition::Blocked)
                .count()
        })
        .unwrap_or(0);
    policy.outcome = RoutingPolicyOutcome::Reflex;
    policy.reflex_reason = Some(RoutingPolicyReflexReason::ProtectedContent);
    policy.caution = false;
    policy.selected_target = None;
    policy
        .findings
        .push(crate::contracts::RoutingPolicyFinding {
            code: "protected-content-blocked".to_string(),
            severity: crate::contracts::RoutingPolicyFindingSeverity::Block,
            detail: format!(
                "blocked protected content remained local for task '{}' ({} blocked reference{})",
                compile.compiled_task.task_id,
                blocked_count,
                if blocked_count == 1 { "" } else { "s" }
            ),
        });
    policy.rationale = Some(
        "blocked protected content requires local-only fallback before remote dispatch".to_string(),
    );
    policy
}

impl<A> MembraneRuntime<A>
where
    A: EmilyApi + ?Sized,
{
    pub(super) async fn append_reflex_audit(
        &self,
        episode_id: &str,
        policy: &RoutingPolicyResult,
        persistence: PolicyReflexPersistence,
        source: &str,
    ) -> Result<String, MembraneRuntimeError> {
        if policy.outcome != RoutingPolicyOutcome::Reflex {
            return Err(MembraneRuntimeError::InvalidState(
                "reflex audit append requires a reflex policy outcome".to_string(),
            ));
        }
        if persistence.audit_id.trim().is_empty() {
            return Err(MembraneRuntimeError::InvalidRequest(
                "reflex audit_id must not be empty".to_string(),
            ));
        }
        let reflex_reason = policy.reflex_reason.ok_or_else(|| {
            MembraneRuntimeError::InvalidState(
                "reflex policy outcomes must include a typed reflex reason".to_string(),
            )
        })?;

        let audit = self
            .emily
            .append_sovereign_audit_record(AppendSovereignAuditRecordRequest {
                audit_id: persistence.audit_id.clone(),
                episode_id: episode_id.to_string(),
                kind: AuditRecordKind::BoundaryEvent,
                ts_unix_ms: persistence.audited_at_unix_ms,
                summary: format!(
                    "reflex blocked remote dispatch for task '{}': {}",
                    policy.task_id,
                    reflex_reason_label(reflex_reason)
                ),
                metadata: SovereignAuditMetadata {
                    remote_episode_id: None,
                    route_decision_id: None,
                    validation_id: None,
                    boundary_profile: Some("reflex".to_string()),
                    metadata: json!({
                        "source": source,
                        "task_id": policy.task_id,
                        "reflex_reason": reflex_reason_label(reflex_reason),
                        "finding_codes": policy
                            .findings
                            .iter()
                            .map(|finding| finding.code.clone())
                            .collect::<Vec<_>>(),
                        "rationale": policy.rationale,
                    }),
                },
            })
            .await?;

        Ok(audit.id)
    }
}

fn reflex_reason_label(reason: RoutingPolicyReflexReason) -> &'static str {
    match reason {
        RoutingPolicyReflexReason::SensitivityBlock => "sensitivity-block",
        RoutingPolicyReflexReason::MissingEpisodeAnchor => "missing-episode-anchor",
        RoutingPolicyReflexReason::EpisodeClosed => "episode-closed",
        RoutingPolicyReflexReason::EarlReflex => "earl-reflex",
        RoutingPolicyReflexReason::EpisodeBlocked => "episode-blocked",
        RoutingPolicyReflexReason::LeakageRisk => "leakage-risk",
        RoutingPolicyReflexReason::ProtectedContent => "protected-content",
        RoutingPolicyReflexReason::BoundaryFailure => "boundary-failure",
    }
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
                severity: to_emily_finding_severity(finding.severity),
                message: finding.detail.clone(),
            })
            .collect(),
        metadata: json!({
            "source": "emily-membrane",
            "mode": "local-only",
            "task_id": validation.task_id.clone(),
            "validated_text": validation.validated_text.clone(),
            "assessments": validation
                .assessments
                .iter()
                .map(|assessment| json!({
                    "category": assessment.category,
                    "status": assessment.status,
                    "summary": assessment.summary,
                }))
                .collect::<Vec<_>>(),
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
    severity: crate::contracts::ValidationFindingSeverity,
) -> EmilyValidationFindingSeverity {
    match severity {
        crate::contracts::ValidationFindingSeverity::Info => EmilyValidationFindingSeverity::Info,
        crate::contracts::ValidationFindingSeverity::Caution => {
            EmilyValidationFindingSeverity::Warning
        }
        crate::contracts::ValidationFindingSeverity::Block => EmilyValidationFindingSeverity::Error,
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
