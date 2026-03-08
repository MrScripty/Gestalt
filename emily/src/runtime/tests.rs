use super::test_support::{FixedEmbeddingProvider, MockStore, ingest_request, locator};
use super::*;
use crate::model::VectorizationJobState;
use crate::store::EmilyStore;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};
use tokio::time::{Duration, sleep};

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
