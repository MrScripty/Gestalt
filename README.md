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
- Contextual Git side panel per active path-group (branches, commits, staging, commit/tag/checkout/worktree actions)

## Architecture

- UI: Dioxus Desktop + CSS layout (`src/ui.rs`, `src/ui/`, `src/style/`)
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

## End-User Installers

`launcher.sh` is for local development workflows. End users should use release
installer artifacts from GitHub Releases (native OS installers):

- `Gestalt-Setup-<version>-x86_64-unknown-linux-gnu.deb`
- `Gestalt-Setup-<version>-x86_64-pc-windows-msvc.msi`
- `Gestalt-Setup-<version>-aarch64-apple-darwin.dmg`

Shortcuts/menu entries are created by each OS-native installer:

- Linux (`.deb`): desktop launcher in the system app menu
- Windows (`.msi`): Start Menu shortcut and standard Add/Remove Programs entry
- macOS (`.dmg`): drag-install app bundle into Applications

Installer/icon metadata is configured via `Dioxus.toml` using
`assets/Gestalt_small.png`.

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
