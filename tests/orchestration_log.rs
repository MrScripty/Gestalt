use gestalt::orchestration_log::{
    CommandKind, CommandPayload, EventPayload, NewCommandRecord, NewEventRecord, NewReceiptRecord,
    OrchestrationLogStore, ReceiptPayload, ReceiptStatus, TimelineEntry,
};
use gestalt::orchestrator;
use gestalt::state::AppState;
use gestalt::terminal::TerminalManager;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn unique_db_path(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    std::env::temp_dir().join(format!("gestalt-{name}-{nonce}.sqlite3"))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

#[test]
fn timeline_replay_preserves_sequence_and_timestamps() {
    let _guard = env_lock().lock().expect("env lock");
    let path = unique_db_path("orchestration-log-replay");
    let store = OrchestrationLogStore::new(path.clone());
    let recorded_at = now_ms();
    let command_id = "cmd-replay".to_string();
    let timeline_id = command_id.clone();

    store
        .record_command(NewCommandRecord {
            command_id: command_id.clone(),
            timeline_id: timeline_id.clone(),
            requested_at_unix_ms: recorded_at - 40,
            recorded_at_unix_ms: recorded_at - 30,
            payload: CommandPayload::BroadcastSendLine {
                group_id: 7,
                group_path: "/tmp/replay".to_string(),
                session_ids: vec![11, 12],
                line: "cargo test".to_string(),
            },
        })
        .expect("command should record");
    store
        .append_event(
            &command_id,
            NewEventRecord {
                occurred_at_unix_ms: recorded_at - 20,
                recorded_at_unix_ms: recorded_at - 10,
                payload: EventPayload::BroadcastWriteSucceeded { session_id: 11 },
            },
        )
        .expect("event should record");
    store
        .append_event(
            &command_id,
            NewEventRecord {
                occurred_at_unix_ms: recorded_at - 15,
                recorded_at_unix_ms: recorded_at - 5,
                payload: EventPayload::BroadcastWriteFailed {
                    session_id: 12,
                    error: "session missing".to_string(),
                },
            },
        )
        .expect("event should record");
    store
        .finalize_receipt(
            &command_id,
            NewReceiptRecord {
                completed_at_unix_ms: recorded_at,
                recorded_at_unix_ms: recorded_at + 1,
                status: ReceiptStatus::PartiallySucceeded,
                payload: ReceiptPayload::Broadcast {
                    ok_count: 1,
                    fail_count: 1,
                },
            },
        )
        .expect("receipt should record");

    let timeline = store
        .load_timeline(&command_id)
        .expect("timeline should load");
    assert_eq!(timeline.len(), 4);
    assert_eq!(
        timeline
            .iter()
            .map(TimelineEntry::sequence_in_timeline)
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
    match &timeline[0] {
        TimelineEntry::Command(command) => {
            assert_eq!(command.timeline_id, timeline_id);
            assert_eq!(command.requested_at_unix_ms, recorded_at - 40);
        }
        other => panic!("expected command entry, got {other:?}"),
    }
    match &timeline[3] {
        TimelineEntry::Receipt(receipt) => {
            assert_eq!(receipt.completed_at_unix_ms, recorded_at);
            assert_eq!(receipt.status, ReceiptStatus::PartiallySucceeded);
        }
        other => panic!("expected receipt entry, got {other:?}"),
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn duplicate_command_id_is_rejected_and_recent_commands_are_queryable() {
    let _guard = env_lock().lock().expect("env lock");
    let path = unique_db_path("orchestration-log-duplicate");
    let store = OrchestrationLogStore::new(path.clone());
    let recorded_at = now_ms();
    let command = NewCommandRecord {
        command_id: "cmd-duplicate".to_string(),
        timeline_id: "cmd-duplicate".to_string(),
        requested_at_unix_ms: recorded_at,
        recorded_at_unix_ms: recorded_at,
        payload: CommandPayload::GitCreateCommit {
            group_path: "/tmp/repo".to_string(),
            title: "feat: add orchestration log".to_string(),
            has_message_body: false,
        },
    };

    store
        .record_command(command.clone())
        .expect("initial command should record");
    let duplicate = store
        .record_command(command)
        .expect_err("duplicate should fail");
    assert!(duplicate.to_string().contains("already exists"));

    let recent = store
        .load_recent_commands(4)
        .expect("recent commands should load");
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].command_id, "cmd-duplicate");

    let _ = std::fs::remove_file(path);
}

#[test]
fn group_broadcast_records_failed_timeline_when_sessions_are_unavailable() {
    let _guard = env_lock().lock().expect("env lock");
    let path = unique_db_path("orchestration-log-broadcast");
    unsafe {
        std::env::set_var("GESTALT_ORCHESTRATION_DB_PATH", &path);
    }

    let state = AppState::default();
    let group_id = state.groups()[0].id;
    let terminal_manager = TerminalManager::new();

    let results = orchestrator::broadcast_line_to_group(
        state.workspace_state(),
        &terminal_manager,
        group_id,
        "echo orchestration-log",
    );
    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|result| result.error.is_some()));

    let store = OrchestrationLogStore::default();
    let recent = store
        .load_recent_commands(1)
        .expect("recent command should load");
    assert_eq!(recent.len(), 1);
    let timeline = store
        .load_timeline(&recent[0].command_id)
        .expect("timeline should load");
    assert_eq!(timeline.len(), 5);
    assert!(matches!(timeline.first(), Some(TimelineEntry::Command(_))));
    assert!(matches!(timeline.last(), Some(TimelineEntry::Receipt(_))));
    let failed_events = timeline
        .iter()
        .filter(|entry| {
            matches!(
                entry,
                TimelineEntry::Event(event)
                    if matches!(event.payload, EventPayload::BroadcastWriteFailed { .. })
            )
        })
        .count();
    assert_eq!(failed_events, 3);

    unsafe {
        std::env::remove_var("GESTALT_ORCHESTRATION_DB_PATH");
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn local_agent_send_records_distinct_timeline_kind() {
    let _guard = env_lock().lock().expect("env lock");
    let path = unique_db_path("orchestration-log-local-agent");
    unsafe {
        std::env::set_var("GESTALT_ORCHESTRATION_DB_PATH", &path);
    }

    let state = AppState::default();
    let group_id = state.groups()[0].id;
    let terminal_manager = TerminalManager::new();

    let results = orchestrator::send_local_agent_command_to_group(
        state.workspace_state(),
        &terminal_manager,
        group_id,
        "cargo check",
    );
    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|result| result.error.is_some()));

    let store = OrchestrationLogStore::default();
    let recent = store
        .load_recent_commands(1)
        .expect("recent command should load");
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].kind, CommandKind::LocalAgentSendLine);
    let timeline = store
        .load_timeline(&recent[0].command_id)
        .expect("timeline should load");
    assert_eq!(timeline.len(), 5);
    assert!(matches!(timeline.first(), Some(TimelineEntry::Command(_))));
    assert!(matches!(timeline.last(), Some(TimelineEntry::Receipt(_))));

    let receipt = timeline.last().expect("receipt entry should exist");
    match receipt {
        TimelineEntry::Receipt(receipt) => match &receipt.payload {
            ReceiptPayload::LocalAgent {
                ok_count,
                fail_count,
                action,
            } => {
                assert_eq!(*ok_count, 0);
                assert_eq!(*fail_count, 3);
                assert_eq!(action, "send_line");
            }
            other => panic!("expected local-agent receipt, got {other:?}"),
        },
        other => panic!("expected receipt entry, got {other:?}"),
    }

    unsafe {
        std::env::remove_var("GESTALT_ORCHESTRATION_DB_PATH");
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn recent_activity_for_group_path_includes_receipt_status() {
    let _guard = env_lock().lock().expect("env lock");
    let path = unique_db_path("orchestration-log-activity");
    let store = OrchestrationLogStore::new(path.clone());
    let recorded_at = now_ms();

    store
        .record_command(NewCommandRecord {
            command_id: "cmd-activity-1".to_string(),
            timeline_id: "cmd-activity-1".to_string(),
            requested_at_unix_ms: recorded_at - 20,
            recorded_at_unix_ms: recorded_at - 20,
            payload: CommandPayload::LocalAgentSendLine {
                group_id: 1,
                group_path: "/tmp/activity".to_string(),
                session_ids: vec![9, 10],
                line: "cargo check".to_string(),
            },
        })
        .expect("command should record");
    store
        .finalize_receipt(
            "cmd-activity-1",
            NewReceiptRecord {
                completed_at_unix_ms: recorded_at - 10,
                recorded_at_unix_ms: recorded_at - 10,
                status: ReceiptStatus::Succeeded,
                payload: ReceiptPayload::LocalAgent {
                    ok_count: 2,
                    fail_count: 0,
                    action: "send_line".to_string(),
                },
            },
        )
        .expect("receipt should record");

    store
        .record_command(NewCommandRecord {
            command_id: "cmd-activity-2".to_string(),
            timeline_id: "cmd-activity-2".to_string(),
            requested_at_unix_ms: recorded_at,
            recorded_at_unix_ms: recorded_at,
            payload: CommandPayload::GitCreateCommit {
                group_path: "/tmp/activity".to_string(),
                title: "feat: capture orchestration activity".to_string(),
                has_message_body: false,
            },
        })
        .expect("command should record");
    store
        .finalize_receipt(
            "cmd-activity-2",
            NewReceiptRecord {
                completed_at_unix_ms: recorded_at + 1,
                recorded_at_unix_ms: recorded_at + 1,
                status: ReceiptStatus::Failed,
                payload: ReceiptPayload::Git {
                    ok_count: 0,
                    fail_count: 1,
                    summary: "commit failed".to_string(),
                },
            },
        )
        .expect("receipt should record");

    let activity = store
        .load_recent_activity_for_group_path("/tmp/activity", 4)
        .expect("recent activity should load");
    assert_eq!(activity.len(), 2);
    assert_eq!(activity[0].command.command_id, "cmd-activity-2");
    assert_eq!(
        activity[0]
            .receipt
            .as_ref()
            .expect("receipt should exist")
            .status,
        ReceiptStatus::Failed
    );
    assert_eq!(activity[1].command.command_id, "cmd-activity-1");
    assert_eq!(
        activity[1]
            .receipt
            .as_ref()
            .expect("receipt should exist")
            .status,
        ReceiptStatus::Succeeded
    );

    let _ = std::fs::remove_file(path);
}
