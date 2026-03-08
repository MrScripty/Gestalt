use emily::api::EmilyApi;
use emily::error::EmilyError;
use emily::model::{
    AuditRecord, ContextPacket, ContextQuery, DatabaseLocator, EarlEvaluationRecord, EpisodeRecord,
    HistoryPage, HistoryPageRequest, RemoteEpisodeRecord, RoutingDecision, ValidationOutcome,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::emily_seed::{EmilySeedError, builtin_seed_corpus, seed_builtin_corpus};

/// Host-side error returned by Gestalt Emily inspection helpers.
#[derive(Debug, Error)]
pub enum EmilyInspectError {
    #[error("unknown Emily seed corpus '{label}'")]
    UnknownCorpus { label: String },
    #[error(transparent)]
    Emily(#[from] EmilyError),
    #[error(transparent)]
    Seed(#[from] EmilySeedError),
}

/// Deterministic inspection result for one seed corpus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmilyInspectionSnapshot {
    pub label: String,
    pub streams: Vec<StreamInspectionSnapshot>,
    pub episodes: Vec<EpisodeInspectionSnapshot>,
}

/// Host-facing inspection result for one Emily stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamInspectionSnapshot {
    pub stream_id: String,
    pub history: HistoryPage,
    pub context_query: Option<String>,
    pub context: Option<ContextPacket>,
}

/// Host-facing inspection result for one Emily episode and related sovereign state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeInspectionSnapshot {
    pub episode_id: String,
    pub episode: Option<EpisodeRecord>,
    pub latest_earl: Option<EarlEvaluationRecord>,
    pub routing_decisions: Vec<RoutingDecision>,
    pub remote_episodes: Vec<RemoteEpisodeRecord>,
    pub validation_outcomes: Vec<ValidationOutcome>,
    pub sovereign_audits: Vec<AuditRecord>,
}

/// Open one Emily database, optionally reseed built-in datasets, then inspect one built-in corpus.
pub async fn open_and_inspect_seeded_corpus<A: EmilyApi + ?Sized>(
    api: &A,
    locator: DatabaseLocator,
    label: &str,
    history_limit: usize,
    context_query: Option<&str>,
    context_top_k: usize,
    reseed_labels: &[String],
) -> Result<EmilyInspectionSnapshot, EmilyInspectError> {
    api.open_db(locator).await?;
    for dataset_label in reseed_labels {
        let _ = seed_builtin_corpus(api, dataset_label).await?;
    }
    inspect_seeded_corpus(api, label, history_limit, context_query, context_top_k).await
}

/// Inspect one built-in corpus through Emily's public read APIs.
pub async fn inspect_seeded_corpus<A: EmilyApi + ?Sized>(
    api: &A,
    label: &str,
    history_limit: usize,
    context_query: Option<&str>,
    context_top_k: usize,
) -> Result<EmilyInspectionSnapshot, EmilyInspectError> {
    let Some(corpus) = builtin_seed_corpus(label) else {
        return Err(EmilyInspectError::UnknownCorpus {
            label: label.to_string(),
        });
    };

    let mut stream_ids = corpus
        .text_objects
        .iter()
        .map(|object| object.stream_id.clone())
        .collect::<Vec<_>>();
    stream_ids.sort();
    stream_ids.dedup();

    let mut streams = Vec::with_capacity(stream_ids.len());
    for stream_id in stream_ids {
        streams.push(
            inspect_stream(api, &stream_id, history_limit, context_query, context_top_k).await?,
        );
    }

    let mut episode_ids = corpus
        .episodes
        .iter()
        .map(|episode| episode.create.episode_id.clone())
        .collect::<Vec<_>>();
    episode_ids.sort();

    let mut episodes = Vec::with_capacity(episode_ids.len());
    for episode_id in episode_ids {
        episodes.push(inspect_episode(api, &episode_id).await?);
    }

    Ok(EmilyInspectionSnapshot {
        label: label.to_string(),
        streams,
        episodes,
    })
}

/// Inspect one stream through Emily history and optional context-query APIs.
pub async fn inspect_stream<A: EmilyApi + ?Sized>(
    api: &A,
    stream_id: &str,
    history_limit: usize,
    context_query: Option<&str>,
    context_top_k: usize,
) -> Result<StreamInspectionSnapshot, EmilyInspectError> {
    let history = api
        .page_history_before(HistoryPageRequest {
            stream_id: stream_id.to_string(),
            before_sequence: None,
            limit: history_limit,
        })
        .await?;

    let context = match context_query {
        Some(query_text) => Some(
            api.query_context(ContextQuery {
                stream_id: Some(stream_id.to_string()),
                query_text: query_text.to_string(),
                top_k: context_top_k,
                neighbor_depth: 1,
            })
            .await?,
        ),
        None => None,
    };

    Ok(StreamInspectionSnapshot {
        stream_id: stream_id.to_string(),
        history,
        context_query: context_query.map(ToString::to_string),
        context,
    })
}

/// Inspect one episode and its related sovereign records through Emily's public APIs.
pub async fn inspect_episode<A: EmilyApi + ?Sized>(
    api: &A,
    episode_id: &str,
) -> Result<EpisodeInspectionSnapshot, EmilyInspectError> {
    let episode = api.episode(episode_id).await?;
    let latest_earl = api.latest_earl_evaluation_for_episode(episode_id).await?;

    let mut routing_decisions = api.routing_decisions_for_episode(episode_id).await?;
    routing_decisions.sort_by(|left, right| {
        left.decided_at_unix_ms
            .cmp(&right.decided_at_unix_ms)
            .then_with(|| left.decision_id.cmp(&right.decision_id))
    });

    let mut remote_episodes = api.remote_episodes_for_episode(episode_id).await?;
    remote_episodes.sort_by(|left, right| {
        left.dispatched_at_unix_ms
            .cmp(&right.dispatched_at_unix_ms)
            .then_with(|| left.id.cmp(&right.id))
    });

    let mut validation_outcomes = api.validation_outcomes_for_episode(episode_id).await?;
    validation_outcomes.sort_by(|left, right| {
        left.validated_at_unix_ms
            .cmp(&right.validated_at_unix_ms)
            .then_with(|| left.validation_id.cmp(&right.validation_id))
    });

    let mut sovereign_audits = api.sovereign_audit_records_for_episode(episode_id).await?;
    sovereign_audits.sort_by(|left, right| {
        left.ts_unix_ms
            .cmp(&right.ts_unix_ms)
            .then_with(|| left.id.cmp(&right.id))
    });

    Ok(EpisodeInspectionSnapshot {
        episode_id: episode_id.to_string(),
        episode,
        latest_earl,
        routing_decisions,
        remote_episodes,
        validation_outcomes,
        sovereign_audits,
    })
}
