use crate::emily_inspect::{EmilyInspectError, EpisodeInspectionSnapshot, inspect_episode};
use crate::emily_seed::{EmilySeedError, builtin_seed_corpus, seed_builtin_corpus};
use emily::api::EmilyApi;
use emily::error::EmilyError;
use emily::model::{ContextQuery, CreateEpisodeRequest, DatabaseLocator};
use emily_membrane::contracts::{
    ContextFragment, LocalExecutionPersistence, MembraneTaskRequest, PolicyExecutionPersistence,
    RoutingPolicyRequest, RoutingSensitivity,
};
use emily_membrane::runtime::{MembraneRuntime, MembraneRuntimeError};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use uuid::Uuid;

const MEMBRANE_DEV_TOGGLE_ENV: &str = "GESTALT_ENABLE_MEMBRANE_DEV";

#[derive(Debug, Error)]
pub enum EmilyMembraneDevError {
    #[error("Emily membrane dev flow is disabled; set {env}=1 to enable it")]
    Disabled { env: &'static str },
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
    Inspect(#[from] EmilyInspectError),
    #[error(transparent)]
    Seed(#[from] EmilySeedError),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmilyMembraneDevRequest {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmilyMembraneDevSnapshot {
    pub dataset: String,
    pub episode_id: String,
    pub task_text: String,
    pub query_text: String,
    pub context_fragments: Vec<ContextFragment>,
    pub execution: emily_membrane::contracts::PolicySelectedExecution,
    pub episode_snapshot: EpisodeInspectionSnapshot,
}

pub fn assert_membrane_dev_enabled() -> Result<(), EmilyMembraneDevError> {
    match std::env::var(MEMBRANE_DEV_TOGGLE_ENV) {
        Ok(value) if value == "1" => Ok(()),
        _ => Err(EmilyMembraneDevError::Disabled {
            env: MEMBRANE_DEV_TOGGLE_ENV,
        }),
    }
}

pub fn membrane_dev_toggle_env() -> &'static str {
    MEMBRANE_DEV_TOGGLE_ENV
}

pub async fn run_membrane_dev_scenario<A: EmilyApi + ?Sized>(
    api: Arc<A>,
    request: EmilyMembraneDevRequest,
) -> Result<EmilyMembraneDevSnapshot, EmilyMembraneDevError> {
    let Some(corpus) = builtin_seed_corpus(&request.dataset) else {
        return Err(EmilyMembraneDevError::UnknownCorpus {
            label: request.dataset,
        });
    };

    if request.reset {
        match std::fs::remove_dir_all(&request.storage_path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(EmilyMembraneDevError::ResetStorage {
                    path: request.storage_path.clone(),
                    source: error,
                });
            }
        }
    }

    api.open_db(DatabaseLocator {
        storage_path: request.storage_path.clone(),
        namespace: request.namespace.clone(),
        database: request.database.clone(),
    })
    .await?;

    if request.reseed {
        let _ = seed_builtin_corpus(api.as_ref(), &request.dataset).await?;
    }

    let stream_id = corpus
        .text_objects
        .first()
        .map(|object| object.stream_id.clone())
        .ok_or_else(|| EmilyMembraneDevError::UnknownCorpus {
            label: request.dataset.clone(),
        })?;
    let context_packet = api
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

    let episode_id = format!("membrane-dev:{}", Uuid::new_v4());
    let now = current_unix_ms();
    let _ = api
        .create_episode(CreateEpisodeRequest {
            episode_id: episode_id.clone(),
            stream_id: Some(stream_id),
            source_kind: "gestalt-membrane-dev".to_string(),
            episode_kind: "membrane_dev_local_only".to_string(),
            started_at_unix_ms: now,
            intent: Some(request.task_text.clone()),
            metadata: json!({
                "dataset": request.dataset,
                "mode": "dev_toggle_local_only",
            }),
        })
        .await?;

    let task_id = format!("membrane-task:{}", Uuid::new_v4());
    let runtime = MembraneRuntime::new(api.clone());
    let execution = runtime
        .execute_with_policy_and_record(
            MembraneTaskRequest {
                task_id: task_id.clone(),
                episode_id: episode_id.clone(),
                task_text: request.task_text.clone(),
                context_fragments: context_fragments.clone(),
                allow_remote: false,
            },
            RoutingPolicyRequest {
                task_id: task_id.clone(),
                episode_id: episode_id.clone(),
                allow_remote: false,
                sensitivity: RoutingSensitivity::Normal,
                preference: emily_membrane::contracts::RemoteRoutingPreference {
                    provider_id: None,
                    profile_id: None,
                    required_capability_tags: Vec::new(),
                },
            },
            PolicyExecutionPersistence {
                local: Some(LocalExecutionPersistence {
                    route_decision_id: format!("{episode_id}:route"),
                    route_decided_at_unix_ms: now + 1,
                    validation_id: format!("{episode_id}:validation"),
                    validated_at_unix_ms: now + 2,
                }),
                remote: None,
            },
        )
        .await?;
    let episode_snapshot = inspect_episode(api.as_ref(), &episode_id).await?;

    let _ = api.close_db().await;

    Ok(EmilyMembraneDevSnapshot {
        dataset: request.dataset,
        episode_id,
        task_text: request.task_text,
        query_text: request.query_text,
        context_fragments,
        execution,
        episode_snapshot,
    })
}

fn current_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as i64)
}
