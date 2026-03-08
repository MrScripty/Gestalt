use serde::{Deserialize, Serialize};

/// Validation output produced by the membrane before local reconstruction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationEnvelope {
    pub task_id: String,
    pub disposition: MembraneValidationDisposition,
    /// Defaults to an empty list when omitted.
    #[serde(default)]
    pub assessments: Vec<ValidationAssessment>,
    /// Defaults to an empty list when omitted.
    #[serde(default)]
    pub findings: Vec<ValidationFinding>,
    pub validated_text: Option<String>,
}

/// High-level validation result for one membrane output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MembraneValidationDisposition {
    Accepted,
    NeedsReview,
    Rejected,
}

/// Structured assessment for one validation category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationAssessment {
    pub category: ValidationCategory,
    pub status: ValidationAssessmentStatus,
    pub summary: String,
}

/// Category evaluated during membrane validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ValidationCategory {
    #[default]
    Coherence,
    Relevance,
    Confidence,
    ProvenanceSufficiency,
}

/// Assessment status for one validation category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationAssessmentStatus {
    Satisfied,
    NeedsReview,
    Failed,
}

/// Human-readable validation finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationFinding {
    pub code: String,
    /// Defaults to `Coherence` when omitted by older payloads.
    #[serde(default)]
    pub category: ValidationCategory,
    /// Defaults to `Info` when omitted by older payloads.
    #[serde(default)]
    pub severity: ValidationFindingSeverity,
    pub detail: String,
}

/// Severity for one structured validation finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ValidationFindingSeverity {
    #[default]
    Info,
    Caution,
    Block,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_envelope_roundtrip_preserves_defaults() {
        let envelope = ValidationEnvelope {
            task_id: "task-1".into(),
            disposition: MembraneValidationDisposition::NeedsReview,
            assessments: vec![ValidationAssessment {
                category: ValidationCategory::Confidence,
                status: ValidationAssessmentStatus::NeedsReview,
                summary: "response is too short for high confidence".into(),
            }],
            findings: vec![ValidationFinding {
                code: "review-required".into(),
                category: ValidationCategory::Confidence,
                severity: ValidationFindingSeverity::Caution,
                detail: "local review required before host output".into(),
            }],
            validated_text: Some("LOCAL: review required".into()),
        };

        let text = serde_json::to_string(&envelope).expect("serialize validation envelope");
        let restored: ValidationEnvelope =
            serde_json::from_str(&text).expect("deserialize validation envelope");
        assert_eq!(restored, envelope);

        let restored_default: ValidationEnvelope = serde_json::from_str(
            r#"{"task_id":"task-2","disposition":"Accepted","validated_text":"safe"}"#,
        )
        .expect("deserialize validation defaults");
        assert!(restored_default.assessments.is_empty());
        assert!(restored_default.findings.is_empty());
    }

    #[test]
    fn validation_finding_roundtrip_preserves_defaults() {
        let finding: ValidationFinding = serde_json::from_str(
            r#"{"code":"legacy-finding","detail":"legacy shape without category"}"#,
        )
        .expect("deserialize legacy validation finding");
        assert_eq!(finding.category, ValidationCategory::Coherence);
        assert_eq!(finding.severity, ValidationFindingSeverity::Info);
    }
}
