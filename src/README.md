# src

## Purpose
Application source modules for Gestalt's state model, command library, terminal runtime, orchestration, Git domain, UI, and persistence.

## Contents
| File/Folder | Description |
| ----------- | ----------- |
| `lib.rs` | Library module exports for integration tests and reuse |
| `main.rs` | Program entry point and module wiring |
| `commands/` | Insert-command models, matching, and validation helpers |
| `emily_bridge.rs` | Gestalt adapter for Emily memory ingest/query APIs |
| `local_restore.rs` | SQLite-backed restore projection for terminal UI/runtime metadata |
| `orchestration_log/` | Durable SQLite command/event/receipt timelines for orchestrated actions |
| `state/` | Core workspace, knowledge, and command state models plus transitions |
| `terminal.rs` | PTY lifecycle, input/output, and snapshots |
| `orchestrator/` | Group-level terminal orchestration helpers |
| `git/` | Git repository query/mutation service layer used by contextual UI controls |
| `ui.rs` + `ui/` | Dioxus desktop presentation, interaction handling, and autosave workflow |
| `style/` | UI styling split by layout concerns |
| `persistence/` | Workspace load/save infrastructure |

## Design Decisions
- `state` stays framework-agnostic and serializable, with workspace, knowledge,
  and command domains split behind an aggregate facade for persistence and callers.
- `terminal` owns live runtime processes and exposes snapshots.
- `terminal` should avoid redundant scrollback cloning on hot PTY read and resize paths; snapshot rebuild work should use locked line views where possible before publishing a new immutable snapshot.
- `emily_bridge` adapts terminal line events into Emily generic text objects.
- `orchestration_log` persists exact command lifecycles using timeline sequence plus timestamps.
- `emily_bridge` can inject an optional Emily embedding provider at worker startup and exposes vectorization control commands.
- Pantograph embedding bootstrap is deferred so provider validation does not block initial UI interaction.
- Terminal history source-of-truth is Emily; workspace persistence stores terminal projection metadata only.
- `local_restore` persists lightweight terminal projection state for startup fidelity.
- `persistence` is isolated infrastructure with a versioned schema.

## Dependencies
**Internal:** `commands`, `state`, `terminal`, `emily_bridge`, `orchestrator`, `orchestration_log`, `git`, `persistence`  
**External:** `dioxus`, `portable-pty`, `vt100`, `serde`, `emily`, `rusqlite`
