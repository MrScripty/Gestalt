use gestalt::orchestration_log::{
    CommandPayload, EventPayload, NewCommandRecord, NewEventRecord, NewReceiptRecord,
    OrchestrationLogStore, ReceiptPayload, ReceiptStatus, TimelineEntry,
};
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
