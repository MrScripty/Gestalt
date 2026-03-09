use emily::api::EmilyApi;
use emily::model::{
    CreateEpisodeRequest, DatabaseLocator, RemoteEpisodeRequest, RoutingDecision,
    RoutingDecisionKind, RoutingTarget, UpdateRemoteEpisodeStateRequest, ValidationDecision,
    ValidationFinding, ValidationFindingSeverity, ValidationOutcome,
};
use emily::runtime::EmilyRuntime;
use emily::store::surreal::SurrealEmilyStore;
use gestalt::emily_bridge::EmilyBridge;
use gestalt::orchestration_activity::load_recent_activity_snapshot;
use gestalt::orchestration_log::{
    CommandPayload, NewCommandRecord, NewReceiptRecord, OrchestrationLogStore, ReceiptPayload,
    ReceiptStatus,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn recent_activity_snapshot_includes_emily_state_for_run_linked_activity() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let locator = unique_locator("emily-orchestration-activity");
    let storage_path = locator.storage_path.clone();
    let orchestration_db = unique_orchestration_db_path("emily-orchestration-activity");
    std::fs::create_dir_all(
        orchestration_db
            .parent()
            .expect("orchestration db should have parent"),
    )
    .expect("orchestration db root should exist");

    unsafe {
        std::env::set_var("GESTALT_ORCHESTRATION_DB_PATH", &orchestration_db);
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");

    runtime.block_on(async {
        let emily_runtime = EmilyRuntime::new(Arc::new(SurrealEmilyStore::new()));
        emily_runtime
            .open_db(locator.clone())
            .await
            .expect("db should open");
        let _ = emily_runtime
            .create_episode(CreateEpisodeRequest {
                episode_id: "local-agent:run-activity".to_string(),
                stream_id: Some("terminal:1".to_string()),
                source_kind: "gestalt-local-agent".to_string(),
                episode_kind: "local_agent_run".to_string(),
                started_at_unix_ms: current_unix_ms(),
                intent: Some("cargo check".to_string()),
                metadata: serde_json::json!({
                    "group_id": 1,
                    "group_path": "/workspace/demo",
                    "run_id": "run-activity",
                }),
            })
            .await
            .expect("episode should create");
        let _ = emily_runtime
            .record_routing_decision(RoutingDecision {
                decision_id: "route-activity".to_string(),
                episode_id: "local-agent:run-activity".to_string(),
                kind: RoutingDecisionKind::SingleRemote,
                decided_at_unix_ms: current_unix_ms(),
                rationale: Some("test remote routing".to_string()),
                targets: vec![RoutingTarget {
                    provider_id: "stub-remote".to_string(),
                    model_id: Some("qwen".to_string()),
                    profile_id: Some("reasoning".to_string()),
                    capability_tags: vec!["analysis".to_string()],
                    metadata: serde_json::json!({}),
                }],
                metadata: serde_json::json!({}),
            })
            .await
            .expect("route should record");
        let _ = emily_runtime
            .create_remote_episode(RemoteEpisodeRequest {
                remote_episode_id: "remote-activity".to_string(),
                episode_id: "local-agent:run-activity".to_string(),
                route_decision_id: Some("route-activity".to_string()),
                dispatch_kind: "single_remote".to_string(),
                dispatched_at_unix_ms: current_unix_ms(),
                metadata: serde_json::json!({}),
            })
            .await
            .expect("remote episode should record");
        let _ = emily_runtime
            .update_remote_episode_state(UpdateRemoteEpisodeStateRequest {
                remote_episode_id: "remote-activity".to_string(),
                next_state: emily::model::RemoteEpisodeState::Succeeded,
                transitioned_at_unix_ms: current_unix_ms(),
                summary: Some("completed".to_string()),
                metadata: serde_json::json!({}),
            })
            .await
            .expect("remote episode should complete");
        let _ = emily_runtime
            .record_validation_outcome(ValidationOutcome {
                validation_id: "validation-activity".to_string(),
                episode_id: "local-agent:run-activity".to_string(),
                remote_episode_id: Some("remote-activity".to_string()),
                decision: ValidationDecision::AcceptedWithCaution,
                validated_at_unix_ms: current_unix_ms(),
                findings: vec![ValidationFinding {
                    code: "thin-context".to_string(),
                    severity: ValidationFindingSeverity::Warning,
                    message: "needs a little more context".to_string(),
                }],
                metadata: serde_json::json!({}),
            })
            .await
            .expect("validation should record");
        emily_runtime
            .close_db()
            .await
            .expect("db should close cleanly");
    });

    let store = OrchestrationLogStore::default();
    let command_id = "activity-cmd-1".to_string();
    let now_ms = current_unix_ms();
    store
        .record_command(NewCommandRecord {
            command_id: command_id.clone(),
            timeline_id: command_id.clone(),
            requested_at_unix_ms: now_ms,
            recorded_at_unix_ms: now_ms,
            payload: CommandPayload::LocalAgentSendLine {
                group_id: 1,
                group_path: "/workspace/demo".to_string(),
                session_ids: vec![1],
                line: "cargo check".to_string(),
                display_line: Some("cargo check".to_string()),
                run_id: Some("run-activity".to_string()),
            },
        })
        .expect("command should record");
    store
        .finalize_receipt(
            &command_id,
            NewReceiptRecord {
                completed_at_unix_ms: now_ms + 10,
                recorded_at_unix_ms: now_ms + 10,
                status: ReceiptStatus::Succeeded,
                payload: ReceiptPayload::LocalAgent {
                    ok_count: 1,
                    fail_count: 0,
                    action: "send".to_string(),
                },
            },
        )
        .expect("receipt should record");

    let bridge = Arc::new(EmilyBridge::new(locator));
    let snapshots = runtime
        .block_on(load_recent_activity_snapshot(
            bridge.clone(),
            "/workspace/demo".to_string(),
            6,
        ))
        .expect("activity snapshot should load");

    assert_eq!(snapshots.len(), 1);
    let emily = snapshots[0]
        .emily
        .as_ref()
        .expect("run-linked activity should include Emily state");
    assert_eq!(emily.episode_id, "local-agent:run-activity");
    assert_eq!(
        emily.latest_route_kind,
        Some(RoutingDecisionKind::SingleRemote)
    );
    assert_eq!(
        emily.latest_validation_decision,
        Some(ValidationDecision::AcceptedWithCaution)
    );
    assert_eq!(
        emily.latest_remote_state,
        Some(emily::model::RemoteEpisodeState::Succeeded)
    );

    drop(bridge);
    thread::sleep(Duration::from_millis(50));
    unsafe {
        std::env::remove_var("GESTALT_ORCHESTRATION_DB_PATH");
    }
    let _ = std::fs::remove_dir_all(storage_path);
    let _ = std::fs::remove_dir_all(
        orchestration_db
            .parent()
            .expect("orchestration db should have parent"),
    );
}

fn unique_locator(name: &str) -> DatabaseLocator {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let storage_path =
        std::env::temp_dir().join(format!("gestalt-{name}-{nonce}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&storage_path);
    DatabaseLocator {
        storage_path,
        namespace: "gestalt_test".to_string(),
        database: "default".to_string(),
    }
}

fn unique_orchestration_db_path(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    std::env::temp_dir()
        .join(format!("gestalt-{name}-{nonce}"))
        .join("orchestration.sqlite3")
}

fn current_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as i64)
}
