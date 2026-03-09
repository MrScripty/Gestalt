use crate::emily_seed::{
    EmilySeedError, SYNTHETIC_AGENT_ROUND_DATASET, builtin_seed_corpus, seed_builtin_corpus,
};
use crate::pantograph_host::{
    PantographReasoningRuntimeConfig, build_membrane_provider_registry_from_env,
    default_reasoning_model_path_for_root,
};
use emily::api::EmilyApi;
use emily::error::EmilyError;
use emily::model::{
    ContextQuery, CreateEpisodeRequest, DatabaseLocator, RemoteEpisodeState, RoutingDecisionKind,
    ValidationDecision,
};
use emily::runtime::EmilyRuntime;
use emily::store::surreal::SurrealEmilyStore;
use emily_membrane::contracts::{
    ContextFragment, MembraneTaskRequest, PolicyExecutionPersistence, RemoteRoutingPreference,
    RoutingPolicyOutcome, RoutingPolicyRequest, RoutingSensitivity,
};
use emily_membrane::runtime::{MembraneRuntime, MembraneRuntimeError};
use pantograph_workflow_service::{
    FileSystemWorkflowGraphStore, GraphEdge, GraphNode, Position, WorkflowGraphAddEdgeRequest,
    WorkflowGraphAddNodeRequest, WorkflowGraphEditSessionCloseRequest,
    WorkflowGraphEditSessionCreateRequest, WorkflowGraphLoadRequest,
    WorkflowGraphRemoveEdgeRequest, WorkflowGraphRemoveNodeRequest, WorkflowGraphSaveRequest,
    WorkflowGraphUpdateNodeDataRequest, WorkflowService,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::time::{Duration, sleep};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum PantographReasoningProbeError {
    #[error(
        "Pantograph reasoning workflow is not configured; set GESTALT_PANTOGRAPH_REASONING_WORKFLOW_ID"
    )]
    MissingReasoningWorkflow,
    #[error("unknown Emily seed corpus '{label}'")]
    UnknownCorpus { label: String },
    #[error("failed resetting Emily storage path {path}: {source}")]
    ResetStorage {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Emily(#[from] EmilyError),
    #[error(transparent)]
    Membrane(#[from] MembraneRuntimeError),
    #[error(transparent)]
    Seed(#[from] EmilySeedError),
    #[error("{0}")]
    Host(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PantographReasoningProbeRequest {
    pub dataset: String,
    pub storage_path: PathBuf,
    pub namespace: String,
    pub database: String,
    pub task_text: String,
    pub query_text: String,
    pub top_k: usize,
    pub reset: bool,
    pub reseed: bool,
}

impl Default for PantographReasoningProbeRequest {
    fn default() -> Self {
        Self {
            dataset: SYNTHETIC_AGENT_ROUND_DATASET.to_string(),
            storage_path: std::env::temp_dir().join("gestalt-emily-reasoning-probe"),
            namespace: "gestalt_reasoning_probe".to_string(),
            database: "default".to_string(),
            task_text:
                "Summarize the likely cause and recommended fix for the failing provider registry check."
                    .to_string(),
            query_text: "provider registry capability tags failing reasoning summary".to_string(),
            top_k: 3,
            reset: true,
            reseed: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PantographReasoningValidationReport {
    pub workflow_id: String,
    pub provider_id: String,
    pub profile_id: String,
    pub configured_model_id: String,
    pub dataset: String,
    pub saved_workflow_path: String,
    pub updated_node_id: String,
    pub updated_node_type: String,
    pub episode_id: String,
    pub context_fragment_count: usize,
    pub attempts: usize,
    pub policy_outcome: RoutingPolicyOutcome,
    pub route_decision_id: String,
    pub remote_episode_id: String,
    pub validation_id: String,
    pub provider_request_id: String,
    pub latest_route_kind: Option<RoutingDecisionKind>,
    pub latest_remote_state: Option<RemoteEpisodeState>,
    pub latest_validation_decision: Option<ValidationDecision>,
    pub audit_count: usize,
    pub response_preview: String,
}

pub async fn run_reasoning_probe(
    request: PantographReasoningProbeRequest,
) -> Result<PantographReasoningValidationReport, PantographReasoningProbeError> {
    let Some(config) = PantographReasoningRuntimeConfig::from_env()
        .map_err(PantographReasoningProbeError::Host)?
    else {
        return Err(PantographReasoningProbeError::MissingReasoningWorkflow);
    };
    let Some(corpus) = builtin_seed_corpus(&request.dataset) else {
        return Err(PantographReasoningProbeError::UnknownCorpus {
            label: request.dataset.clone(),
        });
    };

    if request.reset {
        match std::fs::remove_dir_all(&request.storage_path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(PantographReasoningProbeError::ResetStorage {
                    path: request.storage_path.clone(),
                    source: error,
                });
            }
        }
    }

    let (saved_workflow_path, updated_node_id, updated_node_type) =
        update_reasoning_workflow_graph(&config).await?;
    let provider_registry = build_membrane_provider_registry_from_env()
        .map_err(PantographReasoningProbeError::Host)?
        .ok_or(PantographReasoningProbeError::MissingReasoningWorkflow)?;

    let runtime = Arc::new(EmilyRuntime::new(Arc::new(SurrealEmilyStore::new())));
    runtime
        .open_db(DatabaseLocator {
            storage_path: request.storage_path.clone(),
            namespace: request.namespace.clone(),
            database: request.database.clone(),
        })
        .await?;

    if request.reseed {
        let _ = seed_builtin_corpus(runtime.as_ref(), &request.dataset).await?;
    }

    let stream_id = corpus
        .text_objects
        .first()
        .map(|object| object.stream_id.clone())
        .ok_or_else(|| PantographReasoningProbeError::UnknownCorpus {
            label: request.dataset.clone(),
        })?;
    let context_packet = runtime
        .query_context(ContextQuery {
            stream_id: Some(stream_id.clone()),
            query_text: request.query_text.clone(),
            top_k: request.top_k,
            neighbor_depth: 1,
        })
        .await?;
    let context_fragments = context_packet
        .items
        .iter()
        .map(|item| ContextFragment {
            fragment_id: item.object.id.clone(),
            text: item.object.text.clone(),
        })
        .collect::<Vec<_>>();

    let membrane = MembraneRuntime::with_provider_registry(runtime.clone(), provider_registry);
    let mut last_not_ready_error = None;
    for attempt in 1..=3 {
        let episode_id = format!("pantograph-reasoning:{}:attempt-{attempt}", Uuid::new_v4());
        let now = current_unix_ms();
        let _ = runtime
            .create_episode(CreateEpisodeRequest {
                episode_id: episode_id.clone(),
                stream_id: Some(stream_id.clone()),
                source_kind: "gestalt-pantograph-reasoning-probe".to_string(),
                episode_kind: "reasoning_probe".to_string(),
                started_at_unix_ms: now,
                intent: Some(request.task_text.clone()),
                metadata: json!({
                    "dataset": request.dataset,
                    "query_text": request.query_text,
                    "workflow_id": config.workflow_id,
                    "provider_id": config.provider_id,
                    "attempt": attempt,
                }),
            })
            .await?;

        let task_id = format!("reasoning-probe-task:{}", Uuid::new_v4());
        let route_decision_id = format!("{episode_id}:route");
        let remote_episode_id = format!("{episode_id}:remote");
        let validation_id = format!("{episode_id}:validation");
        let provider_request_id = format!("{episode_id}:provider");

        let execution = membrane
            .execute_with_policy_and_record(
                MembraneTaskRequest {
                    task_id: task_id.clone(),
                    episode_id: episode_id.clone(),
                    task_text: request.task_text.clone(),
                    context_fragments: context_fragments.clone(),
                    allow_remote: true,
                },
                RoutingPolicyRequest {
                    task_id: task_id.clone(),
                    episode_id: episode_id.clone(),
                    allow_remote: true,
                    sensitivity: RoutingSensitivity::Normal,
                    preference: RemoteRoutingPreference {
                        provider_id: Some(config.provider_id.clone()),
                        profile_id: Some(config.profile_id.clone()),
                        required_capability_tags: config.capability_tags.clone(),
                        preferred_provider_classes: Vec::new(),
                        max_latency_class: None,
                        max_cost_class: None,
                        minimum_validation_compatibility: None,
                    },
                },
                PolicyExecutionPersistence {
                    local: None,
                    remote: Some(emily_membrane::contracts::RemoteExecutionPersistence {
                        route_decision_id: route_decision_id.clone(),
                        route_decided_at_unix_ms: now + 1,
                        provider_request_id: provider_request_id.clone(),
                        remote_episode_id: remote_episode_id.clone(),
                        remote_dispatched_at_unix_ms: now + 2,
                        validation_id: validation_id.clone(),
                        validated_at_unix_ms: now + 3,
                    }),
                },
            )
            .await;

        let execution = match execution {
            Ok(execution) => execution,
            Err(error) if error.to_string().contains("LLM server is not ready") && attempt < 3 => {
                last_not_ready_error = Some(error.to_string());
                sleep(Duration::from_secs(3)).await;
                continue;
            }
            Err(error) => {
                let _ = runtime.close_db().await;
                return Err(error.into());
            }
        };

        let remote_execution = execution.remote_execution.as_ref().ok_or_else(|| {
            PantographReasoningProbeError::Host(format!(
                "reasoning probe did not execute a remote path; policy outcome was {:?}",
                execution.policy.outcome
            ))
        })?;

        let routes = runtime.routing_decisions_for_episode(&episode_id).await?;
        let remote_episodes = runtime.remote_episodes_for_episode(&episode_id).await?;
        let validations = runtime.validation_outcomes_for_episode(&episode_id).await?;
        let audits = runtime
            .sovereign_audit_records_for_episode(&episode_id)
            .await?;
        let _ = runtime.close_db().await;

        return Ok(PantographReasoningValidationReport {
            workflow_id: config.workflow_id,
            provider_id: config.provider_id,
            profile_id: config.profile_id,
            configured_model_id: config.model_id,
            dataset: request.dataset,
            saved_workflow_path,
            updated_node_id,
            updated_node_type,
            episode_id,
            context_fragment_count: context_fragments.len(),
            attempts: attempt,
            policy_outcome: execution.policy.outcome,
            route_decision_id: remote_execution.route_decision_id.clone(),
            remote_episode_id: remote_execution.remote_episode_id.clone(),
            validation_id: remote_execution.validation_id.clone(),
            provider_request_id: remote_execution.provider_request_id.clone(),
            latest_route_kind: routes.last().map(|record| record.kind),
            latest_remote_state: remote_episodes.last().map(|record| record.state),
            latest_validation_decision: validations.last().map(|record| record.decision),
            audit_count: audits.len(),
            response_preview: preview_text(&remote_execution.reconstruction.output_text, 240),
        });
    }

    let _ = runtime.close_db().await;
    Err(PantographReasoningProbeError::Host(
        last_not_ready_error.unwrap_or_else(|| "LLM server remained unavailable".to_string()),
    ))
}

async fn update_reasoning_workflow_graph(
    config: &PantographReasoningRuntimeConfig,
) -> Result<(String, String, String), PantographReasoningProbeError> {
    let workflow_service = WorkflowService::new();
    let store = FileSystemWorkflowGraphStore::new(config.pantograph_root.clone());
    let workflow_path = format!(".pantograph/workflows/{}.json", config.workflow_id);
    let workflow_file = workflow_service
        .workflow_graph_load(
            &store,
            WorkflowGraphLoadRequest {
                path: workflow_path,
            },
        )
        .map_err(|error| {
            PantographReasoningProbeError::Host(format!("workflow graph load failed: {error}"))
        })?;

    let edit_session = workflow_service
        .workflow_graph_create_edit_session(WorkflowGraphEditSessionCreateRequest {
            graph: workflow_file.graph,
        })
        .await
        .map_err(|error| {
            PantographReasoningProbeError::Host(format!(
                "workflow edit-session create failed: {error}"
            ))
        })?;

    let session_graph = workflow_service
        .workflow_graph_get_edit_session_graph(
            pantograph_workflow_service::WorkflowGraphEditSessionGraphRequest {
                session_id: edit_session.session_id.clone(),
            },
        )
        .await
        .map_err(|error| {
            PantographReasoningProbeError::Host(format!(
                "workflow edit-session graph load failed: {error}"
            ))
        })?;

    let model_path =
        default_reasoning_model_path_for_root(&config.pantograph_root).ok_or_else(|| {
            PantographReasoningProbeError::Host(format!(
                "default reasoning model path is missing under {}",
                config.pantograph_root.display()
            ))
        })?;

    let input_node = session_graph
        .graph
        .nodes
        .iter()
        .find(|node| {
            (node.node_type == "linked-input" || node.node_type == "text-input")
                && node.id != "system-prompt"
        })
        .cloned()
        .ok_or_else(|| {
            PantographReasoningProbeError::Host(
                "reasoning workflow is missing a prompt input node".to_string(),
            )
        })?;
    let system_node = session_graph
        .graph
        .nodes
        .iter()
        .find(|node| {
            node.id == "system-prompt" || node.data.get("label") == Some(&json!("System Prompt"))
        })
        .cloned()
        .ok_or_else(|| {
            PantographReasoningProbeError::Host(
                "reasoning workflow is missing a system prompt input node".to_string(),
            )
        })?;
    let text_output_node = session_graph
        .graph
        .nodes
        .iter()
        .find(|node| node.node_type == "text-output")
        .cloned()
        .ok_or_else(|| {
            PantographReasoningProbeError::Host(
                "reasoning workflow is missing a text-output node".to_string(),
            )
        })?;

    let mut updated_graph = session_graph.clone();

    let nodes_to_remove = updated_graph
        .graph
        .nodes
        .iter()
        .filter(|node| {
            matches!(
                node.node_type.as_str(),
                "llm-inference"
                    | "tool-loop"
                    | "pytorch-inference"
                    | "puma-lib"
                    | "llamacpp-inference"
            )
        })
        .map(|node| node.id.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    for node_id in nodes_to_remove {
        updated_graph = workflow_service
            .workflow_graph_remove_node(WorkflowGraphRemoveNodeRequest {
                session_id: edit_session.session_id.clone(),
                node_id,
            })
            .await
            .map_err(|error| {
                PantographReasoningProbeError::Host(format!(
                    "workflow node removal failed: {error}"
                ))
            })?;
    }

    let edges_to_remove = updated_graph
        .graph
        .edges
        .iter()
        .map(|edge| edge.id.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    for edge_id in edges_to_remove {
        let _ = workflow_service
            .workflow_graph_remove_edge(WorkflowGraphRemoveEdgeRequest {
                session_id: edit_session.session_id.clone(),
                edge_id,
            })
            .await
            .map_err(|error| {
                PantographReasoningProbeError::Host(format!(
                    "workflow edge removal failed: {error}"
                ))
            })?;
    }

    let puma_node_id = "puma-lib-gestalt-reasoning".to_string();
    let llama_node_id = "llamacpp-inference-gestalt-reasoning".to_string();

    let puma_node = GraphNode {
        id: puma_node_id.clone(),
        node_type: "puma-lib".to_string(),
        position: Position { x: 180.0, y: 40.0 },
        data: json!({
            "definition": {
                "category": "input",
                "description": "Provides AI model file path",
                "execution_mode": "reactive",
                "inputs": [],
                "label": "Puma-Lib",
                "node_type": "puma-lib",
                "outputs": [
                    {"data_type":"string","id":"model_path","label":"Model Path","multiple":false,"required":false},
                    {"data_type":"string","id":"model_id","label":"Model ID","multiple":false,"required":false},
                    {"data_type":"string","id":"model_type","label":"Model Type","multiple":false,"required":false},
                    {"data_type":"string","id":"task_type_primary","label":"Task Type","multiple":false,"required":false},
                    {"data_type":"string","id":"backend_key","label":"Backend Key","multiple":false,"required":false},
                    {"data_type":"string","id":"recommended_backend","label":"Recommended Backend","multiple":false,"required":false},
                    {"data_type":"json","id":"platform_context","label":"Platform Context","multiple":false,"required":false},
                    {"data_type":"json","id":"selected_binding_ids","label":"Selected Bindings","multiple":false,"required":false},
                    {"data_type":"json","id":"dependency_bindings","label":"Dependency Bindings","multiple":false,"required":false},
                    {"data_type":"string","id":"dependency_requirements_id","label":"Dependency Requirements ID","multiple":false,"required":false},
                    {"data_type":"json","id":"inference_settings","label":"Inference Settings","multiple":false,"required":false},
                    {"data_type":"json","id":"dependency_requirements","label":"Dependency Requirements","multiple":false,"required":false}
                ]
            },
            "label": "Puma-Lib",
            "modelName": config.model_id,
            "modelPath": model_path,
            "selectionMode": "library",
            "model_id": "llm/qwen35moe/qwen3_5-35b-a3b-gguf",
            "model_type": "llm",
            "task_type_primary": "text-generation",
            "backend_key": "llamacpp",
            "recommended_backend": "llamacpp",
            "dependency_requirements_id": "llm/qwen35moe/qwen3_5-35b-a3b-gguf",
            "inference_settings": [
                {"key":"gpu_layers","label":"GPU Layers","param_type":"Integer","default":-1,"description":"Layers to offload to GPU (-1 = all)","constraints":{"min":-1,"max":null,"allowed_values":null}},
                {"key":"context_length","label":"Context Length","param_type":"Integer","default":8192,"description":"Maximum context window size in tokens","constraints":{"min":512,"max":131072,"allowed_values":null}},
                {"key":"temperature","label":"Temperature","param_type":"Number","default":0.7,"description":"Sampling temperature (higher = more creative)","constraints":{"min":0,"max":5,"allowed_values":null}},
                {"key":"top_p","label":"Top P","param_type":"Number","default":0.9,"description":"Nucleus sampling threshold","constraints":{"min":0,"max":1,"allowed_values":null}},
                {"key":"top_k","label":"Top K","param_type":"Integer","default":40,"description":"Top-K sampling (0 = disabled)","constraints":{"min":0,"max":1000,"allowed_values":null}},
                {"key":"repeat_penalty","label":"Repeat Penalty","param_type":"Number","default":1.1,"description":"Penalty for repeated tokens","constraints":{"min":0,"max":5,"allowed_values":null}},
                {"key":"seed","label":"Seed","param_type":"Integer","default":-1,"description":"Random seed (-1 = random)","constraints":{"min":-1,"max":null,"allowed_values":null}}
            ],
            "node_type": "puma-lib"
        }),
    };
    let _ = workflow_service
        .workflow_graph_add_node(WorkflowGraphAddNodeRequest {
            session_id: edit_session.session_id.clone(),
            node: puma_node,
        })
        .await
        .map_err(|error| {
            PantographReasoningProbeError::Host(format!("workflow puma-lib add failed: {error}"))
        })?;

    let llama_node = GraphNode {
        id: llama_node_id.clone(),
        node_type: "llamacpp-inference".to_string(),
        position: Position { x: 520.0, y: 210.0 },
        data: json!({
            "definition": {
                "category": "processing",
                "description": "Run inference via llama.cpp server (no model duplication)",
                "execution_mode": "stream",
                "inputs": [
                    {"data_type":"string","id":"model_path","label":"Model Path","multiple":false,"required":true},
                    {"data_type":"prompt","id":"prompt","label":"Prompt","multiple":false,"required":true},
                    {"data_type":"string","id":"system_prompt","label":"System Prompt","multiple":false,"required":false},
                    {"data_type":"number","id":"temperature","label":"Temperature","multiple":false,"required":false},
                    {"data_type":"number","id":"max_tokens","label":"Max Tokens","multiple":false,"required":false},
                    {"data_type":"tools","id":"tools","label":"Tools","multiple":true,"required":false},
                    {"data_type":"json","id":"inference_settings","label":"Inference Settings","multiple":false,"required":false}
                ],
                "label": "LlamaCpp Inference",
                "node_type": "llamacpp-inference",
                "outputs": [
                    {"data_type":"string","id":"response","label":"Response","multiple":false,"required":true},
                    {"data_type":"string","id":"model_path","label":"Model Path","multiple":false,"required":false},
                    {"data_type":"json","id":"model_ref","label":"Model Reference","multiple":false,"required":false},
                    {"data_type":"json","id":"tool_calls","label":"Tool Calls","multiple":false,"required":false},
                    {"data_type":"boolean","id":"has_tool_calls","label":"Has Tool Calls","multiple":false,"required":false},
                    {"data_type":"stream","id":"stream","label":"Stream","multiple":false,"required":false}
                ]
            },
            "label": "LlamaCpp Inference",
            "temperature": 0.7,
            "max_tokens": 512,
            "node_type": "llamacpp-inference"
        }),
    };
    let _ = workflow_service
        .workflow_graph_add_node(WorkflowGraphAddNodeRequest {
            session_id: edit_session.session_id.clone(),
            node: llama_node,
        })
        .await
        .map_err(|error| {
            PantographReasoningProbeError::Host(format!(
                "workflow llamacpp-inference add failed: {error}"
            ))
        })?;

    let required_edges = [
        GraphEdge {
            id: "prompt-to-llamacpp".to_string(),
            source: input_node.id.clone(),
            source_handle: if input_node.node_type == "linked-input" {
                "value".to_string()
            } else {
                "text".to_string()
            },
            target: llama_node_id.clone(),
            target_handle: "prompt".to_string(),
        },
        GraphEdge {
            id: "system-to-llamacpp".to_string(),
            source: system_node.id.clone(),
            source_handle: "text".to_string(),
            target: llama_node_id.clone(),
            target_handle: "system_prompt".to_string(),
        },
        GraphEdge {
            id: "puma-to-llamacpp-model".to_string(),
            source: puma_node_id.clone(),
            source_handle: "model_path".to_string(),
            target: llama_node_id.clone(),
            target_handle: "model_path".to_string(),
        },
        GraphEdge {
            id: "puma-to-llamacpp-settings".to_string(),
            source: puma_node_id.clone(),
            source_handle: "inference_settings".to_string(),
            target: llama_node_id.clone(),
            target_handle: "inference_settings".to_string(),
        },
        GraphEdge {
            id: "llamacpp-to-output".to_string(),
            source: llama_node_id.clone(),
            source_handle: "response".to_string(),
            target: text_output_node.id.clone(),
            target_handle: "text".to_string(),
        },
    ];
    for edge in required_edges {
        let _ = workflow_service
            .workflow_graph_add_edge(WorkflowGraphAddEdgeRequest {
                session_id: edit_session.session_id.clone(),
                edge,
            })
            .await
            .map_err(|error| {
                PantographReasoningProbeError::Host(format!("workflow edge add failed: {error}"))
            })?;
    }

    updated_graph = workflow_service
        .workflow_graph_get_edit_session_graph(
            pantograph_workflow_service::WorkflowGraphEditSessionGraphRequest {
                session_id: edit_session.session_id.clone(),
            },
        )
        .await
        .map_err(|error| {
            PantographReasoningProbeError::Host(format!(
                "workflow edit-session graph refresh failed: {error}"
            ))
        })?;

    let nodes_to_patch = updated_graph
        .graph
        .nodes
        .iter()
        .filter_map(|node| {
            let category = node
                .data
                .get("definition")
                .and_then(|value| value.get("category"))
                .and_then(|value| value.as_str())?;
            let missing_origin = node
                .data
                .get("definition")
                .and_then(|value| value.get("io_binding_origin"))
                .and_then(|value| value.as_str())
                .is_none();
            if !missing_origin {
                return None;
            }
            let origin = io_binding_origin_for_node(node.node_type.as_str(), category)?;
            Some((node.id.clone(), node.data.clone(), origin))
        })
        .collect::<Vec<_>>();

    for (node_id, mut node_data, origin) in nodes_to_patch {
        node_data["definition"]["io_binding_origin"] = json!(origin);
        if node_data
            .get("node_type")
            .and_then(|value| value.as_str())
            .is_none()
        {
            if let Some(node_type) = updated_graph
                .graph
                .nodes
                .iter()
                .find(|node| node.id == node_id)
                .map(|node| node.node_type.clone())
            {
                node_data["node_type"] = json!(node_type);
            }
        }
        updated_graph = workflow_service
            .workflow_graph_update_node_data(WorkflowGraphUpdateNodeDataRequest {
                session_id: edit_session.session_id.clone(),
                node_id,
                data: node_data,
            })
            .await
            .map_err(|error| {
                PantographReasoningProbeError::Host(format!(
                    "workflow io-binding origin update failed: {error}"
                ))
            })?;
    }

    let nodes_missing_type = updated_graph
        .graph
        .nodes
        .iter()
        .filter(|node| {
            node.data
                .get("node_type")
                .and_then(|value| value.as_str())
                .is_none()
        })
        .map(|node| (node.id.clone(), node.data.clone(), node.node_type.clone()))
        .collect::<Vec<_>>();

    for (node_id, mut node_data, node_type) in nodes_missing_type {
        node_data["node_type"] = json!(node_type);
        updated_graph = workflow_service
            .workflow_graph_update_node_data(WorkflowGraphUpdateNodeDataRequest {
                session_id: edit_session.session_id.clone(),
                node_id,
                data: node_data,
            })
            .await
            .map_err(|error| {
                PantographReasoningProbeError::Host(format!(
                    "workflow node_type update failed: {error}"
                ))
            })?;
    }

    let save = workflow_service
        .workflow_graph_save(
            &store,
            WorkflowGraphSaveRequest {
                name: config.workflow_id.clone(),
                graph: updated_graph.graph,
            },
        )
        .map_err(|error| {
            PantographReasoningProbeError::Host(format!("workflow graph save failed: {error}"))
        })?;

    workflow_service
        .workflow_graph_close_edit_session(WorkflowGraphEditSessionCloseRequest {
            session_id: edit_session.session_id,
        })
        .await
        .map_err(|error| {
            PantographReasoningProbeError::Host(format!(
                "workflow edit-session close failed: {error}"
            ))
        })?;

    Ok((save.path, llama_node_id, "llamacpp-inference".to_string()))
}

fn preview_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut preview = String::new();
    for ch in text.chars().take(max_chars) {
        preview.push(ch);
    }
    preview.push_str("...");
    preview
}

fn current_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as i64)
}

fn io_binding_origin_for_node(node_type: &str, category: &str) -> Option<&'static str> {
    match category {
        "input" | "output" => match node_type {
            "linked-input" | "puma-lib" | "model-provider" | "component-preview"
            | "point-cloud-output" => Some("integrated"),
            "audio-input" | "boolean-input" | "human-input" | "image-input"
            | "masked-text-input" | "number-input" | "selection-input" | "text-input"
            | "vector-input" | "audio-output" | "image-output" | "text-output"
            | "vector-output" => Some("client_session"),
            _ => None,
        },
        _ => None,
    }
}
