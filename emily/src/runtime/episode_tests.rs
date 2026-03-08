use super::EmilyRuntime;
use crate::api::EmilyApi;
use crate::model::{
    AppendAuditRecordRequest, AuditRecordKind, CreateEpisodeRequest, EpisodeRecord, EpisodeState,
    EpisodeTraceKind, IngestTextRequest, OutcomeStatus, RecordOutcomeRequest, TextObjectKind,
    TraceLinkRequest,
};
use crate::store::EmilyStore;
use crate::store::surreal::SurrealEmilyStore;
use serde_json::json;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

fn locator() -> crate::model::DatabaseLocator {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch");
    let storage_path = std::env::temp_dir().join(format!(
        "emily-runtime-episode-test-{}-{}",
        std::process::id(),
        now.as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&storage_path);
    crate::model::DatabaseLocator {
        storage_path,
        namespace: "ns".to_string(),
        database: "db".to_string(),
    }
}

fn ingest_request(sequence: u64, text: &str) -> IngestTextRequest {
    IngestTextRequest {
        stream_id: "stream-a".to_string(),
        source_kind: "terminal".to_string(),
        object_kind: TextObjectKind::SystemOutput,
        sequence,
        ts_unix_ms: sequence as i64,
        text: text.to_string(),
        metadata: json!({"cwd": "/tmp"}),
    }
}

fn episode_request() -> CreateEpisodeRequest {
    CreateEpisodeRequest {
        episode_id: "ep-1".to_string(),
        stream_id: Some("stream-a".to_string()),
        source_kind: "terminal".to_string(),
        episode_kind: "command_round".to_string(),
        started_at_unix_ms: 1,
        intent: Some("inspect state".to_string()),
        metadata: json!({"cwd": "/tmp"}),
    }
}

fn outcome_request() -> RecordOutcomeRequest {
    RecordOutcomeRequest {
        outcome_id: "out-1".to_string(),
        episode_id: "ep-1".to_string(),
        status: OutcomeStatus::Succeeded,
        recorded_at_unix_ms: 3,
        summary: Some("command completed".to_string()),
        metadata: json!({"exit_code": 0}),
    }
}

#[tokio::test]
async fn episode_flow_roundtrips_through_runtime_and_replays_idempotently() {
    let store = Arc::new(SurrealEmilyStore::new());
    let runtime = EmilyRuntime::new(store.clone());
    let locator = locator();
    runtime
        .open_db(locator.clone())
        .await
        .expect("open runtime");

    runtime
        .ingest_text(ingest_request(1, "stdout line"))
        .await
        .expect("ingest text");

    runtime
        .create_episode(episode_request())
        .await
        .expect("create episode");
    runtime
        .link_text_to_episode(TraceLinkRequest {
            episode_id: "ep-1".to_string(),
            object_id: "stream-a:1".to_string(),
            trace_kind: EpisodeTraceKind::Output,
            linked_at_unix_ms: 2,
            metadata: json!({"source": "terminal"}),
        })
        .await
        .expect("link text");
    runtime
        .record_outcome(outcome_request())
        .await
        .expect("record outcome");
    runtime
        .append_audit_record(AppendAuditRecordRequest {
            audit_id: "audit-1".to_string(),
            episode_id: "ep-1".to_string(),
            kind: AuditRecordKind::OutcomeRecorded,
            ts_unix_ms: 4,
            summary: "outcome stored".to_string(),
            metadata: json!({"origin": "test"}),
        })
        .await
        .expect("append audit");

    runtime
        .create_episode(episode_request())
        .await
        .expect("replay episode");
    runtime
        .link_text_to_episode(TraceLinkRequest {
            episode_id: "ep-1".to_string(),
            object_id: "stream-a:1".to_string(),
            trace_kind: EpisodeTraceKind::Output,
            linked_at_unix_ms: 2,
            metadata: json!({"source": "terminal"}),
        })
        .await
        .expect("replay link");
    runtime
        .record_outcome(outcome_request())
        .await
        .expect("replay outcome");
    runtime
        .append_audit_record(AppendAuditRecordRequest {
            audit_id: "audit-1".to_string(),
            episode_id: "ep-1".to_string(),
            kind: AuditRecordKind::OutcomeRecorded,
            ts_unix_ms: 4,
            summary: "outcome stored".to_string(),
            metadata: json!({"origin": "test"}),
        })
        .await
        .expect("replay audit");

    let episode = runtime
        .episode("ep-1")
        .await
        .expect("read episode")
        .expect("episode exists");
    let links = store
        .list_episode_trace_links("ep-1")
        .await
        .expect("list links");
    let outcomes = store.list_outcomes("ep-1").await.expect("list outcomes");
    let audits = store.list_audit_records("ep-1").await.expect("list audits");

    assert_eq!(episode.state, EpisodeState::Completed);
    assert_eq!(episode.last_outcome_id.as_deref(), Some("out-1"));
    assert_eq!(episode.closed_at_unix_ms, Some(3));
    assert_eq!(links.len(), 1);
    assert_eq!(outcomes.len(), 1);
    assert_eq!(audits.len(), 1);

    runtime.close_db().await.expect("close runtime");
    let _ = std::fs::remove_dir_all(locator.storage_path);
}

#[tokio::test]
async fn record_outcome_replay_repairs_episode_projection() {
    let store = Arc::new(SurrealEmilyStore::new());
    let runtime = EmilyRuntime::new(store.clone());
    let locator = locator();
    runtime
        .open_db(locator.clone())
        .await
        .expect("open runtime");

    store
        .upsert_episode(&EpisodeRecord {
            id: "ep-1".to_string(),
            stream_id: Some("stream-a".to_string()),
            source_kind: "terminal".to_string(),
            episode_kind: "command_round".to_string(),
            state: EpisodeState::Open,
            started_at_unix_ms: 1,
            closed_at_unix_ms: None,
            intent: Some("inspect state".to_string()),
            metadata: json!({"cwd": "/tmp"}),
            last_outcome_id: None,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
        })
        .await
        .expect("seed episode");
    store
        .upsert_outcome(&crate::model::OutcomeRecord {
            id: "out-1".to_string(),
            episode_id: "ep-1".to_string(),
            status: OutcomeStatus::Succeeded,
            recorded_at_unix_ms: 3,
            summary: Some("command completed".to_string()),
            metadata: json!({"exit_code": 0}),
        })
        .await
        .expect("seed outcome only");

    runtime
        .record_outcome(outcome_request())
        .await
        .expect("replay outcome");

    let repaired = runtime
        .episode("ep-1")
        .await
        .expect("read repaired episode")
        .expect("episode exists");
    assert_eq!(repaired.state, EpisodeState::Completed);
    assert_eq!(repaired.last_outcome_id.as_deref(), Some("out-1"));
    assert_eq!(repaired.closed_at_unix_ms, Some(3));

    runtime.close_db().await.expect("close runtime");
    let _ = std::fs::remove_dir_all(locator.storage_path);
}

#[tokio::test]
async fn episode_read_api_returns_none_for_missing_episode() {
    let store = Arc::new(SurrealEmilyStore::new());
    let runtime = EmilyRuntime::new(store);
    let locator = locator();
    runtime
        .open_db(locator.clone())
        .await
        .expect("open runtime");

    let episode = runtime
        .episode("missing")
        .await
        .expect("read missing episode");
    assert!(episode.is_none());

    runtime.close_db().await.expect("close runtime");
    let _ = std::fs::remove_dir_all(locator.storage_path);
}
