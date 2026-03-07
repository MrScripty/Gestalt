# orchestrator

## Purpose
`orchestrator` coordinates cross-session operations and emits orchestration events for UI refresh and control flows.

## Contents
| File/Folder | Description |
| ----------- | ----------- |
| `mod.rs` | Module exports and orchestration surface |
| `runtime.rs` | Group snapshot derivation and broadcast terminal actions |
| `workspace.rs` | Active-workspace projection and session-status reconciliation |
| `git.rs` | Orchestrator-facing Git operation wrappers |
| `events.rs` | In-process event bus and event contracts |
| `repo_watcher.rs` | Active-repo change watcher lifecycle |

## Problem
UI needs grouped terminal actions and refresh signaling without direct dependence on runtime internals.

## Constraints
- Must remain transport-agnostic.
- Must support partial failures for multi-session writes.
- Must avoid UI dependencies.

## Decision
Expose orchestrator APIs as a thin application layer over `state`, `terminal`, and `git` services,
including typed workspace projections and runtime-derived status reconciliation for UI consumers.

## Alternatives Rejected
- Calling terminal and git modules directly from every UI component: rejected due to duplication.
- Introducing IPC transport now: rejected as premature.

## Invariants
- Orchestrator does not depend on UI modules.
- Broadcast operations return per-session result status.
- Event bus payloads remain typed.
- Workspace projections may depend on terminal snapshots, but they remain presentation-agnostic.

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

let ids = orchestrator::group_session_ids(&app_state, group_id);
let results = orchestrator::interrupt_sessions(&terminal_manager, &ids);
# let _ = results;
```
