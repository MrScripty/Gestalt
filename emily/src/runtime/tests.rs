use super::*;
use crate::model::{TextEdge, TextObjectKind, VectorizationConfig, VectorizationJobState};
use crate::store::EmilyStore;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};
use tokio::time::{Duration, sleep};

#[derive(Default)]
struct MockStore {
    objects: Mutex<Vec<TextObject>>,
    edges: Mutex<Vec<TextEdge>>,
    vectors: Mutex<Vec<TextVector>>,
    config: Mutex<Option<VectorizationConfig>>,
    insert_started: Option<Arc<Notify>>,
    release_insert: Option<Arc<Notify>>,
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

struct FixedEmbeddingProvider {
    vectors_by_text: std::collections::HashMap<String, Vec<f32>>,
    default_vector: Vec<f32>,
    shutdown_calls: Mutex<u64>,
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

fn locator() -> DatabaseLocator {
    DatabaseLocator {
        storage_path: std::env::temp_dir().join("emily-runtime-tests"),
        namespace: "ns".to_string(),
        database: "db".to_string(),
    }
}

fn ingest_request(sequence: u64) -> IngestTextRequest {
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

#[tokio::test]
async fn ingest_text_persists_vector_record_when_enabled_and_embedding_is_1024() {
    let store = Arc::new(MockStore::default());
    let provider = Arc::new(FixedEmbeddingProvider {
        vectors_by_text: std::collections::HashMap::new(),
        default_vector: vec![0.25; 1024],
        shutdown_calls: Mutex::new(0),
    });
    let runtime = EmilyRuntime::with_embedding_provider(store.clone(), Some(provider));
    runtime.open_db(locator()).await.expect("open");
    runtime
        .update_vectorization_config(VectorizationConfigPatch {
            enabled: Some(true),
            ..VectorizationConfigPatch::default()
        })
        .await
        .expect("enable vectorization");

    runtime
        .ingest_text(ingest_request(1))
        .await
        .expect("ingest should succeed");

    let vectors = store.vectors.lock().await;
    assert_eq!(vectors.len(), 1);
    assert_eq!(vectors[0].dimensions, 1024);
    assert_eq!(vectors[0].profile_id, "qwen3-0.6b");
}

#[tokio::test]
async fn ingest_text_skips_embedding_when_vectorization_disabled() {
    let store = Arc::new(MockStore::default());
    let provider = Arc::new(FixedEmbeddingProvider {
        vectors_by_text: std::collections::HashMap::new(),
        default_vector: vec![0.25; 1024],
        shutdown_calls: Mutex::new(0),
    });
    let runtime = EmilyRuntime::with_embedding_provider(store.clone(), Some(provider));
    runtime.open_db(locator()).await.expect("open");

    runtime
        .ingest_text(ingest_request(1))
        .await
        .expect("ingest should succeed");

    let vectors = store.vectors.lock().await;
    assert!(vectors.is_empty());
}

#[tokio::test]
async fn ingest_text_starts_objects_as_unintegrated() {
    let store = Arc::new(MockStore::default());
    let runtime = EmilyRuntime::new(store);
    runtime.open_db(locator()).await.expect("open");

    let object = runtime
        .ingest_text(ingest_request(1))
        .await
        .expect("ingest should succeed");

    assert!(!object.integrated);
}

#[tokio::test]
async fn health_reports_in_flight_ingest_operations() {
    let insert_started = Arc::new(Notify::new());
    let release_insert = Arc::new(Notify::new());
    let store = Arc::new(MockStore {
        insert_started: Some(insert_started.clone()),
        release_insert: Some(release_insert.clone()),
        ..MockStore::default()
    });
    let runtime = Arc::new(EmilyRuntime::new(store));
    runtime.open_db(locator()).await.expect("open");

    let ingest_runtime = runtime.clone();
    let ingest_task =
        tokio::spawn(async move { ingest_runtime.ingest_text(ingest_request(1)).await });

    insert_started.notified().await;

    let during_ingest = runtime.health().await.expect("health during ingest");
    assert_eq!(during_ingest.queued_ingest_events, 1);

    release_insert.notify_one();
    ingest_task
        .await
        .expect("join ingest task")
        .expect("ingest should succeed");

    let after_ingest = runtime.health().await.expect("health after ingest");
    assert_eq!(after_ingest.queued_ingest_events, 0);
}

#[tokio::test]
async fn backfill_job_vectors_missing_rows() {
    let store = Arc::new(MockStore::default());
    store
        .insert_text_object(&EmilyRuntime::<MockStore>::build_text_object(
            ingest_request(1),
        ))
        .await
        .expect("insert 1");
    store
        .insert_text_object(&EmilyRuntime::<MockStore>::build_text_object(
            ingest_request(2),
        ))
        .await
        .expect("insert 2");

    let provider = Arc::new(FixedEmbeddingProvider {
        vectors_by_text: std::collections::HashMap::new(),
        default_vector: vec![0.4; 1024],
        shutdown_calls: Mutex::new(0),
    });
    let runtime = EmilyRuntime::with_embedding_provider(store.clone(), Some(provider));
    runtime.open_db(locator()).await.expect("open");
    runtime
        .update_vectorization_config(VectorizationConfigPatch {
            enabled: Some(true),
            ..VectorizationConfigPatch::default()
        })
        .await
        .expect("enable");

    let job = runtime
        .start_backfill(VectorizationRunRequest { stream_id: None })
        .await
        .expect("start backfill");

    for _ in 0..50 {
        let status = runtime.vectorization_status().await.expect("status");
        if status
            .last_job
            .as_ref()
            .is_some_and(|last| last.job_id == job.job_id)
        {
            break;
        }
        sleep(Duration::from_millis(10)).await;
    }

    let status = runtime.vectorization_status().await.expect("status");
    let last = status.last_job.expect("last job");
    assert_eq!(last.job_id, job.job_id);
    assert_eq!(last.state, VectorizationJobState::Completed);
    assert_eq!(last.vectorized, 2);
}

#[tokio::test]
async fn close_db_invokes_provider_shutdown() {
    let store = Arc::new(MockStore::default());
    let provider = Arc::new(FixedEmbeddingProvider {
        vectors_by_text: std::collections::HashMap::new(),
        default_vector: vec![0.25; 1024],
        shutdown_calls: Mutex::new(0),
    });
    let runtime = EmilyRuntime::with_embedding_provider(store, Some(provider.clone()));
    runtime.open_db(locator()).await.expect("open");
    runtime.close_db().await.expect("close");
    assert_eq!(*provider.shutdown_calls.lock().await, 1);
}

#[tokio::test]
async fn query_context_prefers_semantic_hits_and_expands_neighbors() {
    let store = Arc::new(MockStore::default());
    let mut vectors_by_text = std::collections::HashMap::new();
    vectors_by_text.insert("alpha memory".to_string(), vec![1.0, 0.0, 0.0]);
    vectors_by_text.insert("beta memory".to_string(), vec![0.9, 0.1, 0.0]);
    vectors_by_text.insert("gamma memory".to_string(), vec![0.0, 1.0, 0.0]);
    vectors_by_text.insert("alpha question".to_string(), vec![1.0, 0.0, 0.0]);
    let provider = Arc::new(FixedEmbeddingProvider {
        vectors_by_text,
        default_vector: vec![0.0, 0.0, 1.0],
        shutdown_calls: Mutex::new(0),
    });
    let runtime = EmilyRuntime::with_embedding_provider(store.clone(), Some(provider));
    runtime.open_db(locator()).await.expect("open");
    runtime
        .update_vectorization_config(VectorizationConfigPatch {
            enabled: Some(true),
            expected_dimensions: Some(3),
            ..VectorizationConfigPatch::default()
        })
        .await
        .expect("enable vectorization");

    runtime
        .ingest_text(IngestTextRequest {
            text: "alpha memory".to_string(),
            ..ingest_request(1)
        })
        .await
        .expect("ingest alpha");
    runtime
        .ingest_text(IngestTextRequest {
            text: "beta memory".to_string(),
            ..ingest_request(2)
        })
        .await
        .expect("ingest beta");
    runtime
        .ingest_text(IngestTextRequest {
            text: "gamma memory".to_string(),
            ..ingest_request(3)
        })
        .await
        .expect("ingest gamma");

    runtime
        .update_vectorization_config(VectorizationConfigPatch {
            enabled: Some(false),
            ..VectorizationConfigPatch::default()
        })
        .await
        .expect("disable vectorization for lexical query");

    let packet = runtime
        .query_context(ContextQuery {
            stream_id: Some("stream-a".to_string()),
            query_text: "alpha".to_string(),
            top_k: 2,
            neighbor_depth: 1,
        })
        .await
        .expect("query context");

    assert_eq!(packet.items.len(), 2);
    assert_eq!(packet.items[0].object.text, "alpha memory");
    assert_eq!(packet.items[1].object.text, "beta memory");
    assert_eq!(packet.items[0].provenance, vec!["stream-a:1".to_string()]);
    assert!(
        packet.items[1].provenance.len() >= 2,
        "neighbor expansion should include provenance path"
    );
}
