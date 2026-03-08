use super::{MembraneRuntime, MembraneRuntimeError};
use crate::contracts::{
    DispatchResult, DispatchStatus, MembraneValidationDisposition, ValidationAssessment,
    ValidationAssessmentStatus, ValidationCategory, ValidationEnvelope, ValidationFinding,
    ValidationFindingSeverity,
};

const MIN_CONFIDENCE_BODY_CHARS: usize = 12;
const MIN_RELEVANCE_BODY_CHARS: usize = 24;
const LOCAL_PREFIX: &str = "LOCAL:";

impl<A> MembraneRuntime<A>
where
    A: emily::EmilyApi + ?Sized,
{
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

        Ok(evaluate_local_validation(dispatch))
    }
}

fn evaluate_local_validation(dispatch: &DispatchResult) -> ValidationEnvelope {
    let trimmed = dispatch.response_text.trim();
    let mut assessments = Vec::new();
    let mut findings = Vec::new();

    if trimmed.is_empty() {
        assessments.push(failed_assessment(
            ValidationCategory::Coherence,
            "local dispatch returned no text",
        ));
        assessments.push(failed_assessment(
            ValidationCategory::Relevance,
            "relevance cannot be established without local output text",
        ));
        assessments.push(failed_assessment(
            ValidationCategory::Confidence,
            "confidence is unavailable because local output text is empty",
        ));
        assessments.push(review_assessment(
            ValidationCategory::ProvenanceSufficiency,
            "provenance is incomplete because no validated local output is available",
        ));
        findings.push(block_finding(
            "empty-local-response",
            ValidationCategory::Coherence,
            "local dispatch returned an empty response",
        ));
        return ValidationEnvelope {
            task_id: dispatch.task_id.clone(),
            disposition: validation_disposition(&assessments),
            assessments,
            findings,
            validated_text: None,
        };
    }

    let body = trimmed.strip_prefix(LOCAL_PREFIX).unwrap_or(trimmed).trim();

    if trimmed.starts_with(LOCAL_PREFIX) {
        assessments.push(satisfied_assessment(
            ValidationCategory::Coherence,
            "local output keeps the expected local adapter prefix",
        ));
        assessments.push(satisfied_assessment(
            ValidationCategory::ProvenanceSufficiency,
            "local output preserves a direct local provenance marker",
        ));
    } else {
        assessments.push(review_assessment(
            ValidationCategory::Coherence,
            "local output is missing the expected local adapter prefix",
        ));
        assessments.push(review_assessment(
            ValidationCategory::ProvenanceSufficiency,
            "local output does not preserve an explicit local provenance marker",
        ));
        findings.push(caution_finding(
            "local-prefix-missing",
            ValidationCategory::Coherence,
            "local output is missing the expected 'LOCAL:' prefix",
        ));
        findings.push(caution_finding(
            "local-provenance-unclear",
            ValidationCategory::ProvenanceSufficiency,
            "local output does not retain an explicit local provenance marker",
        ));
    }

    if body.is_empty() {
        assessments.push(failed_assessment(
            ValidationCategory::Relevance,
            "local output contains no task-facing content after the local prefix",
        ));
        assessments.push(failed_assessment(
            ValidationCategory::Confidence,
            "local output is too thin to support confidence",
        ));
        findings.push(block_finding(
            "local-body-empty",
            ValidationCategory::Relevance,
            "local output does not contain task-facing content after the local prefix",
        ));
    } else {
        if body.len() >= MIN_RELEVANCE_BODY_CHARS {
            assessments.push(satisfied_assessment(
                ValidationCategory::Relevance,
                "local output has enough task-facing content for first-pass relevance",
            ));
        } else {
            assessments.push(review_assessment(
                ValidationCategory::Relevance,
                "local output is brief enough that relevance should be reviewed",
            ));
            findings.push(caution_finding(
                "local-response-brief",
                ValidationCategory::Relevance,
                "local output is brief enough that relevance should be reviewed",
            ));
        }

        if body.len() >= MIN_CONFIDENCE_BODY_CHARS {
            assessments.push(satisfied_assessment(
                ValidationCategory::Confidence,
                "local output length supports first-pass confidence",
            ));
        } else {
            assessments.push(review_assessment(
                ValidationCategory::Confidence,
                "local output is too short for high confidence",
            ));
            findings.push(caution_finding(
                "local-confidence-low",
                ValidationCategory::Confidence,
                "local output is too short for high confidence",
            ));
        }
    }

    let disposition = validation_disposition(&assessments);
    ValidationEnvelope {
        task_id: dispatch.task_id.clone(),
        disposition,
        assessments,
        findings,
        validated_text: match disposition {
            MembraneValidationDisposition::Rejected => None,
            MembraneValidationDisposition::Accepted
            | MembraneValidationDisposition::NeedsReview => Some(trimmed.to_string()),
        },
    }
}

fn validation_disposition(assessments: &[ValidationAssessment]) -> MembraneValidationDisposition {
    if assessments
        .iter()
        .any(|assessment| assessment.status == ValidationAssessmentStatus::Failed)
    {
        return MembraneValidationDisposition::Rejected;
    }
    if assessments
        .iter()
        .any(|assessment| assessment.status == ValidationAssessmentStatus::NeedsReview)
    {
        return MembraneValidationDisposition::NeedsReview;
    }
    MembraneValidationDisposition::Accepted
}

fn satisfied_assessment(category: ValidationCategory, summary: &str) -> ValidationAssessment {
    ValidationAssessment {
        category,
        status: ValidationAssessmentStatus::Satisfied,
        summary: summary.to_string(),
    }
}

fn review_assessment(category: ValidationCategory, summary: &str) -> ValidationAssessment {
    ValidationAssessment {
        category,
        status: ValidationAssessmentStatus::NeedsReview,
        summary: summary.to_string(),
    }
}

fn failed_assessment(category: ValidationCategory, summary: &str) -> ValidationAssessment {
    ValidationAssessment {
        category,
        status: ValidationAssessmentStatus::Failed,
        summary: summary.to_string(),
    }
}

fn caution_finding(code: &str, category: ValidationCategory, detail: &str) -> ValidationFinding {
    ValidationFinding {
        code: code.to_string(),
        category,
        severity: ValidationFindingSeverity::Caution,
        detail: detail.to_string(),
    }
}

fn block_finding(code: &str, category: ValidationCategory, detail: &str) -> ValidationFinding {
    ValidationFinding {
        code: code.to_string(),
        category,
        severity: ValidationFindingSeverity::Block,
        detail: detail.to_string(),
    }
}
