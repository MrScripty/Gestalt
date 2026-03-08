//! Provider boundary for membrane-owned remote dispatch.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::Arc;

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

/// Host-supplied provider registration entry for registry-backed routing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegisteredProviderTarget {
    pub target: ProviderTarget,
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

/// Registry abstraction for host-supplied provider lookup.
pub trait MembraneProviderRegistry: Send + Sync {
    /// Resolve one provider by its stable provider identifier.
    fn provider(&self, provider_id: &str) -> Option<Arc<dyn MembraneProvider>>;

    /// Return all registered targets available for routing decisions.
    fn targets(&self) -> Vec<RegisteredProviderTarget>;
}

/// In-memory registry for membrane-owned provider lookup.
pub struct InMemoryProviderRegistry {
    providers: HashMap<String, RegisteredProvider>,
}

struct RegisteredProvider {
    target: RegisteredProviderTarget,
    provider: Arc<dyn MembraneProvider>,
}

impl InMemoryProviderRegistry {
    /// Build a registry from an iterator of injected providers.
    pub fn new<I>(providers: I) -> Self
    where
        I: IntoIterator<Item = Arc<dyn MembraneProvider>>,
    {
        let registered = providers.into_iter().map(|provider| {
            (
                RegisteredProviderTarget {
                    target: ProviderTarget {
                        provider_id: provider.provider_id().to_string(),
                        model_id: None,
                        profile_id: None,
                        capability_tags: Vec::new(),
                        metadata: Value::Object(Default::default()),
                    },
                },
                provider,
            )
        });
        Self::with_targets(registered)
    }

    /// Build a registry from explicit target metadata and providers.
    pub fn with_targets<I>(providers: I) -> Self
    where
        I: IntoIterator<Item = (RegisteredProviderTarget, Arc<dyn MembraneProvider>)>,
    {
        let mut entries = HashMap::new();
        for (target, provider) in providers {
            entries.insert(
                provider.provider_id().to_string(),
                RegisteredProvider { target, provider },
            );
        }
        Self { providers: entries }
    }

    /// Build a registry from one injected provider.
    pub fn single(provider: Arc<dyn MembraneProvider>) -> Self {
        Self::new([provider])
    }

    /// Build a registry from one explicit target and provider pair.
    pub fn single_target(
        target: RegisteredProviderTarget,
        provider: Arc<dyn MembraneProvider>,
    ) -> Self {
        Self::with_targets([(target, provider)])
    }
}

impl MembraneProviderRegistry for InMemoryProviderRegistry {
    fn provider(&self, provider_id: &str) -> Option<Arc<dyn MembraneProvider>> {
        self.providers
            .get(provider_id)
            .map(|entry| entry.provider.clone())
    }

    fn targets(&self) -> Vec<RegisteredProviderTarget> {
        let mut targets: Vec<_> = self
            .providers
            .values()
            .map(|entry| entry.target.clone())
            .collect();
        targets.sort_by(|left, right| {
            left.target
                .provider_id
                .cmp(&right.target.provider_id)
                .then_with(|| left.target.profile_id.cmp(&right.target.profile_id))
                .then_with(|| left.target.model_id.cmp(&right.target.model_id))
        });
        targets
    }
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

    struct ExampleProvider {
        provider_id: &'static str,
    }

    #[async_trait]
    impl MembraneProvider for ExampleProvider {
        fn provider_id(&self) -> &str {
            self.provider_id
        }

        async fn dispatch(
            &self,
            request: ProviderDispatchRequest,
        ) -> Result<ProviderDispatchResult, MembraneProviderError> {
            Ok(ProviderDispatchResult {
                provider_request_id: request.provider_request_id,
                provider_id: self.provider_id().to_string(),
                status: ProviderDispatchStatus::Completed,
                output_text: "ok".to_string(),
                metadata: json!({}),
            })
        }
    }

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

    #[test]
    fn in_memory_registry_resolves_provider_by_id() {
        let registry = InMemoryProviderRegistry::new([
            Arc::new(ExampleProvider {
                provider_id: "provider-a",
            }) as Arc<dyn MembraneProvider>,
            Arc::new(ExampleProvider {
                provider_id: "provider-b",
            }) as Arc<dyn MembraneProvider>,
        ]);

        assert!(registry.provider("provider-a").is_some());
        assert!(registry.provider("provider-b").is_some());
        assert!(registry.provider("missing").is_none());
    }

    #[test]
    fn in_memory_registry_preserves_registered_target_metadata() {
        let registry = InMemoryProviderRegistry::single_target(
            RegisteredProviderTarget {
                target: ProviderTarget {
                    provider_id: "provider-a".to_string(),
                    model_id: Some("model-a".to_string()),
                    profile_id: Some("reasoning".to_string()),
                    capability_tags: vec!["analysis".to_string()],
                    metadata: json!({"rank": 1}),
                },
            },
            Arc::new(ExampleProvider {
                provider_id: "provider-a",
            }) as Arc<dyn MembraneProvider>,
        );

        let targets = registry.targets();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].target.provider_id, "provider-a");
        assert_eq!(targets[0].target.profile_id.as_deref(), Some("reasoning"));
        assert_eq!(targets[0].target.capability_tags, vec!["analysis"]);
    }
}
