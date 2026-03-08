use super::*;
use crate::model::{
    AuditRecord, AuditRecordKind, EpisodeRecord, EpisodeState, EpisodeTraceKind, EpisodeTraceLink,
    HistoryPageRequest, OutcomeRecord, OutcomeStatus, TextEdgeType, TextObjectKind,
};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

fn locator() -> DatabaseLocator {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch");
    let storage_path = std::env::temp_dir().join(format!(
        "emily-surreal-test-{}-{}",
        std::process::id(),
        now.as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&storage_path);
    DatabaseLocator {
        storage_path,
        namespace: "ns".to_string(),
        database: "db".to_string(),
    }
}

fn sample_object(sequence: u64, text: &str) -> TextObject {
    TextObject {
        id: format!("stream-a:{sequence}"),
        stream_id: "stream-a".to_string(),
        source_kind: "terminal".to_string(),
        object_kind: TextObjectKind::SystemOutput,
        sequence,
        ts_unix_ms: sequence as i64,
        text: text.to_string(),
        metadata: json!({}),
        epsilon: None,
        confidence: 1.0,
        outcome_factor: 0.5,
        novelty_factor: 0.5,
        stability_factor: 1.0,
        learning_weight: 1.0,
        gate_score: None,
        integrated: false,
        quarantine_score: 0.0,
    }
}

#[tokio::test]
async fn open_insert_and_page_history_roundtrip() {
    let store = SurrealEmilyStore::new();
    let locator = locator();
    store.open(&locator).await.expect("open store");
    store
        .insert_text_object(&sample_object(1, "hello world"))
        .await
        .expect("insert 1");
    store
        .insert_text_object(&sample_object(2, "second line"))
        .await
        .expect("insert 2");

    store
        .upsert_text_vector(&TextVector {
            id: "vec:stream-a:2".to_string(),
            object_id: "stream-a:2".to_string(),
            stream_id: "stream-a".to_string(),
            sequence: 2,
            ts_unix_ms: 2,
            dimensions: 1024,
            profile_id: "qwen3-0.6b".to_string(),
            vector: vec![0.0; 1024],
        })
        .await
        .expect("upsert vector");

    let page = store
        .page_history_before(&HistoryPageRequest {
            stream_id: "stream-a".to_string(),
            before_sequence: None,
            limit: 1,
        })
        .await
        .expect("page history");
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].sequence, 2);
    assert_eq!(page.next_before_sequence, Some(2));

    store.close().await.expect("close store");
    let _ = std::fs::remove_dir_all(locator.storage_path);
}

#[tokio::test]
async fn list_text_edges_returns_neighbors_within_depth() {
    let store = SurrealEmilyStore::new();
    let locator = locator();
    store.open(&locator).await.expect("open store");

    let edge_ab = TextEdge {
        id: "edge:a:b".to_string(),
        from_id: "a".to_string(),
        to_id: "b".to_string(),
        edge_type: TextEdgeType::SemanticSimilar,
        weight: 0.9,
        ts_unix_ms: 1,
    };
    let edge_bc = TextEdge {
        id: "edge:b:c".to_string(),
        from_id: "b".to_string(),
        to_id: "c".to_string(),
        edge_type: TextEdgeType::SemanticSimilar,
        weight: 0.8,
        ts_unix_ms: 2,
    };

    store.upsert_text_edge(&edge_ab).await.expect("upsert ab");
    store.upsert_text_edge(&edge_bc).await.expect("upsert bc");

    let depth_one = store
        .list_text_edges(&["a".to_string()], 1)
        .await
        .expect("list depth one");
    assert_eq!(depth_one.len(), 1);
    assert_eq!(depth_one[0].from_id, edge_ab.from_id);
    assert_eq!(depth_one[0].to_id, edge_ab.to_id);

    let depth_two = store
        .list_text_edges(&["a".to_string()], 2)
        .await
        .expect("list depth two");
    assert_eq!(depth_two.len(), 2);

    store.close().await.expect("close store");
    let _ = std::fs::remove_dir_all(locator.storage_path);
}

#[tokio::test]
async fn episode_records_roundtrip() {
    let store = SurrealEmilyStore::new();
    let locator = locator();
    store.open(&locator).await.expect("open store");

    let episode = EpisodeRecord {
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
    };
    let link = EpisodeTraceLink {
        id: "episode:ep-1:output:stream-a:1".to_string(),
        episode_id: "ep-1".to_string(),
        object_id: "stream-a:1".to_string(),
        trace_kind: EpisodeTraceKind::Output,
        linked_at_unix_ms: 2,
        metadata: json!({"source": "terminal"}),
    };
    let outcome = OutcomeRecord {
        id: "out-1".to_string(),
        episode_id: "ep-1".to_string(),
        status: OutcomeStatus::Succeeded,
        recorded_at_unix_ms: 3,
        summary: Some("command completed".to_string()),
        metadata: json!({"exit_code": 0}),
    };
    let audit = AuditRecord {
        id: "audit-1".to_string(),
        episode_id: "ep-1".to_string(),
        kind: AuditRecordKind::OutcomeRecorded,
        ts_unix_ms: 4,
        summary: "outcome stored".to_string(),
        metadata: json!({"origin": "test"}),
    };

    store
        .upsert_episode(&episode)
        .await
        .expect("upsert episode");
    store
        .upsert_episode_trace_link(&link)
        .await
        .expect("upsert trace link");
    store
        .upsert_outcome(&outcome)
        .await
        .expect("upsert outcome");
    store
        .upsert_audit_record(&audit)
        .await
        .expect("upsert audit");

    assert_eq!(
        store
            .get_episode("ep-1")
            .await
            .expect("get episode")
            .expect("episode"),
        episode
    );
    assert_eq!(
        store
            .list_episode_trace_links("ep-1")
            .await
            .expect("list trace links"),
        vec![link]
    );
    assert_eq!(
        store.list_outcomes("ep-1").await.expect("list outcomes"),
        vec![outcome]
    );
    assert_eq!(
        store.list_audit_records("ep-1").await.expect("list audits"),
        vec![audit]
    );

    store.close().await.expect("close store");
    let _ = std::fs::remove_dir_all(locator.storage_path);
}
