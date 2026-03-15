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
| `frame.rs` | Snapshot/native-frame adapter used by the pilot canvas |
| `glyph_atlas.rs` | Monospace glyph atlas cache for the pilot renderer |
| `paint.rs` | Custom paint bridge and paint-source lifecycle |
| `renderer.rs` | WGPU pipeline, buffers, and output texture management |
| `scene.rs` | Renderer-facing quad scene construction |

## Constraints
- Must remain a presentation/infrastructure seam only.
- Must not become a second owner for PTY/session lifecycle.
- Must keep the legacy `terminal_view` DOM path available as fallback.
- The pilot defaults to the selected pane when `GESTALT_NATIVE_TERMINAL_PILOT`
  is enabled under the `native-renderer` feature, and may be widened to all visible panes with
  `GESTALT_NATIVE_TERMINAL_PILOT_SCOPE=visible`.
- The first pilot renders the active viewport only and falls back to the legacy path when CRT is
  enabled.

## Invariants
- `TerminalManager` remains the app-facing runtime owner for pilot panes.
- Workspace and orchestrator modules decide which panes exist and which session is selected.
- When `GESTALT_NATIVE_TERMINAL_BACKEND=1` is enabled, `TerminalManager` may route the selected
  session through the imported `terminal_native` backend while still publishing compatibility
  snapshots for legacy consumers.
- Native terminal components prefer immutable native frames when available and fall back to
  immutable terminal snapshots otherwise.
- Native pane text entry is captured through a transparent input overlay above the canvas so
  keyboard ownership stays in `terminal_view` instead of moving into the renderer path.
- `glyph_atlas.rs` owns glyph baseline preservation by rasterizing outlined glyphs at their pixel
  bounds inside each tile; `scene.rs` should keep treating glyph quads as full-cell consumers of
  those atlas tiles instead of layering extra vertical offsets on top.

## Usage Examples
```bash
cargo run --features native-renderer --bin gestalt
GESTALT_NATIVE_TERMINAL_PILOT=1 cargo run --features native-renderer --bin gestalt
GESTALT_NATIVE_TERMINAL_PILOT=1 GESTALT_NATIVE_TERMINAL_PILOT_SCOPE=visible \
  cargo run --features native-renderer --bin gestalt
GESTALT_NATIVE_TERMINAL_PILOT=1 GESTALT_NATIVE_TERMINAL_BACKEND=1 \
  cargo run --features native-renderer --bin gestalt
```

## Validation Notes
- 2026-03-15 compile gates:
  - `cargo check`
  - `cargo check --features native-renderer`
- 2026-03-15 backend migration compile gate:
  - `cargo check --features native-renderer`
- 2026-03-15 bounded launch smoke:
  - baseline native workspace: `elapsed=10.01 user=0.47 sys=0.17 maxrss=182504`
  - pilot-enabled native workspace: `elapsed=10.01 user=0.46 sys=0.18 maxrss=182752`
- 2026-03-15 direct native-frame handoff:
  - the pilot canvas now reads `TerminalManager::native_frame_shared(...)` when
    `GESTALT_NATIVE_TERMINAL_BACKEND=1`
  - `cargo test --features native-renderer frame_builds_cells_from_native_frame` is currently
    blocked by unrelated existing test compile failures in `src/pantograph_host.rs` and
    `src/ui/git_commit_graph.rs`
- 2026-03-15 pilot interaction smoke:
  - bounded native run with `GESTALT_NATIVE_TERMINAL_PILOT=1 GESTALT_NATIVE_TERMINAL_BACKEND=1`
    opened the real Gestalt window, accepted an `xdotool`-driven `echo native-pilot-smoke`
    command, and survived two live window resizes before timeout shutdown
- 2026-03-15 visible-pane pilot smoke:
  - bounded native run with
    `GESTALT_NATIVE_TERMINAL_PILOT=1 GESTALT_NATIVE_TERMINAL_PILOT_SCOPE=visible`
    `GESTALT_NATIVE_TERMINAL_BACKEND=1`
    launched successfully and stayed up until timeout with all visible workspace panes routed
    through the native terminal body path
- 2026-03-15 integrated native replay profiling:
  - `rows rebuilt/frame` p95: `42`
  - `cells rebuilt/frame` p95: `5880`
  - `spans published/frame` p95: `1`
  - `cells published/frame` p95: `5880`
  - Interpretation: the current native replay workload behaves like a near-full-frame path, so the
    next performance work should target cheaper near-full publication and scene rebuild behavior
    rather than assuming narrow partial updates.
- 2026-03-15 integrated visible-pane profiling:
  - reduced native-backend sample rendered `3` visible native panes in the active group
  - `native visible render pass` p95: `472 us`
  - `native visible row rebuild` p95: `410 us`
  - `native visible cells rebuilt` p95: `11900`
  - Interpretation: real integrated visible-pane cost is now measurable separately from the legacy
    shell path, and the remaining native work is still dominated by near-full-frame scene rebuilds
    across the visible pane set.
- Interpretation: the selected-pane pilot launched successfully and did not materially change
  process memory in a no-interaction 10-second workspace run. This is only a launch-path sanity
  check, not a full interactive performance benchmark.

## Revisit Triggers
- The pilot requires more than the current backend-routing seam inside `terminal`.
- More than one native terminal body path is needed and the module needs decomposition.
