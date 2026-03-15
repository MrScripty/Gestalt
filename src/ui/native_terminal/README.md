# native_terminal

## Purpose
`native_terminal` owns the native-renderer pilot integration for workspace terminal panes. It keeps
the pilot behind a narrow presentation-edge adapter so workspace layout, orchestrator projection,
and terminal runtime ownership remain unchanged.

## Contents
| File | Description |
| ---- | ----------- |
| `mod.rs` | Public pilot gate helpers and native terminal body exports |
| `component.rs` | Dioxus terminal body component for the pilot canvas |
| `frame.rs` | `TerminalSnapshot` to renderer-frame adapter |
| `glyph_atlas.rs` | Monospace glyph atlas cache for the pilot renderer |
| `paint.rs` | Custom paint bridge and paint-source lifecycle |
| `renderer.rs` | WGPU pipeline, buffers, and output texture management |
| `scene.rs` | Renderer-facing quad scene construction |

## Constraints
- Must remain a presentation/infrastructure seam only.
- Must not become a second owner for PTY/session lifecycle.
- Must keep the legacy `terminal_view` DOM path available as fallback.
- The first pilot only activates for the selected pane when `GESTALT_NATIVE_TERMINAL_PILOT`
  is enabled under the `native-renderer` feature.
- The first pilot renders the active viewport only and falls back to the legacy path when CRT is
  enabled.

## Invariants
- `TerminalManager` remains the runtime owner for pilot panes.
- Workspace and orchestrator modules decide which panes exist and which session is selected.
- Native terminal components consume immutable terminal snapshots and forward input through
  existing UI-side adapters.

## Usage Examples
```bash
cargo run --features native-renderer --bin gestalt
GESTALT_NATIVE_TERMINAL_PILOT=1 cargo run --features native-renderer --bin gestalt
```

## Validation Notes
- 2026-03-15 compile gates:
  - `cargo check`
  - `cargo check --features native-renderer`
- 2026-03-15 bounded launch smoke:
  - baseline native workspace: `elapsed=10.01 user=0.47 sys=0.17 maxrss=182504`
  - pilot-enabled native workspace: `elapsed=10.01 user=0.46 sys=0.18 maxrss=182752`
- Interpretation: the selected-pane pilot launched successfully and did not materially change
  process memory in a no-interaction 10-second workspace run. This is only a launch-path sanity
  check, not a full interactive performance benchmark.

## Revisit Triggers
- The pilot requires runtime ownership changes inside `terminal`.
- More than one native terminal body path is needed and the module needs decomposition.
