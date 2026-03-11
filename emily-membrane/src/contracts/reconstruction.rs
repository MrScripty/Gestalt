use crate::contracts::MembraneReconstructionStrategy;
use serde::{Deserialize, Serialize};

/// Final local reconstruction result returned to the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconstructionResult {
    pub task_id: String,
    pub output_text: String,
    /// Defaults to an empty list when omitted.
    #[serde(default)]
    pub references: Vec<ReconstructionReference>,
    /// Defaults to `false` when omitted.
    #[serde(default)]
    pub caution: bool,
}

/// Provenance reference captured during reconstruction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconstructionReference {
    pub source: ReconstructionSource,
    pub reference_id: String,
    /// Defaults to `None` when omitted by older payloads.
    #[serde(default)]
    pub summary: Option<String>,
}

/// Source category for one reconstruction reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReconstructionSource {
    LocalContext,
    RemoteResult,
    ValidationPolicy,
    ReconstructionHandle,
    ProtectedLocal,
}

impl ReconstructionReference {
    pub(crate) fn reconstruction_handle(
        handle_id: String,
        strategy: MembraneReconstructionStrategy,
    ) -> Self {
        Self {
            source: ReconstructionSource::ReconstructionHandle,
            reference_id: handle_id,
            summary: Some(match strategy {
                MembraneReconstructionStrategy::InlineText => {
                    "inline text reconstruction".to_string()
                }
            }),
        }
    }

    pub(crate) fn protected_local(reference_id: String, summary: String) -> Self {
        Self {
            source: ReconstructionSource::ProtectedLocal,
            reference_id,
            summary: Some(summary),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ReconstructionReference, ReconstructionResult, ReconstructionSource};

    #[test]
    fn reconstruction_result_roundtrip_preserves_defaults() {
        let result = ReconstructionResult {
            task_id: "task-1".into(),
            output_text: "final response".into(),
            references: vec![ReconstructionReference {
                source: ReconstructionSource::LocalContext,
                reference_id: "ctx-1".into(),
                summary: Some("admitted context fragment".into()),
            }],
            caution: true,
        };

        let text = serde_json::to_string(&result).expect("serialize reconstruction result");
        let restored: ReconstructionResult =
            serde_json::from_str(&text).expect("deserialize reconstruction result");
        assert_eq!(restored, result);

        let restored_default: ReconstructionResult =
            serde_json::from_str(r#"{"task_id":"task-2","output_text":"plain response"}"#)
                .expect("deserialize reconstruction defaults");
        assert!(restored_default.references.is_empty());
        assert!(!restored_default.caution);
    }

    #[test]
    fn reconstruction_reference_defaults_summary_to_none() {
        let restored: ReconstructionReference = serde_json::from_str(
            r#"{"source":"ValidationPolicy","reference_id":"review-required"}"#,
        )
        .expect("deserialize reconstruction reference defaults");
        assert_eq!(restored.summary, None);
    }
}
