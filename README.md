# Gestalt

Gestalt is a Rust desktop workspace for running and coordinating large terminal fleets (40+ coding agents) in one UI.

It is built with Dioxus Desktop (`0.7.x`) and uses real PTY sessions + VT100 parsing so each pane behaves like an actual terminal, not a fake log window.

## What It Does

- Vertical tab rail with color-coded status (`Idle`, `Busy`, `Error`)
- Path-based groups (`/some/project/path` = one workspace group)
- Default 3-pane layout per group:
  - `Agent A` and `Agent B` stacked in the center
  - `Run / Compile` in a dedicated blue right sidebar pane
- Drag-and-drop tab organization across groups
- Real interactive terminals (PTY-backed)
- Inline tab renaming
- Round-aware selection helpers (`Ctrl+A` for command/output block selection)
- Local Agent pane for group orchestration controls

## Architecture

- UI: Dioxus Desktop + CSS layout (`src/ui.rs`, `src/style.css`)
- Session/Group state: `src/state.rs`
- Terminal runtime: PTY + VT100 (`src/terminal.rs`)
- Orchestration scaffolding: `src/orchestrator.rs`
- Workspace persistence: atomic save/load + schema versioning (`src/persistence/`)

## Documentation

- Orchestration API for local agents: [ORCHESTRATION-API.md](ORCHESTRATION-API.md)

## Run

```bash
cargo run
```

## Performance Gate

Run the terminal latency regression gate (PTY required):

```bash
scripts/perf-gate.sh
```

If PTY access is unavailable in your environment:

```bash
GESTALT_SKIP_PERF_GATE=1 scripts/perf-gate.sh
```

## Current Limitations

- Session status is still operator-updated (not process-derived yet).
- Orchestration API is currently in-process Rust only (no HTTP/IPC transport yet).
- Resume restores groups/sessions/selection and saved terminal snapshots, but live shell processes do not survive machine crash/reboot without an external persistent backend (e.g. tmux).
