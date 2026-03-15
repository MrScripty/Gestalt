# native_terminal

## Purpose
`native_terminal` owns the native-renderer pilot integration for workspace terminal panes. It keeps
the pilot behind a narrow presentation-edge adapter so workspace layout, orchestrator projection,
and terminal runtime ownership remain unchanged.

## Contents
| File | Description |
| ---- | ----------- |
| `mod.rs` | Public pilot gate helpers and native terminal body exports |

## Constraints
- Must remain a presentation/infrastructure seam only.
- Must not become a second owner for PTY/session lifecycle.
- Must keep the legacy `terminal_view` DOM path available as fallback.

## Invariants
- `TerminalManager` remains the runtime owner for pilot panes.
- Workspace and orchestrator modules decide which panes exist and which session is selected.
- Native terminal components consume immutable terminal snapshots and forward input through
  existing UI-side adapters.

## Revisit Triggers
- The pilot requires runtime ownership changes inside `terminal`.
- More than one native terminal body path is needed and the module needs decomposition.
