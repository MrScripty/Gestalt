# state

## Purpose
`state` owns Gestalt's durable, framework-independent workspace, knowledge, and
insert-command models plus the pure transition rules that mutate them.

## Contents
| File/Folder | Description |
| ----------- | ----------- |
| `mod.rs` | Shared state types, the aggregate `AppState`, and persistence-facing compatibility surface |
| `workspace.rs` | Path groups, sessions, selection, layout, and workspace repair rules |
| `knowledge.rs` | Notes, snippets, embedding metadata, and knowledge repair rules without UI navigation state |
| `commands.rs` | Durable insert-command state facade over `CommandLibrary` |
| `tests.rs` | Unit tests for state transitions, restore compatibility, and domain invariants |

## Problem
Gestalt needs one durable model layer that can be persisted, restored, projected
by orchestrator code, and consumed by UI without letting terminal/runtime
infrastructure leak into business state.

## Constraints
- Must remain side-effect free and independent of Dioxus/runtime wiring.
- Must preserve persisted workspace compatibility across refactors.
- Must provide stable IDs and repair invalid restored relationships.
- Must not own PTY lifecycle, polling loops, or async coordination.

## Decision
Split the former monolithic state module into workspace, knowledge, and command
domains while keeping `AppState` as the aggregate persistence and compatibility
facade. Cross-domain workflows remain explicit in `AppState`; domain-local rules
live in the owning module, and purely transient note selection stays in UI.

## Alternatives Rejected
- Keep one flat `state.rs`: rejected because the module had grown beyond review
  and decomposition thresholds.
- Move durable note/session selection into UI: rejected for `selected_session`
  because startup and orchestrator logic depend on it.

## Invariants
- Workspace state remains serializable and repairable from persisted JSON.
- Knowledge objects never own runtime resources, UI handles, or editor selection state.
- Command CRUD remains durable and restore-safe.
- `AppState` revision tracks durable mutations only.

## Revisit Triggers
- Another durable domain emerges that does not fit workspace, knowledge, or commands.
- Persisted schema changes require versioned migration beyond `serde(flatten)` compatibility.
- UI can safely consume domain-specific signals without an aggregate facade.

## Dependencies
**Internal:** `commands` for `CommandLibrary` and `InsertCommand`  
**External:** `serde`

## Related ADRs
- None identified as of 2026-03-07.
- Reason: The refactor stays within the established module layering and persistence model.
- Revisit trigger: State leaves a single-process in-memory/persisted boundary or gains external consumers.

## Usage Examples
```rust
use crate::state::AppState;

let mut state = AppState::default();
let group_id = state.groups()[0].id;
let session_id = state.add_session(group_id);
state.select_session(session_id);
```

## API Consumer Contract
- Consumers read durable workspace, knowledge, and command data through `AppState`
  facade methods or domain references.
- Mutations are synchronous and side-effect free.
- Restore callers must use `into_restored` before treating loaded state as valid.
- Unknown persisted fields are tolerated unless schema migration explicitly changes that policy.

## Structured Producer Contract
- Persisted workspace JSON keeps the same top-level field shape through flattened
  domain structs unless a versioned migration says otherwise.
- Missing layout/UI scale/note/snippet ID fields fall back to documented defaults.
- Group/session ordering is stable and preserved for consumers that rely on insertion order.
- When the persisted contract changes, `persistence::migrate` must own upgrade behavior.
