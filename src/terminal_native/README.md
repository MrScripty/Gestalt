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
| `raster.rs` | Damage-aware CPU rasterization used by the GPU paint source |
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
- The raster and paint stages own pixel generation and GPU texture lifecycle.
- The published frame is immutable to consumers and replaced atomically.

## Revisit Triggers
- The spike needs multi-pane lifecycle management.
- The renderer requires a richer frame model for selection, hyperlinks, or IME.

## Dependencies
**Internal:** none  
**External:** `alacritty_terminal`, `dioxus-native`, `font8x8`, `portable-pty`, `parking_lot`, `wgpu`

## Related ADRs
None.

## Usage Examples
```bash
cargo run --features terminal-native-spike --bin terminal_native_spike
```

## Current Constraints
- Glyph rasterization currently uses `font8x8`, so complex Unicode shaping is
  not yet covered.
- Selection, mouse reporting, clipboard integration, and IME are intentionally
  out of scope for this spike.
- Binary build verification is complete and an X11 smoke pass covered launch,
  typed input, and resize.
- A true interactive visual/manual validation run still needs a local desktop
  session.
