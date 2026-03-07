use crate::orchestration_log::{
    CommandPayload, EventPayload, NewCommandRecord, NewEventRecord, NewReceiptRecord,
    OrchestrationLogStore, ReceiptPayload, ReceiptStatus,
};
use crate::state::{AppState, GroupId, SessionId, SessionRole, SessionStatus};
use crate::terminal::TerminalManager;
use std::collections::HashMap;
use uuid::Uuid;

const MAX_ROUND_LINES: usize = 8;
const MAX_PROMPT_SCAN_LINES: usize = 2_048;

/// Extracted round boundaries and text for a terminal buffer.
#[derive(Debug, Clone, PartialEq)]
pub struct TerminalRound {
    pub start_row: u16,
    pub end_row: u16,
    pub lines: Vec<String>,
}

impl TerminalRound {
    /// Returns the round as newline-delimited plain text.
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }
}

/// Snapshot entry for a single terminal in a group orchestrator view.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupTerminalState {
    pub session_id: SessionId,
    pub title: String,
    pub role: SessionRole,
    pub status: SessionStatus,
    pub cwd: String,
    pub is_selected: bool,
    pub is_focused: bool,
    pub is_runtime_ready: bool,
    pub latest_round: TerminalRound,
}

/// Group-level snapshot consumed by local orchestration controls.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupOrchestratorSnapshot {
    pub group_id: GroupId,
    pub group_path: String,
    pub terminals: Vec<GroupTerminalState>,
}

/// Result for a write/broadcast operation against one session.
#[derive(Debug, Clone)]
pub struct SessionWriteResult {
    pub session_id: SessionId,
    pub error: Option<String>,
}

/// Lightweight runtime data used to build orchestrator snapshots.
#[derive(Debug, Clone, Copy)]
pub struct SessionRuntimeView<'a> {
    pub lines: &'a [String],
    pub cwd: &'a str,
    pub is_runtime_ready: bool,
}

/// Builds a group snapshot directly from live terminal manager runtime.
pub fn snapshot_group(
    app_state: &AppState,
    terminal_manager: &TerminalManager,
    group_id: GroupId,
    focused_session: Option<SessionId>,
) -> GroupOrchestratorSnapshot {
    let group_path = app_state.group_path(group_id).unwrap_or(".").to_string();
    let terminals = app_state
        .sessions_in_group(group_id)
        .into_iter()
        .map(|session| {
            let runtime_snapshot = terminal_manager.snapshot(session.id);
            let lines = runtime_snapshot
                .as_ref()
                .map(|snapshot| snapshot.lines.clone())
                .unwrap_or_default();

            GroupTerminalState {
                session_id: session.id,
                title: session.title,
                role: session.role,
                status: session.status,
                cwd: terminal_manager
                    .session_cwd(session.id)
                    .unwrap_or_else(|| group_path.clone()),
                is_selected: app_state.selected_session() == Some(session.id),
                is_focused: focused_session == Some(session.id),
                is_runtime_ready: runtime_snapshot.is_some(),
                latest_round: latest_round_from_lines(&lines),
            }
        })
        .collect();

    GroupOrchestratorSnapshot {
        group_id,
        group_path,
        terminals,
    }
}

/// Builds a group snapshot from caller-provided runtime state.
pub fn snapshot_group_from_runtime(
    app_state: &AppState,
    group_id: GroupId,
    focused_session: Option<SessionId>,
    runtime_by_session: &HashMap<SessionId, SessionRuntimeView<'_>>,
) -> GroupOrchestratorSnapshot {
    let group_path = app_state.group_path(group_id).unwrap_or(".").to_string();
    let terminals = app_state
        .sessions_in_group(group_id)
        .into_iter()
        .map(|session| {
            let runtime = runtime_by_session.get(&session.id);
            let latest_round = runtime
                .as_ref()
                .map(|runtime| latest_round_from_lines(runtime.lines))
                .unwrap_or_else(|| latest_round_from_lines(&[]));

            GroupTerminalState {
                session_id: session.id,
                title: session.title,
                role: session.role,
                status: session.status,
                cwd: runtime
                    .as_ref()
                    .map(|runtime| runtime.cwd.to_string())
                    .unwrap_or_else(|| group_path.clone()),
                is_selected: app_state.selected_session() == Some(session.id),
                is_focused: focused_session == Some(session.id),
                is_runtime_ready: runtime
                    .as_ref()
                    .map(|runtime| runtime.is_runtime_ready)
                    .unwrap_or(false),
                latest_round,
            }
        })
        .collect();

    GroupOrchestratorSnapshot {
        group_id,
        group_path,
        terminals,
    }
}

/// Returns all session IDs currently attached to a group.
pub fn group_session_ids(app_state: &AppState, group_id: GroupId) -> Vec<SessionId> {
    app_state
        .sessions()
        .iter()
        .filter(|session| session.group_id == group_id)
        .map(|session| session.id)
        .collect()
}

/// Sends a line of input to every provided session.
pub fn send_line_to_sessions(
    terminal_manager: &TerminalManager,
    session_ids: &[SessionId],
    line: &str,
) -> Vec<SessionWriteResult> {
    session_ids
        .iter()
        .copied()
        .map(|session_id| SessionWriteResult {
            session_id,
            error: terminal_manager
                .send_line(session_id, line)
                .err()
                .map(|error| error.to_string()),
        })
        .collect()
}

/// Sends a line of input to every session in one group and records the lifecycle durably.
pub fn broadcast_line_to_group(
    app_state: &AppState,
    terminal_manager: &TerminalManager,
    group_id: GroupId,
    line: &str,
) -> Vec<SessionWriteResult> {
    let session_ids = group_session_ids(app_state, group_id);
    let group_path = app_state.group_path(group_id).unwrap_or(".").to_string();
    let now_ms = current_unix_ms();
    let command_id = Uuid::new_v4().to_string();
    let store = OrchestrationLogStore::default();

    if let Err(error) = store.record_command(NewCommandRecord {
        command_id: command_id.clone(),
        timeline_id: command_id.clone(),
        requested_at_unix_ms: now_ms,
        recorded_at_unix_ms: now_ms,
        payload: CommandPayload::BroadcastSendLine {
            group_id,
            group_path: group_path.clone(),
            session_ids: session_ids.clone(),
            line: line.to_string(),
        },
    }) {
        return log_blocked_results(
            &session_ids,
            format!("failed recording orchestration command: {error}"),
        );
    }

    let results = send_line_to_sessions(terminal_manager, &session_ids, line);
    finalize_group_write(
        &store,
        &command_id,
        &results,
        now_ms,
        ReceiptPayload::Broadcast {
            ok_count: 0,
            fail_count: 0,
        },
    );
    results
}

/// Sends Ctrl+C to every provided session.
pub fn interrupt_sessions(
    terminal_manager: &TerminalManager,
    session_ids: &[SessionId],
) -> Vec<SessionWriteResult> {
    session_ids
        .iter()
        .copied()
        .map(|session_id| SessionWriteResult {
            session_id,
            error: terminal_manager
                .send_input(session_id, &[0x03])
                .err()
                .map(|error| error.to_string()),
        })
        .collect()
}

/// Sends Ctrl+C to every session in one group and records the lifecycle durably.
pub fn interrupt_group(
    app_state: &AppState,
    terminal_manager: &TerminalManager,
    group_id: GroupId,
) -> Vec<SessionWriteResult> {
    let session_ids = group_session_ids(app_state, group_id);
    let group_path = app_state.group_path(group_id).unwrap_or(".").to_string();
    let now_ms = current_unix_ms();
    let command_id = Uuid::new_v4().to_string();
    let store = OrchestrationLogStore::default();

    if let Err(error) = store.record_command(NewCommandRecord {
        command_id: command_id.clone(),
        timeline_id: command_id.clone(),
        requested_at_unix_ms: now_ms,
        recorded_at_unix_ms: now_ms,
        payload: CommandPayload::BroadcastInterrupt {
            group_id,
            group_path,
            session_ids: session_ids.clone(),
        },
    }) {
        return log_blocked_results(
            &session_ids,
            format!("failed recording orchestration command: {error}"),
        );
    }

    let results = interrupt_sessions(terminal_manager, &session_ids);
    finalize_group_write(
        &store,
        &command_id,
        &results,
        now_ms,
        ReceiptPayload::Broadcast {
            ok_count: 0,
            fail_count: 0,
        },
    );
    results
}

/// Sends a local-agent command to every session in one group and records a distinct lifecycle.
pub fn send_local_agent_command_to_group(
    app_state: &AppState,
    terminal_manager: &TerminalManager,
    group_id: GroupId,
    line: &str,
) -> Vec<SessionWriteResult> {
    let session_ids = group_session_ids(app_state, group_id);
    let group_path = app_state.group_path(group_id).unwrap_or(".").to_string();
    let now_ms = current_unix_ms();
    let command_id = Uuid::new_v4().to_string();
    let store = OrchestrationLogStore::default();

    if let Err(error) = store.record_command(NewCommandRecord {
        command_id: command_id.clone(),
        timeline_id: command_id.clone(),
        requested_at_unix_ms: now_ms,
        recorded_at_unix_ms: now_ms,
        payload: CommandPayload::LocalAgentSendLine {
            group_id,
            group_path,
            session_ids: session_ids.clone(),
            line: line.to_string(),
        },
    }) {
        return log_blocked_results(
            &session_ids,
            format!("failed recording orchestration command: {error}"),
        );
    }

    let results = send_line_to_sessions(terminal_manager, &session_ids, line);
    finalize_group_write(
        &store,
        &command_id,
        &results,
        now_ms,
        ReceiptPayload::LocalAgent {
            ok_count: 0,
            fail_count: 0,
            action: "send_line".to_string(),
        },
    );
    results
}

/// Sends Ctrl+C to every session in one group from the local-agent panel and records it separately.
pub fn interrupt_local_agent_group(
    app_state: &AppState,
    terminal_manager: &TerminalManager,
    group_id: GroupId,
) -> Vec<SessionWriteResult> {
    let session_ids = group_session_ids(app_state, group_id);
    let group_path = app_state.group_path(group_id).unwrap_or(".").to_string();
    let now_ms = current_unix_ms();
    let command_id = Uuid::new_v4().to_string();
    let store = OrchestrationLogStore::default();

    if let Err(error) = store.record_command(NewCommandRecord {
        command_id: command_id.clone(),
        timeline_id: command_id.clone(),
        requested_at_unix_ms: now_ms,
        recorded_at_unix_ms: now_ms,
        payload: CommandPayload::LocalAgentInterrupt {
            group_id,
            group_path,
            session_ids: session_ids.clone(),
        },
    }) {
        return log_blocked_results(
            &session_ids,
            format!("failed recording orchestration command: {error}"),
        );
    }

    let results = interrupt_sessions(terminal_manager, &session_ids);
    finalize_group_write(
        &store,
        &command_id,
        &results,
        now_ms,
        ReceiptPayload::LocalAgent {
            ok_count: 0,
            fail_count: 0,
            action: "interrupt".to_string(),
        },
    );
    results
}

fn finalize_group_write(
    store: &OrchestrationLogStore,
    command_id: &str,
    results: &[SessionWriteResult],
    started_at_unix_ms: i64,
    receipt_payload: ReceiptPayload,
) {
    let mut ok_count = 0usize;
    let mut fail_count = 0usize;

    for result in results {
        let payload = match result.error.as_ref() {
            Some(error) => {
                fail_count = fail_count.saturating_add(1);
                EventPayload::BroadcastWriteFailed {
                    session_id: result.session_id,
                    error: error.clone(),
                }
            }
            None => {
                ok_count = ok_count.saturating_add(1);
                EventPayload::BroadcastWriteSucceeded {
                    session_id: result.session_id,
                }
            }
        };
        let event_time = current_unix_ms();
        let _ = store.append_event(
            command_id,
            NewEventRecord {
                occurred_at_unix_ms: event_time.max(started_at_unix_ms),
                recorded_at_unix_ms: event_time,
                payload,
            },
        );
    }

    let status = if fail_count == 0 {
        ReceiptStatus::Succeeded
    } else if ok_count == 0 {
        ReceiptStatus::Failed
    } else {
        ReceiptStatus::PartiallySucceeded
    };
    let completed_at_unix_ms = current_unix_ms();
    let payload = match receipt_payload {
        ReceiptPayload::Broadcast { .. } => ReceiptPayload::Broadcast {
            ok_count,
            fail_count,
        },
        ReceiptPayload::LocalAgent { action, .. } => ReceiptPayload::LocalAgent {
            ok_count,
            fail_count,
            action,
        },
        ReceiptPayload::Git { .. } => ReceiptPayload::Git {
            ok_count,
            fail_count,
            summary: String::new(),
        },
    };
    let _ = store.finalize_receipt(
        command_id,
        NewReceiptRecord {
            completed_at_unix_ms,
            recorded_at_unix_ms: completed_at_unix_ms,
            status,
            payload,
        },
    );
}

fn log_blocked_results(session_ids: &[SessionId], message: String) -> Vec<SessionWriteResult> {
    session_ids
        .iter()
        .copied()
        .map(|session_id| SessionWriteResult {
            session_id,
            error: Some(message.clone()),
        })
        .collect()
}

fn current_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn latest_round_from_lines(lines: &[String]) -> TerminalRound {
    if lines.is_empty() {
        return TerminalRound {
            start_row: 0,
            end_row: 0,
            lines: vec![String::new()],
        };
    }

    let last_non_empty = lines
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .unwrap_or(0);
    let scan_floor = last_non_empty.saturating_sub(MAX_PROMPT_SCAN_LINES.saturating_sub(1));
    let start_idx = (scan_floor..=last_non_empty)
        .rev()
        .find(|idx| {
            is_prompt_row(
                lines
                    .get(*idx)
                    .map(|line| line.as_str())
                    .unwrap_or_default(),
            )
        })
        .unwrap_or(scan_floor);
    let end_idx = last_non_empty.max(start_idx);

    let total_round_lines = end_idx.saturating_sub(start_idx).saturating_add(1);
    let mut round_lines: Vec<String> = lines[start_idx..=end_idx]
        .iter()
        .take(MAX_ROUND_LINES)
        .cloned()
        .collect();
    if total_round_lines > MAX_ROUND_LINES {
        round_lines.push(format!(
            "... {} additional lines omitted",
            total_round_lines - MAX_ROUND_LINES
        ));
    }
    if round_lines.is_empty() {
        round_lines.push(String::new());
    }

    let start_row = u16::try_from(start_idx).unwrap_or(u16::MAX);
    let end_row = u16::try_from(end_idx).unwrap_or(u16::MAX);
    TerminalRound {
        start_row,
        end_row,
        lines: round_lines,
    }
}

fn is_prompt_row(line: &str) -> bool {
    split_prompt_prefix(line).is_some()
}

fn split_prompt_prefix(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return None;
    }

    let leading = line.len().saturating_sub(trimmed.len());

    if trimmed.starts_with("$ ") || trimmed.starts_with("# ") {
        let end = leading + 2;
        return Some((&line[..end], &line[end..]));
    }

    if trimmed == "$" || trimmed == "#" {
        return Some((line, ""));
    }

    if (trimmed.ends_with('$') || trimmed.ends_with('#'))
        && (trimmed.contains('@') || trimmed.contains(':'))
    {
        return Some((line, ""));
    }

    let marker = trimmed.find("$ ").or_else(|| trimmed.find("# "))?;
    let end = leading + marker + 2;
    let prefix = &line[..end];
    if !prefix.contains('@') || !prefix.contains(':') {
        return None;
    }

    Some((prefix, &line[end..]))
}

#[cfg(test)]
mod tests {
    use super::latest_round_from_lines;

    #[test]
    fn latest_round_starts_on_last_prompt_row() {
        let lines = vec![
            "jeremy@box:~/a$ ls".to_string(),
            "Cargo.toml".to_string(),
            "src".to_string(),
            "jeremy@box:~/a$ pwd".to_string(),
            "/tmp/a".to_string(),
        ];

        let round = latest_round_from_lines(&lines);
        assert_eq!(round.start_row, 3);
        assert_eq!(round.end_row, 4);
        assert_eq!(round.lines.len(), 2);
        assert_eq!(round.lines[0], "jeremy@box:~/a$ pwd");
    }

    #[test]
    fn latest_round_handles_wrapped_prompt_marker_rows() {
        let lines = vec![
            "jeremy@box:~/very/long/path/that/wrapped".to_string(),
            "$ cargo check".to_string(),
            "Finished dev profile".to_string(),
        ];

        let round = latest_round_from_lines(&lines);
        assert_eq!(round.start_row, 1);
        assert_eq!(round.end_row, 2);
        assert_eq!(round.lines[0], "$ cargo check");
    }
}
