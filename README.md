# Gestalt

Rust desktop control surface for large terminal fleets (40+ coding agents).

## Why Dioxus 0.7.x
- Rust-first desktop app model with a fast iteration loop.
- Reactive state + HTML/CSS UI makes dense tabbing/tiling workflows straightforward.
- Good fit for rapidly iterating on operator UX while terminal backend matures.

## Current MVP
- Vertical tab rail with color-coded status indicators (`Idle`, `Busy`, `Error`).
- Path-based tab groups: each group is a filesystem path.
- New path groups default to three real PTY-backed shells in that path:
  - `Agent A` (stacked center/top)
  - `Agent B` (stacked center/bottom)
  - `Run / Compile` (dedicated blue right sidebar)
- Drag tabs to reorder, including moving tabs across path groups.
- When a tab is moved to another group, it receives `cd <group-path>`.
- Tabs can be renamed inline from the left rail.
- Interactive terminal panes: click a pane and type directly; key events are forwarded to PTY.
- VT100 parsing for terminal output, so cursor movement and ANSI control sequences render properly.

## Run
```bash
cargo run
```

## Notes
- Shell output is live from PTY processes.
- The UI forwards raw key input for interactive CLI workflows.
- Busy/idle/error status is still user-driven for now.

## Next Milestones
1. Full keystroke terminal emulation and cursor handling in each pane.
2. Busy detection based on process state and prompt detection.
3. Persist workspace state (paths, sessions, layout, status).
4. Keyboard-first control for focus, move, and command dispatch.
