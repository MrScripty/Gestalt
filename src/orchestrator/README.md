# orchestrator

## Purpose
`orchestrator` coordinates cross-session operations and emits orchestration events for UI refresh and control flows.

## Contents
| File/Folder | Description |
| ----------- | ----------- |
| `mod.rs` | Module exports and orchestration surface |
| `runtime.rs` | Group snapshot derivation, broadcast terminal actions, and local-agent run start sequencing |
| `workspace.rs` | Active-workspace projection and session-status reconciliation |
| `startup.rs` | Startup target ordering and background session/history coordination |
| `session.rs` | Group/session lifecycle facades used by UI actions |
| `autosave.rs` | Autosave worker and debounce/inflight coordination facade |
| `git.rs` | Orchestrator-facing Git operation wrappers |
| `events.rs` | In-process event bus and event contracts |
| `repo_watcher.rs` | Active-repo change watcher lifecycle |

## Problem
UI needs grouped terminal actions and refresh signaling without direct dependence on runtime internals.

## Constraints
- Must remain transport-agnostic.
- Must support partial failures for multi-session writes.
- Must avoid UI dependencies.
- Background ownership must remain explicit when startup/session coordination moves out of UI.
- Background autosave scheduling must preserve dedupe and shutdown semantics.
- Local-agent run start must sequence checkpoint capture before command dispatch when run attribution is required.

## Decision
Expose orchestrator APIs as a thin application layer over `state`, `terminal`, and `git` services,
including typed workspace projections, startup coordination, runtime-derived status
reconciliation for UI consumers, and checkpoint-before-dispatch run sequencing for local-agent
starts, while keeping autosave worker state outside presentation code.

## Alternatives Rejected
- Calling terminal and git modules directly from every UI component: rejected due to duplication.
- Introducing IPC transport now: rejected as premature.

## Invariants
- Orchestrator does not depend on UI modules.
- Broadcast operations return per-session result status.
- Event bus payloads remain typed.
- Workspace projections may depend on terminal snapshots, but they remain presentation-agnostic.
- Startup/session helpers may own coordination state, but UI remains responsible for rendering and transient signals.
- Autosave coordination may debounce and defer requests, but persistence payload construction remains typed and deterministic.
- Local-agent run start owns checkpoint capture sequencing; UI callers do not create checkpoints directly.
- Durable receipt status is derived strictly from per-session write outcomes at finalize time.

## Revisit Triggers
- External API/IPC layer is introduced.
- Group orchestration needs persistence-backed jobs.

## Dependencies
**Internal:** `state`, `terminal`, `git`  
**External:** `parking_lot`

## Related ADRs
None.

## Usage Examples
```rust
use crate::orchestrator;

let ids = orchestrator::group_session_ids(app_state.workspace_state(), group_id);
let results = orchestrator::interrupt_sessions(&terminal_manager, &ids);
# let _ = results;
```
