use super::{MembraneRuntime, MembraneRuntimeError};
use crate::contracts::{
    CompileResult, DispatchResult, MembraneProtectedReference, MembraneProtectionDisposition,
    MembraneRouteKind, MembraneValidationDisposition, ReconstructionReference,
    ReconstructionResult, ReconstructionSource, ValidationEnvelope, ValidationFinding,
    ValidationFindingSeverity,
};

impl<A> MembraneRuntime<A>
where
    A: emily::EmilyApi + ?Sized,
{
    /// Reconstruct the final host-facing output from a validated result without
    /// additional compile or dispatch provenance.
    pub async fn reconstruct(
        &self,
        validation: &ValidationEnvelope,
    ) -> Result<ReconstructionResult, MembraneRuntimeError> {
        build_reconstruction(None, None, validation)
    }

    /// Reconstruct the final host-facing output from compile, dispatch, and
    /// validation state so the result carries membrane provenance.
    pub async fn reconstruct_with_context(
        &self,
        compile: &CompileResult,
        dispatch: &DispatchResult,
        validation: &ValidationEnvelope,
    ) -> Result<ReconstructionResult, MembraneRuntimeError> {
        if compile.compiled_task.task_id != validation.task_id {
            return Err(MembraneRuntimeError::InvalidRequest(
                "compile task_id must match validation task_id during reconstruction".to_string(),
            ));
        }
        if dispatch.task_id != validation.task_id {
            return Err(MembraneRuntimeError::InvalidRequest(
                "dispatch task_id must match validation task_id during reconstruction".to_string(),
            ));
        }

        build_reconstruction(Some(compile), Some(dispatch), validation)
    }
}

fn build_reconstruction(
    compile: Option<&CompileResult>,
    dispatch: Option<&DispatchResult>,
    validation: &ValidationEnvelope,
) -> Result<ReconstructionResult, MembraneRuntimeError> {
    match validation.disposition {
        MembraneValidationDisposition::Rejected => Err(MembraneRuntimeError::InvalidState(
            "cannot reconstruct from a rejected validation result".to_string(),
        )),
        MembraneValidationDisposition::Accepted | MembraneValidationDisposition::NeedsReview => {
            let protected_references = compile
                .and_then(|compiled| compiled.compiled_task.membrane_ir.as_ref())
                .map(|ir| ir.protected_references.as_slice())
                .unwrap_or(&[]);
            let output_text = validation.validated_text.clone().ok_or_else(|| {
                MembraneRuntimeError::InvalidState(
                    "validated_text is required for reconstruction".to_string(),
                )
            })?;
            Ok(ReconstructionResult {
                task_id: validation.task_id.clone(),
                output_text: render_reconstruction_output(
                    dispatch,
                    validation,
                    &output_text,
                    protected_references,
                ),
                references: build_reconstruction_references(
                    compile,
                    dispatch,
                    validation,
                    protected_references,
                ),
                caution: validation.disposition == MembraneValidationDisposition::NeedsReview,
            })
        }
    }
}

fn build_reconstruction_references(
    compile: Option<&CompileResult>,
    dispatch: Option<&DispatchResult>,
    validation: &ValidationEnvelope,
    protected_references: &[MembraneProtectedReference],
) -> Vec<ReconstructionReference> {
    let mut references = Vec::new();

    if let Some(compile) = compile
        && let Some(ir) = compile.compiled_task.membrane_ir.as_ref()
    {
        if let Some(handle) = ir.reconstruction.as_ref() {
            references.push(ReconstructionReference::reconstruction_handle(
                handle.handle_id.clone(),
                handle.strategy,
            ));
        }

        references.extend(
            ir.context_handles
                .iter()
                .map(|handle| ReconstructionReference {
                    source: ReconstructionSource::LocalContext,
                    reference_id: handle.fragment_id.clone(),
                    summary: Some("admitted membrane context fragment".to_string()),
                }),
        );
    }

    if let Some(dispatch) = dispatch
        && let Some(reference_id) = dispatch.remote_reference.as_ref()
    {
        references.push(ReconstructionReference {
            source: ReconstructionSource::RemoteResult,
            reference_id: reference_id.clone(),
            summary: Some(match dispatch.route {
                MembraneRouteKind::SingleRemote => {
                    "single-remote provider result rendered locally".to_string()
                }
                MembraneRouteKind::MultiRemote => {
                    "multi-remote provider result rendered locally".to_string()
                }
                MembraneRouteKind::LocalOnly => {
                    "unexpected remote reference on local-only dispatch".to_string()
                }
                MembraneRouteKind::Rejected => {
                    "unexpected remote reference on rejected dispatch".to_string()
                }
            }),
        });
    }

    if validation.findings.is_empty()
        && validation.disposition == MembraneValidationDisposition::NeedsReview
    {
        references.push(ReconstructionReference {
            source: ReconstructionSource::ValidationPolicy,
            reference_id: "validation:needs-review".to_string(),
            summary: Some(
                "validation required review without explicit finding metadata".to_string(),
            ),
        });
    } else {
        references.extend(validation.findings.iter().map(validation_reference));
    }

    references.extend(protected_references.iter().map(protected_reference));

    references
}

fn validation_reference(finding: &ValidationFinding) -> ReconstructionReference {
    ReconstructionReference {
        source: ReconstructionSource::ValidationPolicy,
        reference_id: finding.code.clone(),
        summary: Some(format!(
            "[{}] {}",
            finding_severity_label(finding.severity),
            finding.detail
        )),
    }
}

fn render_reconstruction_output(
    dispatch: Option<&DispatchResult>,
    validation: &ValidationEnvelope,
    output_text: &str,
    protected_references: &[MembraneProtectedReference],
) -> String {
    let output_text = restore_protected_output(output_text, protected_references);
    let mut headers = Vec::new();

    if let Some(dispatch) = dispatch {
        match dispatch.route {
            MembraneRouteKind::SingleRemote | MembraneRouteKind::MultiRemote => {
                headers.push(match dispatch.remote_reference.as_deref() {
                    Some(reference_id) => {
                        format!("Membrane rendered remote output from '{reference_id}'.")
                    }
                    None => {
                        "Membrane rendered remote output from a bounded remote result.".to_string()
                    }
                });
            }
            MembraneRouteKind::LocalOnly | MembraneRouteKind::Rejected => {}
        }
    }

    if validation.disposition == MembraneValidationDisposition::NeedsReview {
        headers.push("Review required before relying on this output.".to_string());
    }

    if !validation.findings.is_empty() {
        headers.push(format!(
            "Validation findings: {}",
            validation
                .findings
                .iter()
                .map(render_finding_summary)
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }

    if protected_references
        .iter()
        .any(|reference| reference.disposition == MembraneProtectionDisposition::Blocked)
    {
        headers.push(
            "Protected local content remained withheld during remote reconstruction.".to_string(),
        );
    }

    if headers.is_empty() {
        output_text
    } else {
        format!("{}\n\n{}", headers.join("\n"), output_text)
    }
}

fn protected_reference(reference: &MembraneProtectedReference) -> ReconstructionReference {
    let summary = match reference.disposition {
        MembraneProtectionDisposition::Transformed => format!(
            "transformable {:?} restored locally from '{}'",
            reference.kind, reference.placeholder
        ),
        MembraneProtectionDisposition::Blocked => format!(
            "blocked {:?} remained withheld behind '{}'",
            reference.kind, reference.placeholder
        ),
    };
    ReconstructionReference::protected_local(reference.reference_id.clone(), summary)
}

fn restore_protected_output(
    output_text: &str,
    protected_references: &[MembraneProtectedReference],
) -> String {
    let mut restored = output_text.to_string();

    for reference in protected_references {
        match reference.disposition {
            MembraneProtectionDisposition::Transformed => {
                if let Some(local_text) = reference.local_text.as_deref() {
                    restored = restored.replace(&reference.placeholder, local_text);
                }
            }
            MembraneProtectionDisposition::Blocked => {
                restored = restored.replace(&reference.placeholder, "[WITHHELD_SECRET]");
            }
        }
    }

    restored
}

fn render_finding_summary(finding: &ValidationFinding) -> String {
    format!(
        "[{}] {}",
        finding_severity_label(finding.severity),
        finding.code
    )
}

fn finding_severity_label(severity: ValidationFindingSeverity) -> &'static str {
    match severity {
        ValidationFindingSeverity::Info => "info",
        ValidationFindingSeverity::Caution => "caution",
        ValidationFindingSeverity::Block => "block",
    }
}
