use super::*;
use crate::model::{
    AuditRecord, EpisodeRecord, EpisodeTraceLink, OutcomeRecord, TextEdge, TextObjectKind,
    VectorizationConfig,
};
use crate::store::EmilyStore;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};

#[derive(Default)]
pub(super) struct MockStore {
    pub(super) objects: Mutex<Vec<TextObject>>,
    pub(super) edges: Mutex<Vec<TextEdge>>,
    pub(super) vectors: Mutex<Vec<TextVector>>,
    pub(super) episodes: Mutex<Vec<EpisodeRecord>>,
    pub(super) trace_links: Mutex<Vec<EpisodeTraceLink>>,
    pub(super) outcomes: Mutex<Vec<OutcomeRecord>>,
    pub(super) audits: Mutex<Vec<AuditRecord>>,
    pub(super) config: Mutex<Option<VectorizationConfig>>,
    pub(super) insert_started: Option<Arc<Notify>>,
    pub(super) release_insert: Option<Arc<Notify>>,
}

#[async_trait]
impl EmilyStore for MockStore {
    async fn open(&self, _locator: &DatabaseLocator) -> Result<(), EmilyError> {
        Ok(())
    }

    async fn close(&self) -> Result<(), EmilyError> {
        Ok(())
    }

    async fn insert_text_object(&self, object: &TextObject) -> Result<(), EmilyError> {
        if let Some(insert_started) = self.insert_started.as_ref() {
            insert_started.notify_one();
        }
        if let Some(release_insert) = self.release_insert.as_ref() {
            release_insert.notified().await;
        }
        self.objects.lock().await.push(object.clone());
        Ok(())
    }

    async fn get_text_object(&self, object_id: &str) -> Result<Option<TextObject>, EmilyError> {
        let objects = self.objects.lock().await;
        Ok(objects.iter().find(|item| item.id == object_id).cloned())
    }

    async fn upsert_text_edge(&self, edge: &TextEdge) -> Result<(), EmilyError> {
        let mut edges = self.edges.lock().await;
        if let Some(index) = edges.iter().position(|item| item.id == edge.id) {
            edges[index] = edge.clone();
        } else {
            edges.push(edge.clone());
        }
        Ok(())
    }

    async fn upsert_text_vector(&self, vector: &TextVector) -> Result<(), EmilyError> {
        let mut vectors = self.vectors.lock().await;
        if let Some(index) = vectors.iter().position(|item| item.id == vector.id) {
            vectors[index] = vector.clone();
        } else {
            vectors.push(vector.clone());
        }
        Ok(())
    }

    async fn get_text_vector(&self, object_id: &str) -> Result<Option<TextVector>, EmilyError> {
        let vectors = self.vectors.lock().await;
        Ok(vectors
            .iter()
            .find(|item| item.object_id == object_id)
            .cloned())
    }

    async fn list_text_vectors(
        &self,
        stream_id: Option<&str>,
    ) -> Result<Vec<TextVector>, EmilyError> {
        let mut vectors = self.vectors.lock().await.clone();
        if let Some(stream_id) = stream_id {
            vectors.retain(|item| item.stream_id == stream_id);
        }
        vectors.sort_by(|left, right| left.sequence.cmp(&right.sequence));
        Ok(vectors)
    }

    async fn list_text_objects(
        &self,
        stream_id: Option<&str>,
    ) -> Result<Vec<TextObject>, EmilyError> {
        let mut objects = self.objects.lock().await.clone();
        if let Some(stream_id) = stream_id {
            objects.retain(|item| item.stream_id == stream_id);
        }
        objects.sort_by(|left, right| left.sequence.cmp(&right.sequence));
        Ok(objects)
    }

    async fn list_text_edges(
        &self,
        object_ids: &[String],
        max_depth: u8,
    ) -> Result<Vec<TextEdge>, EmilyError> {
        if object_ids.is_empty() || max_depth == 0 {
            return Ok(Vec::new());
        }

        let edges = self.edges.lock().await.clone();
        let mut frontier = object_ids
            .iter()
            .cloned()
            .map(|id| (id, 0_u8))
            .collect::<std::collections::VecDeque<_>>();
        let mut visited_nodes = object_ids
            .iter()
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        let mut seen_edges = std::collections::HashSet::<String>::new();
        let mut collected = Vec::new();

        while let Some((node_id, depth)) = frontier.pop_front() {
            if depth >= max_depth {
                continue;
            }
            for edge in &edges {
                let next_node = if edge.from_id == node_id {
                    Some(edge.to_id.clone())
                } else if edge.to_id == node_id {
                    Some(edge.from_id.clone())
                } else {
                    None
                };

                let Some(next_node) = next_node else {
                    continue;
                };

                if seen_edges.insert(edge.id.clone()) {
                    collected.push(edge.clone());
                }
                if visited_nodes.insert(next_node.clone()) {
                    frontier.push_back((next_node, depth.saturating_add(1)));
                }
            }
        }

        Ok(collected)
    }

    async fn get_vectorization_config(&self) -> Result<Option<VectorizationConfig>, EmilyError> {
        Ok(self.config.lock().await.clone())
    }

    async fn upsert_vectorization_config(
        &self,
        config: &VectorizationConfig,
    ) -> Result<(), EmilyError> {
        *self.config.lock().await = Some(config.clone());
        Ok(())
    }

    async fn upsert_episode(&self, episode: &EpisodeRecord) -> Result<(), EmilyError> {
        let mut episodes = self.episodes.lock().await;
        if let Some(index) = episodes.iter().position(|item| item.id == episode.id) {
            episodes[index] = episode.clone();
        } else {
            episodes.push(episode.clone());
        }
        Ok(())
    }

    async fn get_episode(&self, episode_id: &str) -> Result<Option<EpisodeRecord>, EmilyError> {
        let episodes = self.episodes.lock().await;
        Ok(episodes.iter().find(|item| item.id == episode_id).cloned())
    }

    async fn upsert_episode_trace_link(&self, link: &EpisodeTraceLink) -> Result<(), EmilyError> {
        let mut links = self.trace_links.lock().await;
        if let Some(index) = links.iter().position(|item| item.id == link.id) {
            links[index] = link.clone();
        } else {
            links.push(link.clone());
        }
        Ok(())
    }

    async fn get_episode_trace_link(
        &self,
        link_id: &str,
    ) -> Result<Option<EpisodeTraceLink>, EmilyError> {
        let links = self.trace_links.lock().await;
        Ok(links.iter().find(|item| item.id == link_id).cloned())
    }

    async fn list_episode_trace_links(
        &self,
        episode_id: &str,
    ) -> Result<Vec<EpisodeTraceLink>, EmilyError> {
        let mut links = self.trace_links.lock().await.clone();
        links.retain(|item| item.episode_id == episode_id);
        links.sort_by(|left, right| left.linked_at_unix_ms.cmp(&right.linked_at_unix_ms));
        Ok(links)
    }

    async fn upsert_outcome(&self, outcome: &OutcomeRecord) -> Result<(), EmilyError> {
        let mut outcomes = self.outcomes.lock().await;
        if let Some(index) = outcomes.iter().position(|item| item.id == outcome.id) {
            outcomes[index] = outcome.clone();
        } else {
            outcomes.push(outcome.clone());
        }
        Ok(())
    }

    async fn get_outcome(&self, outcome_id: &str) -> Result<Option<OutcomeRecord>, EmilyError> {
        let outcomes = self.outcomes.lock().await;
        Ok(outcomes.iter().find(|item| item.id == outcome_id).cloned())
    }

    async fn list_outcomes(&self, episode_id: &str) -> Result<Vec<OutcomeRecord>, EmilyError> {
        let mut outcomes = self.outcomes.lock().await.clone();
        outcomes.retain(|item| item.episode_id == episode_id);
        outcomes.sort_by(|left, right| left.recorded_at_unix_ms.cmp(&right.recorded_at_unix_ms));
        Ok(outcomes)
    }

    async fn upsert_audit_record(&self, audit: &AuditRecord) -> Result<(), EmilyError> {
        let mut audits = self.audits.lock().await;
        if let Some(index) = audits.iter().position(|item| item.id == audit.id) {
            audits[index] = audit.clone();
        } else {
            audits.push(audit.clone());
        }
        Ok(())
    }

    async fn get_audit_record(&self, audit_id: &str) -> Result<Option<AuditRecord>, EmilyError> {
        let audits = self.audits.lock().await;
        Ok(audits.iter().find(|item| item.id == audit_id).cloned())
    }

    async fn list_audit_records(&self, episode_id: &str) -> Result<Vec<AuditRecord>, EmilyError> {
        let mut audits = self.audits.lock().await.clone();
        audits.retain(|item| item.episode_id == episode_id);
        audits.sort_by(|left, right| left.ts_unix_ms.cmp(&right.ts_unix_ms));
        Ok(audits)
    }

    async fn query_context(&self, _query: &ContextQuery) -> Result<ContextPacket, EmilyError> {
        Ok(ContextPacket { items: Vec::new() })
    }

    async fn page_history_before(
        &self,
        _request: &HistoryPageRequest,
    ) -> Result<HistoryPage, EmilyError> {
        Ok(HistoryPage {
            items: Vec::new(),
            next_before_sequence: None,
        })
    }
}

pub(super) struct FixedEmbeddingProvider {
    pub(super) vectors_by_text: std::collections::HashMap<String, Vec<f32>>,
    pub(super) default_vector: Vec<f32>,
    pub(super) shutdown_calls: Mutex<u64>,
}

#[async_trait]
impl EmbeddingProvider for FixedEmbeddingProvider {
    async fn embed_text(&self, text: &str) -> Result<Vec<f32>, EmilyError> {
        Ok(self
            .vectors_by_text
            .get(text)
            .cloned()
            .unwrap_or_else(|| self.default_vector.clone()))
    }

    async fn shutdown(&self) -> Result<(), EmilyError> {
        let mut calls = self.shutdown_calls.lock().await;
        *calls += 1;
        Ok(())
    }
}

pub(super) fn locator() -> DatabaseLocator {
    DatabaseLocator {
        storage_path: std::env::temp_dir().join("emily-runtime-tests"),
        namespace: "ns".to_string(),
        database: "db".to_string(),
    }
}

pub(super) fn ingest_request(sequence: u64) -> IngestTextRequest {
    IngestTextRequest {
        stream_id: "stream-a".to_string(),
        source_kind: "terminal".to_string(),
        object_kind: TextObjectKind::SystemOutput,
        sequence,
        ts_unix_ms: sequence as i64,
        text: format!("line {sequence}"),
        metadata: json!({"cwd": "/tmp"}),
    }
}
