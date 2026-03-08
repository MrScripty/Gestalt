use super::test_support::{MockStore, ingest_request, locator};
use super::*;
use crate::api::EmilyApi;
use crate::model::{
    ContextQuery, CreateEpisodeRequest, EpisodeTraceKind, IntegritySnapshot, MemoryState,
    OutcomeStatus, RecordOutcomeRequest, TextObjectKind, TraceLinkRequest,
};
use crate::store::EmilyStore;
use serde_json::json;
use std::sync::Arc;

fn episode_request() -> CreateEpisodeRequest {
    CreateEpisodeRequest {
        episode_id: "ep-ecgl".to_string(),
        stream_id: Some("stream-a".to_string()),
        source_kind: "terminal".to_string(),
        episode_kind: "command_round".to_string(),
        started_at_unix_ms: 1,
        intent: Some("score memory".to_string()),
        metadata: json!({"cwd": "/tmp"}),
    }
}

async fn create_linked_episode(runtime: &EmilyRuntime<MockStore>) {
    runtime
        .ingest_text(ingest_request(1))
        .await
        .expect("ingest text");
    runtime
        .create_episode(episode_request())
        .await
        .expect("create episode");
    runtime
        .link_text_to_episode(TraceLinkRequest {
            episode_id: "ep-ecgl".to_string(),
            object_id: "stream-a:1".to_string(),
            trace_kind: EpisodeTraceKind::Output,
            linked_at_unix_ms: 2,
            metadata: json!({"source": "terminal"}),
        })
        .await
        .expect("link text");
}

#[tokio::test]
async fn record_outcome_integrates_successful_objects_and_updates_snapshot() {
    let store = Arc::new(MockStore::default());
    let runtime = EmilyRuntime::new(store.clone());
    runtime.open_db(locator()).await.expect("open");
    create_linked_episode(&runtime).await;

    runtime
        .record_outcome(RecordOutcomeRequest {
            outcome_id: "out-success".to_string(),
            episode_id: "ep-ecgl".to_string(),
            status: OutcomeStatus::Succeeded,
            recorded_at_unix_ms: 3,
            summary: Some("success".to_string()),
            metadata: json!({"exit_code": 0}),
        })
        .await
        .expect("record outcome");

    let object = store
        .get_text_object("stream-a:1")
        .await
        .expect("get object")
        .expect("object exists");
    let snapshot = runtime
        .latest_integrity_snapshot()
        .await
        .expect("latest snapshot")
        .expect("snapshot exists");

    assert_eq!(object.memory_state, MemoryState::Integrated);
    assert!(object.integrated);
    assert!(object.learning_weight >= super::ecgl::TAU_INITIAL);
    assert_eq!(snapshot.integrated_count, 1);
}

#[tokio::test]
async fn record_outcome_quarantines_failed_objects() {
    let store = Arc::new(MockStore::default());
    let runtime = EmilyRuntime::new(store.clone());
    runtime.open_db(locator()).await.expect("open");
    create_linked_episode(&runtime).await;

    runtime
        .record_outcome(RecordOutcomeRequest {
            outcome_id: "out-failed".to_string(),
            episode_id: "ep-ecgl".to_string(),
            status: OutcomeStatus::Failed,
            recorded_at_unix_ms: 3,
            summary: Some("failed".to_string()),
            metadata: json!({"exit_code": 1}),
        })
        .await
        .expect("record outcome");

    let object = store
        .get_text_object("stream-a:1")
        .await
        .expect("get object")
        .expect("object exists");
    let snapshot = runtime
        .latest_integrity_snapshot()
        .await
        .expect("latest snapshot")
        .expect("snapshot exists");

    assert_eq!(object.memory_state, MemoryState::Quarantined);
    assert!(!object.integrated);
    assert!(object.quarantine_score > 0.0);
    assert_eq!(snapshot.quarantined_count, 1);
}

#[tokio::test]
async fn open_db_restores_tau_from_latest_snapshot() {
    let store = Arc::new(MockStore::default());
    store
        .upsert_integrity_snapshot(&IntegritySnapshot {
            id: "integrity:1".to_string(),
            ts_unix_ms: 1,
            ci_value: 0.70,
            tau: 0.74,
            integrated_count: 0,
            quarantined_count: 0,
            pending_count: 0,
            deferred_count: 0,
        })
        .await
        .expect("seed snapshot");
    let runtime = EmilyRuntime::new(store);
    runtime.open_db(locator()).await.expect("open");

    let snapshot = runtime
        .latest_integrity_snapshot()
        .await
        .expect("latest snapshot")
        .expect("snapshot exists");
    assert_eq!(snapshot.tau, 0.74);
}

#[tokio::test]
async fn query_context_skips_quarantined_objects() {
    let store = Arc::new(MockStore::default());
    store
        .upsert_text_object(&TextObject {
            id: "stream-a:1".to_string(),
            stream_id: "stream-a".to_string(),
            source_kind: "terminal".to_string(),
            object_kind: TextObjectKind::SystemOutput,
            sequence: 1,
            ts_unix_ms: 1,
            text: "danger memory".to_string(),
            metadata: json!({}),
            epsilon: None,
            confidence: 1.0,
            outcome_factor: 0.0,
            novelty_factor: 0.5,
            stability_factor: 0.0,
            learning_weight: 0.2,
            gate_score: Some(0.1),
            memory_state: MemoryState::Quarantined,
            integrated: false,
            quarantine_score: 0.9,
        })
        .await
        .expect("seed quarantined object");
    let runtime = EmilyRuntime::new(store);
    runtime.open_db(locator()).await.expect("open");

    let packet = runtime
        .query_context(ContextQuery {
            stream_id: Some("stream-a".to_string()),
            query_text: "danger".to_string(),
            top_k: 1,
            neighbor_depth: 0,
        })
        .await
        .expect("query context");
    assert!(packet.items.is_empty());
}
