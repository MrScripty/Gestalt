use super::test_support::{MockStore, ingest_request, locator};
use super::*;
use crate::api::EmilyApi;
use crate::model::{
    AuditRecordKind, EarlDecision, EarlEvaluationRequest, EarlHostAction, EarlSignalVector,
    EpisodeState, EpisodeTraceKind, OutcomeStatus, RecordOutcomeRequest,
};
use crate::store::EmilyStore;
use crate::store::surreal::SurrealEmilyStore;
use serde_json::json;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

fn episode_request() -> CreateEpisodeRequest {
    CreateEpisodeRequest {
        episode_id: "ep-earl".to_string(),
        stream_id: Some("stream-a".to_string()),
        source_kind: "terminal".to_string(),
        episode_kind: "command_round".to_string(),
        started_at_unix_ms: 1,
        intent: Some("evaluate risk".to_string()),
        metadata: json!({"cwd": "/tmp"}),
    }
}

fn earl_request(
    evaluation_id: &str,
    uncertainty: f32,
    conflict: f32,
    continuity_drift: f32,
) -> EarlEvaluationRequest {
    EarlEvaluationRequest {
        evaluation_id: evaluation_id.to_string(),
        episode_id: "ep-earl".to_string(),
        evaluated_at_unix_ms: 2,
        signals: EarlSignalVector {
            uncertainty,
            conflict,
            continuity_drift,
            constraint_pressure: 0.2,
            tool_instability: 0.1,
            novelty_spike: 0.2,
        },
        metadata: json!({"origin": "test"}),
    }
}

fn real_locator() -> DatabaseLocator {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch");
    let storage_path = std::env::temp_dir().join(format!(
        "emily-runtime-earl-test-{}-{}",
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

#[tokio::test]
async fn earl_returns_ok_for_low_risk_episode() {
    let store = Arc::new(MockStore::default());
    let runtime = EmilyRuntime::new(store.clone());
    runtime.open_db(locator()).await.expect("open");
    runtime
        .create_episode(episode_request())
        .await
        .expect("create episode");

    let evaluation = runtime
        .evaluate_episode_risk(earl_request("earl-ok", 0.1, 0.1, 0.1))
        .await
        .expect("evaluate episode");

    assert_eq!(evaluation.decision, EarlDecision::Ok);
    assert_eq!(evaluation.host_action, EarlHostAction::Proceed);
    assert!(!evaluation.retryable);
    assert_eq!(
        store
            .get_episode("ep-earl")
            .await
            .expect("get episode")
            .expect("episode exists")
            .state,
        EpisodeState::Open
    );
}

#[tokio::test]
async fn earl_returns_caution_for_mid_risk_episode() {
    let store = Arc::new(MockStore::default());
    let runtime = EmilyRuntime::new(store.clone());
    runtime.open_db(locator()).await.expect("open");
    runtime
        .create_episode(episode_request())
        .await
        .expect("create episode");

    let evaluation = runtime
        .evaluate_episode_risk(earl_request("earl-caution", 0.7, 0.65, 0.5))
        .await
        .expect("evaluate episode");

    assert_eq!(evaluation.decision, EarlDecision::Caution);
    assert_eq!(evaluation.host_action, EarlHostAction::Clarify);
    assert!(evaluation.retryable);
    assert_eq!(
        store
            .get_episode("ep-earl")
            .await
            .expect("get episode")
            .expect("episode exists")
            .state,
        EpisodeState::Cautioned
    );
}

#[tokio::test]
async fn earl_reflex_blocks_episode_and_prevents_outcomes() {
    let store = Arc::new(SurrealEmilyStore::new());
    let runtime = EmilyRuntime::new(store.clone());
    let locator = real_locator();
    runtime
        .open_db(locator.clone())
        .await
        .expect("open runtime");

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
            episode_id: "ep-earl".to_string(),
            object_id: "stream-a:1".to_string(),
            trace_kind: EpisodeTraceKind::Input,
            linked_at_unix_ms: 2,
            metadata: json!({"source": "terminal"}),
        })
        .await
        .expect("link text");

    let evaluation = runtime
        .evaluate_episode_risk(earl_request("earl-reflex", 0.8, 0.3, 0.95))
        .await
        .expect("evaluate reflex");

    assert_eq!(evaluation.decision, EarlDecision::Reflex);
    assert_eq!(evaluation.host_action, EarlHostAction::Abort);
    assert!(!evaluation.retryable);

    let episode = store
        .get_episode("ep-earl")
        .await
        .expect("get episode")
        .expect("episode exists");
    let object = store
        .get_text_object("stream-a:1")
        .await
        .expect("get text")
        .expect("text exists");
    let evaluations = store
        .list_earl_evaluations("ep-earl")
        .await
        .expect("list earl evaluations");
    let audit = store
        .get_audit_record("audit:earl:earl-reflex")
        .await
        .expect("get audit")
        .expect("audit exists");

    assert_eq!(episode.state, EpisodeState::Blocked);
    assert!(!object.integrated);
    assert!(object.quarantine_score > 0.0);
    assert_eq!(evaluations.len(), 1);
    assert_eq!(audit.kind, AuditRecordKind::EarlEvaluated);

    let outcome_error = runtime
        .record_outcome(RecordOutcomeRequest {
            outcome_id: "out-blocked".to_string(),
            episode_id: "ep-earl".to_string(),
            status: OutcomeStatus::Failed,
            recorded_at_unix_ms: 3,
            summary: Some("blocked".to_string()),
            metadata: json!({"exit_code": 1}),
        })
        .await
        .expect_err("blocked episode should reject outcome");
    assert!(matches!(outcome_error, EmilyError::EpisodeGated(_)));

    runtime.close_db().await.expect("close runtime");
    let _ = std::fs::remove_dir_all(locator.storage_path);
}

#[tokio::test]
async fn latest_earl_evaluation_for_episode_returns_most_recent_record() {
    let store = Arc::new(MockStore::default());
    let runtime = EmilyRuntime::new(store);
    runtime.open_db(locator()).await.expect("open");
    runtime
        .create_episode(episode_request())
        .await
        .expect("create episode");

    runtime
        .evaluate_episode_risk(earl_request("earl-first", 0.1, 0.1, 0.1))
        .await
        .expect("evaluate first");
    runtime
        .evaluate_episode_risk(EarlEvaluationRequest {
            evaluation_id: "earl-second".to_string(),
            episode_id: "ep-earl".to_string(),
            evaluated_at_unix_ms: 4,
            signals: EarlSignalVector {
                uncertainty: 0.7,
                conflict: 0.65,
                continuity_drift: 0.5,
                constraint_pressure: 0.2,
                tool_instability: 0.1,
                novelty_spike: 0.2,
            },
            metadata: json!({"origin": "test"}),
        })
        .await
        .expect("evaluate second");

    let latest = runtime
        .latest_earl_evaluation_for_episode("ep-earl")
        .await
        .expect("read latest earl")
        .expect("latest earl exists");

    assert_eq!(latest.id, "earl-second");
    assert_eq!(latest.decision, EarlDecision::Caution);
}
