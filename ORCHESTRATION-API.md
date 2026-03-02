# Orchestration API (Local Agents)

This project exposes an in-process orchestration API in:

- `src/orchestrator.rs`

It is designed for a local agent controller that can inspect terminal state for a group and issue commands/interruption across that group.

## Scope

- Current transport: in-process Rust calls only (no HTTP/IPC server yet).
- Target: all terminal sessions in one `group_id`.
- Data source: `AppState` + `TerminalManager` snapshots.

## Core Types

### `TerminalRound`

Represents the latest command round for a terminal:

- `start_row: u16`
- `end_row: u16`
- `lines: Vec<String>`

Helper:

- `text() -> String`: joins `lines` with `\n`.

### `GroupTerminalState`

Per-terminal orchestration view:

- `session_id: SessionId`
- `title: String`
- `role: SessionRole`
- `status: SessionStatus`
- `cwd: String`
- `is_selected: bool`
- `is_focused: bool`
- `is_runtime_ready: bool`
- `latest_round: TerminalRound`

### `GroupOrchestratorSnapshot`

Group-level snapshot:

- `group_id: GroupId`
- `group_path: String`
- `terminals: Vec<GroupTerminalState>`

### `SessionWriteResult`

Result of a write/interrupt attempt:

- `session_id: SessionId`
- `error: Option<String>` (`None` means success)

## Public API Functions

### `snapshot_group(app_state, terminal_manager, group_id, focused_session)`

Builds a `GroupOrchestratorSnapshot` for the provided group.

Use this to:

- read each terminal’s latest round
- inspect active/focused/selected state
- inspect runtime readiness and cwd

### `group_session_ids(app_state, group_id)`

Returns all `SessionId`s currently in the group.

Use this as the input session list for broadcast operations.

### `send_line_to_sessions(terminal_manager, session_ids, line)`

Sends one command line (with Enter) to each session.

Returns `Vec<SessionWriteResult>` with per-session success/failure.

### `interrupt_sessions(terminal_manager, session_ids)`

Sends `Ctrl+C` (`0x03`) to each session.

Returns `Vec<SessionWriteResult>` with per-session success/failure.

## Typical Flow

1. Resolve active `group_id`.
2. Read snapshot via `snapshot_group(...)`.
3. Decide action from `latest_round` + status/active flags.
4. Broadcast with `send_line_to_sessions(...)` or stop with `interrupt_sessions(...)`.
5. Apply results to UI/state (set session status to `Error` on failure; `Idle/Busy` are reconciled from runtime activity).

## Example (in-process)

```rust
use crate::orchestrator;

let snapshot = orchestrator::snapshot_group(&app_state, &terminal_manager, group_id, focused);
let ids = orchestrator::group_session_ids(&app_state, group_id);

let results = orchestrator::send_line_to_sessions(&mut terminal_manager, &ids, "cargo test -q");
// or:
// let results = orchestrator::interrupt_sessions(&mut terminal_manager, &ids);

for result in results {
    if result.error.is_some() {
        // mark error
    }
}
```

## Notes and Limits

- Latest round detection is prompt-based. Prompt format assumptions are aligned with current UI parsing.
- Writes are best-effort per terminal; one failure does not stop others.
- This is orchestration scaffolding, not a full autonomous planner/runtime.
- If you need out-of-process agents, add an IPC/HTTP layer that wraps these functions.
