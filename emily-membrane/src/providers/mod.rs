//! Provider boundary for membrane-owned remote dispatch.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::error::Error;
use std::fmt::{Display, Formatter};

#[cfg(feature = "pantograph")]
mod pantograph;

#[cfg(feature = "pantograph")]
pub use pantograph::{
    PantographProviderConfig, PantographWorkflowBinding, PantographWorkflowProvider,
};

/// Remote dispatch request issued by the membrane to one provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderDispatchRequest {
    pub provider_request_id: String,
    pub task_id: String,
    pub episode_id: String,
    pub target: ProviderTarget,
    pub dispatch_kind: ProviderDispatchKind,
    pub bounded_payload: String,
    /// Defaults to an empty list when omitted.
    #[serde(default)]
    pub context_fragment_ids: Vec<String>,
    pub metadata: Value,
}

/// Provider-selected remote target descriptor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderTarget {
    pub provider_id: String,
    pub model_id: Option<String>,
    pub profile_id: Option<String>,
    /// Defaults to an empty list when omitted.
    #[serde(default)]
    pub capability_tags: Vec<String>,
    pub metadata: Value,
}

/// Dispatch-shape label for one provider call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderDispatchKind {
    Prompt,
    Program,
}

/// Provider response returned to the membrane runtime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderDispatchResult {
    pub provider_request_id: String,
    pub provider_id: String,
    pub status: ProviderDispatchStatus,
    pub output_text: String,
    pub metadata: Value,
}

/// Result status for one provider dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderDispatchStatus {
    Completed,
    Failed,
    Rejected,
}

/// Membrane-owned provider adapter trait.
#[async_trait]
pub trait MembraneProvider: Send + Sync {
    /// Stable provider identifier used for routing and audit metadata.
    fn provider_id(&self) -> &str;

    /// Execute one membrane-owned remote dispatch request.
    async fn dispatch(
        &self,
        request: ProviderDispatchRequest,
    ) -> Result<ProviderDispatchResult, MembraneProviderError>;
}

/// Provider-facing error surface owned by the membrane crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MembraneProviderError {
    InvalidRequest(String),
    Unavailable(String),
    Execution(String),
}

impl Display for MembraneProviderError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest(message) => write!(f, "invalid provider request: {message}"),
            Self::Unavailable(message) => write!(f, "provider unavailable: {message}"),
            Self::Execution(message) => write!(f, "provider execution failed: {message}"),
        }
    }
}

impl Error for MembraneProviderError {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn provider_dispatch_request_roundtrip_preserves_defaults() {
        let request = ProviderDispatchRequest {
            provider_request_id: "provider-request-1".into(),
            task_id: "task-1".into(),
            episode_id: "episode-1".into(),
            target: ProviderTarget {
                provider_id: "provider-a".into(),
                model_id: Some("model-x".into()),
                profile_id: Some("reasoning".into()),
                capability_tags: vec!["analysis".into()],
                metadata: json!({"priority": 1}),
            },
            dispatch_kind: ProviderDispatchKind::Prompt,
            bounded_payload: "bounded prompt".into(),
            context_fragment_ids: vec!["ctx-1".into()],
            metadata: json!({"source": "membrane"}),
        };

        let text = serde_json::to_string(&request).expect("serialize provider dispatch request");
        let restored: ProviderDispatchRequest =
            serde_json::from_str(&text).expect("deserialize provider dispatch request");
        assert_eq!(restored, request);

        let restored_default: ProviderDispatchRequest = serde_json::from_str(
            r#"{
                "provider_request_id":"provider-request-2",
                "task_id":"task-2",
                "episode_id":"episode-2",
                "target":{"provider_id":"provider-b","metadata":{}},
                "dispatch_kind":"Program",
                "bounded_payload":"program",
                "metadata":{}
            }"#,
        )
        .expect("deserialize provider dispatch request defaults");
        assert!(restored_default.context_fragment_ids.is_empty());
        assert!(restored_default.target.capability_tags.is_empty());
        assert_eq!(restored_default.target.model_id, None);
        assert_eq!(restored_default.target.profile_id, None);
    }

    #[test]
    fn provider_dispatch_result_roundtrip() {
        let result = ProviderDispatchResult {
            provider_request_id: "provider-request-1".into(),
            provider_id: "provider-a".into(),
            status: ProviderDispatchStatus::Completed,
            output_text: "remote result".into(),
            metadata: json!({"latency_ms": 12}),
        };

        let text = serde_json::to_string(&result).expect("serialize provider dispatch result");
        let restored: ProviderDispatchResult =
            serde_json::from_str(&text).expect("deserialize provider dispatch result");
        assert_eq!(restored, result);
    }
}
