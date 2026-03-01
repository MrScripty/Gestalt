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
| `state.rs` | Core workspace/group/session model and transitions |
| `terminal.rs` | PTY lifecycle, input/output, and snapshots |
| `orchestrator/` | Group-level terminal orchestration helpers |
| `git/` | Git repository query/mutation service layer used by contextual UI controls |
| `ui.rs` + `ui/` | Dioxus desktop presentation, interaction handling, and autosave workflow |
| `style/` | UI styling split by layout concerns |
| `persistence/` | Workspace load/save infrastructure |

## Design Decisions
- `state` stays framework-agnostic and serializable, including command library persistence.
- `terminal` owns live runtime processes and exposes snapshots.
- `emily_bridge` adapts terminal line events into Emily generic text objects.
- `local_restore` persists lightweight terminal projection state for startup fidelity.
- `persistence` is isolated infrastructure with a versioned schema.

## Dependencies
**Internal:** `commands`, `state`, `terminal`, `emily_bridge`, `orchestrator`, `git`, `persistence`  
**External:** `dioxus`, `portable-pty`, `vt100`, `serde`, `emily`, `rusqlite`
