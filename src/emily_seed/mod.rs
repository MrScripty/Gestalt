use emily::api::EmilyApi;
use emily::error::EmilyError;
use emily::model::{
    CreateEpisodeRequest, DatabaseLocator, EarlEvaluationRequest, EpisodeTraceKind,
    IngestTextRequest, RecordOutcomeRequest, TextObjectKind, TraceLinkRequest,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io;
use std::path::PathBuf;
use thiserror::Error;

mod datasets;

pub use datasets::{
    SYNTHETIC_AGENT_ROUND_DATASET, SYNTHETIC_RISK_GATED_DATASET,
    SYNTHETIC_SEMANTIC_CONTEXT_DATASET, SYNTHETIC_TERMINAL_DATASET,
};

const BUILTIN_DATASET_LABELS: [&str; 4] = [
    SYNTHETIC_SEMANTIC_CONTEXT_DATASET,
    SYNTHETIC_TERMINAL_DATASET,
    SYNTHETIC_AGENT_ROUND_DATASET,
    SYNTHETIC_RISK_GATED_DATASET,
];

/// Host-side error returned by Gestalt Emily seeding helpers.
#[derive(Debug, Error)]
pub enum EmilySeedError {
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
}

/// Deterministic seed corpus consumed through Emily's public facade.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmilySeedCorpus {
    pub label: String,
    pub text_objects: Vec<SeedTextObject>,
    pub episodes: Vec<SeedEpisodeFixture>,
}

/// One deterministic text object fixture for Emily ingestion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeedTextObject {
    pub stream_id: String,
    pub source_kind: String,
    pub object_kind: TextObjectKind,
    pub sequence: u64,
    pub ts_unix_ms: i64,
    pub text: String,
    pub metadata: Value,
}

/// One deterministic episode fixture with linked trace, outcomes, and optional EARL.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeedEpisodeFixture {
    pub create: CreateEpisodeRequest,
    pub trace_links: Vec<SeedTraceLinkFixture>,
    pub outcome: Option<RecordOutcomeRequest>,
    pub earl_evaluations: Vec<EarlEvaluationRequest>,
}

/// One deterministic episode-to-text trace fixture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeedTraceLinkFixture {
    pub object_id: String,
    pub trace_kind: EpisodeTraceKind,
    pub linked_at_unix_ms: i64,
    pub metadata: Value,
}

/// Summary returned after one deterministic seed run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmilySeedReport {
    pub label: String,
    pub stream_ids: Vec<String>,
    pub episode_ids: Vec<String>,
    pub text_objects_seeded: usize,
    pub episodes_seeded: usize,
    pub trace_links_seeded: usize,
    pub outcomes_seeded: usize,
    pub earl_evaluations_seeded: usize,
}

impl SeedTextObject {
    /// Deterministic Emily object id implied by the current ingest contract.
    pub fn object_id(&self) -> String {
        format!("{}:{}", self.stream_id, self.sequence)
    }

    fn into_ingest_request(self) -> IngestTextRequest {
        IngestTextRequest {
            stream_id: self.stream_id,
            source_kind: self.source_kind,
            object_kind: self.object_kind,
            sequence: self.sequence,
            ts_unix_ms: self.ts_unix_ms,
            text: self.text,
            metadata: self.metadata,
        }
    }
}

/// Return the built-in dataset labels supported by the Gestalt seed tooling.
pub fn builtin_dataset_labels() -> &'static [&'static str] {
    &BUILTIN_DATASET_LABELS
}

/// Build one deterministic built-in corpus by label.
pub fn builtin_seed_corpus(label: &str) -> Option<EmilySeedCorpus> {
    datasets::builtin_seed_corpus(label)
}

/// Open one Emily database target, optionally reset it, then seed one built-in corpus.
pub async fn open_and_seed_builtin_corpus<A: EmilyApi + ?Sized>(
    api: &A,
    locator: DatabaseLocator,
    label: &str,
    reset: bool,
) -> Result<EmilySeedReport, EmilySeedError> {
    let Some(corpus) = builtin_seed_corpus(label) else {
        return Err(EmilySeedError::UnknownCorpus {
            label: label.to_string(),
        });
    };
    open_and_seed_corpus(api, locator, &corpus, reset).await
}

/// Seed one built-in corpus into an already-open Emily database.
pub async fn seed_builtin_corpus<A: EmilyApi + ?Sized>(
    api: &A,
    label: &str,
) -> Result<EmilySeedReport, EmilySeedError> {
    let Some(corpus) = builtin_seed_corpus(label) else {
        return Err(EmilySeedError::UnknownCorpus {
            label: label.to_string(),
        });
    };
    seed_corpus(api, &corpus)
        .await
        .map_err(EmilySeedError::from)
}

/// Open one Emily database target, optionally reset it, then seed the provided corpus.
pub async fn open_and_seed_corpus<A: EmilyApi + ?Sized>(
    api: &A,
    locator: DatabaseLocator,
    corpus: &EmilySeedCorpus,
    reset: bool,
) -> Result<EmilySeedReport, EmilySeedError> {
    if reset {
        reset_storage_path(&locator.storage_path)?;
    }
    api.open_db(locator).await?;
    seed_corpus(api, corpus).await.map_err(EmilySeedError::from)
}

/// Seed one already-open Emily database through the public facade.
pub async fn seed_corpus<A: EmilyApi + ?Sized>(
    api: &A,
    corpus: &EmilySeedCorpus,
) -> Result<EmilySeedReport, EmilyError> {
    for text_object in &corpus.text_objects {
        let _ = api
            .ingest_text(text_object.clone().into_ingest_request())
            .await?;
    }

    for episode in &corpus.episodes {
        let _ = api.create_episode(episode.create.clone()).await?;
    }

    for episode in &corpus.episodes {
        for trace_link in &episode.trace_links {
            let _ = api
                .link_text_to_episode(TraceLinkRequest {
                    episode_id: episode.create.episode_id.clone(),
                    object_id: trace_link.object_id.clone(),
                    trace_kind: trace_link.trace_kind,
                    linked_at_unix_ms: trace_link.linked_at_unix_ms,
                    metadata: trace_link.metadata.clone(),
                })
                .await?;
        }

        for evaluation in &episode.earl_evaluations {
            let _ = api.evaluate_episode_risk(evaluation.clone()).await?;
        }

        if let Some(outcome) = episode.outcome.as_ref() {
            let _ = api.record_outcome(outcome.clone()).await?;
        }
    }

    Ok(EmilySeedReport {
        label: corpus.label.clone(),
        stream_ids: distinct_stream_ids(corpus),
        episode_ids: corpus
            .episodes
            .iter()
            .map(|episode| episode.create.episode_id.clone())
            .collect(),
        text_objects_seeded: corpus.text_objects.len(),
        episodes_seeded: corpus.episodes.len(),
        trace_links_seeded: corpus
            .episodes
            .iter()
            .map(|episode| episode.trace_links.len())
            .sum(),
        outcomes_seeded: corpus
            .episodes
            .iter()
            .filter(|episode| episode.outcome.is_some())
            .count(),
        earl_evaluations_seeded: corpus
            .episodes
            .iter()
            .map(|episode| episode.earl_evaluations.len())
            .sum(),
    })
}

fn reset_storage_path(storage_path: &PathBuf) -> Result<(), EmilySeedError> {
    match std::fs::remove_dir_all(storage_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(EmilySeedError::ResetStorage {
            path: storage_path.clone(),
            source: error,
        }),
    }
}

fn distinct_stream_ids(corpus: &EmilySeedCorpus) -> Vec<String> {
    let mut stream_ids = corpus
        .text_objects
        .iter()
        .map(|object| object.stream_id.clone())
        .collect::<Vec<_>>();
    stream_ids.sort();
    stream_ids.dedup();
    stream_ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn builtin_corpora_use_supported_labels() {
        let labels = builtin_dataset_labels();
        for label in labels {
            let corpus = builtin_seed_corpus(label).expect("corpus should exist");
            assert_eq!(&corpus.label, label);
        }
    }

    #[test]
    fn seed_object_id_matches_emily_ingest_contract() {
        let object = SeedTextObject {
            stream_id: "seed:stream".to_string(),
            source_kind: "gestalt-seed".to_string(),
            object_kind: TextObjectKind::UserInput,
            sequence: 7,
            ts_unix_ms: 1,
            text: "hello".to_string(),
            metadata: json!({}),
        };
        assert_eq!(object.object_id(), "seed:stream:7");
    }
}
