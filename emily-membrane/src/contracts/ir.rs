use serde::{Deserialize, Serialize};

/// Typed membrane IR produced before provider or local rendering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembraneIr {
    pub task: MembraneTaskPayload,
    /// Defaults to an empty list when omitted.
    #[serde(default)]
    pub context_handles: Vec<MembraneContextHandle>,
    pub boundary: MembraneBoundaryMetadata,
    pub reconstruction: Option<MembraneReconstructionHandle>,
}

/// Primary task payload carried by the membrane IR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembraneTaskPayload {
    pub task_id: String,
    pub episode_id: String,
    pub text: String,
}

/// Reference to one context fragment already admitted into the membrane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembraneContextHandle {
    pub fragment_id: String,
    pub text: String,
}

/// Boundary metadata that remains meaningful before transport rendering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembraneBoundaryMetadata {
    pub remote_allowed: bool,
    pub render_mode: MembraneIrRenderMode,
}

/// Render strategy used to derive the current execution payload from IR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MembraneIrRenderMode {
    PromptV1,
}

/// Optional local reconstruction handle reserved for later provenance depth.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembraneReconstructionHandle {
    pub handle_id: String,
    pub strategy: MembraneReconstructionStrategy,
}

/// Requested reconstruction strategy for one IR payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MembraneReconstructionStrategy {
    InlineText,
}

#[cfg(test)]
mod tests {
    use super::{
        MembraneBoundaryMetadata, MembraneContextHandle, MembraneIr, MembraneIrRenderMode,
        MembraneReconstructionHandle, MembraneReconstructionStrategy, MembraneTaskPayload,
    };

    #[test]
    fn membrane_ir_roundtrip_preserves_defaults_and_handles() {
        let ir = MembraneIr {
            task: MembraneTaskPayload {
                task_id: "task-1".into(),
                episode_id: "episode-1".into(),
                text: "Summarize the provider context.".into(),
            },
            context_handles: vec![MembraneContextHandle {
                fragment_id: "ctx-1".into(),
                text: "provider context".into(),
            }],
            boundary: MembraneBoundaryMetadata {
                remote_allowed: false,
                render_mode: MembraneIrRenderMode::PromptV1,
            },
            reconstruction: Some(MembraneReconstructionHandle {
                handle_id: "reconstruct-1".into(),
                strategy: MembraneReconstructionStrategy::InlineText,
            }),
        };

        let text = serde_json::to_string(&ir).expect("serialize membrane ir");
        let restored: MembraneIr = serde_json::from_str(&text).expect("deserialize membrane ir");
        assert_eq!(restored, ir);

        let restored_default: MembraneIr = serde_json::from_str(
            r#"{
                "task":{"task_id":"task-2","episode_id":"episode-2","text":"plain task"},
                "boundary":{"remote_allowed":true,"render_mode":"PromptV1"},
                "reconstruction":null
            }"#,
        )
        .expect("deserialize membrane ir defaults");
        assert!(restored_default.context_handles.is_empty());
    }
}
