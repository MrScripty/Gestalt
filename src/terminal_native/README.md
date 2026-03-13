# terminal_native

## Purpose
`terminal_native` contains the feature-gated terminal core and runtime seam for
the Alacritty semantics + native GPU rendering spike.

## Contents
| File/Folder | Description |
| ----------- | ----------- |
| `mod.rs` | Public module exports for the native terminal spike |
| `model.rs` | Renderer-facing terminal frame, cursor, cell, and damage types |
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

## Alternatives Rejected
- Replacing the production `terminal` module in-place: rejected because the
  spike needs a contained risk boundary.
- Passing `alacritty_terminal` grid types directly into future renderer code:
  rejected to keep the renderer seam locally owned.

## Invariants
- The PTY runtime owns process lifecycle and writer access.
- The emulator owns terminal semantics and damage tracking.
- The published frame is immutable to consumers and replaced atomically.

## Revisit Triggers
- The spike needs multi-pane lifecycle management.
- The renderer requires a richer frame model for selection, hyperlinks, or IME.

## Dependencies
**Internal:** none  
**External:** `alacritty_terminal`, `portable-pty`, `parking_lot`

## Related ADRs
None.
