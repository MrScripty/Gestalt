# src

## Purpose
Application source modules for Gestalt's state model, terminal runtime, orchestration, UI, and persistence.

## Contents
| File/Folder | Description |
| ----------- | ----------- |
| `lib.rs` | Library module exports for integration tests and reuse |
| `main.rs` | Program entry point and module wiring |
| `state.rs` | Core workspace/group/session model and transitions |
| `terminal.rs` | PTY lifecycle, input/output, and snapshots |
| `orchestrator.rs` | Group-level terminal orchestration helpers |
| `ui.rs` | Dioxus desktop presentation and event wiring |
| `style.css` | UI styling |
| `persistence/` | Workspace load/save infrastructure |

## Design Decisions
- `state` stays framework-agnostic and serializable.
- `terminal` owns live runtime processes and exposes snapshots.
- `persistence` is isolated infrastructure with a versioned schema.

## Dependencies
**Internal:** `state`, `terminal`, `orchestrator`, `persistence`  
**External:** `dioxus`, `portable-pty`, `vt100`, `serde`
