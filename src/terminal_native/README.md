# terminal_native

## Purpose
`terminal_native` contains the feature-gated terminal core and runtime seam for
the Alacritty semantics + native GPU rendering spike.

## Contents
| File/Folder | Description |
| ----------- | ----------- |
| `mod.rs` | Public module exports for the native terminal spike |
| `demo.rs` | Presentation-layer terminal demo component for the standalone spike |
| `app.rs` | Composition root that assembles the session, controller, and presentation |
| `controller.rs` | Runtime-owned command surface for PTY input, resize, and frame access |
| `constants.rs` | Spike-local UI, session, and render constants grouped by concern |
| `model.rs` | Renderer-facing terminal frame, cursor, cell, and damage types |
| `input.rs` | Keyboard and text-input translation into PTY byte sequences |
| `paint.rs` | Native GPU custom paint source and texture lifecycle management |
| `glyph_atlas.rs` | Bundled monospace glyph atlas cache used by the GPU renderer |
| `gpu_scene.rs` | Renderer-facing quad instance generation from immutable terminal frames |
| `gpu_renderer.rs` | WGPU terminal renderer that composes backgrounds, glyphs, and cursor quads |
| `raster.rs` | Legacy CPU raster path kept for fallback benchmarking during the spike |
| `emulator.rs` | Alacritty-backed terminal emulator adapter that projects frames into the local model |
| `session.rs` | Single-session PTY runtime that feeds emulator frames for the spike renderer |

## Problem
The current terminal UI path rebuilds line strings and Dioxus nodes, which is
too expensive for a high-frequency terminal surface.

## Constraints
- Must remain feature-gated so the production Dioxus Desktop path is unchanged.
- Must preserve a single owner for PTY lifecycle, emulator state, and frame publication.
- Must expose a renderer-facing model without leaking the rest of the app's
  terminal snapshot assumptions into the spike path.

## Decision
Keep the spike behind a dedicated `terminal-native-spike` feature and project
Alacritty terminal state into a narrow local frame model suitable for a native
renderer.

## Layering
- `app.rs` is the composition root for the spike path.
- `controller.rs` is the application/controller seam and owns runtime commands.
- `demo.rs` is presentation only and forwards user events to the controller.
- `paint.rs` and `session.rs` are infrastructure concerns.
- `paint.rs` reports surface-size changes through the controller instead of
  mutating PTY state directly.
- `model.rs` is the renderer-facing contract between layers.

## Alternatives Rejected
- Replacing the production `terminal` module in-place: rejected because the
  spike needs a contained risk boundary.
- Passing `alacritty_terminal` grid types directly into future renderer code:
  rejected to keep the renderer seam locally owned.

## Invariants
- The composition root assembles long-lived runtime resources near the binary entrypoint.
- The controller owns PTY lifecycle, resize, and input dispatch.
- The emulator owns terminal semantics and damage tracking.
- The emulator maintains the mutable projected-cell cache and is the only layer allowed to mutate projected terminal cells.
- The emulator publishes immutable frames as either a full cell snapshot or a partial row-span update set.
- Full publication is required on resize, full damage, or display-offset changes.
- The GPU renderer owns atlas textures, pipelines, and output texture lifecycle.
- Panes in the same native window share one GPU renderer core and one glyph atlas cache instead of constructing separate atlas textures and pipelines per pane.
- Hidden panes keep their PTY sessions running, but the standalone spike unmounts their native paint surfaces until they become visible again.
- Renderer-side caches may retain prior immutable state, but they only apply published changes and never mutate emulator-owned state.

## Revisit Triggers
- The spike needs multi-pane lifecycle management.
- The renderer requires a richer frame model for selection, hyperlinks, or IME.

## Dependencies
**Internal:** none  
**External:** `ab_glyph`, `alacritty_terminal`, `dioxus-native`, `font8x8`, `portable-pty`, `parking_lot`, `wgpu`

## Related ADRs
None.

## Usage Examples
```bash
cargo run --features terminal-native-spike --bin terminal_native_spike
```

## Current Constraints
- The hot render path now uses a bundled DejaVu Sans Mono glyph atlas, but it
  still assumes fixed terminal cells rather than full text shaping.
- The current shared glyph atlas assumes panes in the same window converge on
  the same fixed cell metrics; materially different pane cell sizes would
  thrash the shared atlas and need a keyed atlas pool instead.
- The standalone spike now schedules visibility at the pane level: only the
  selected pane keeps a live canvas mounted, while background panes show a
  summary card and resume rendering when re-selected.
- Selection, mouse reporting, clipboard integration, and IME are intentionally
  out of scope for this spike.
- Keyboard capture still relies on an invisible full-surface input overlay
  because direct terminal-surface focus handling in `dioxus-native` is not yet
  reliable enough for this spike.
- Binary build verification is complete, focused GPU-scene tests pass, and the
  replay benchmark now breaks native snapshot cost into damage collection,
  projection update, and publication build in addition to GPU scene
  preparation.
- Current profiling now shows native snapshot build below 1 ms p95 on the
  reduced replay workload; render preparation is the larger remaining native
  cost after the latest projection and scene-cache reductions.
- A true interactive visual/manual validation run still needs a local desktop
  session.
