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
- Insert command mode (`Insert` opens autocomplete, `Enter` inserts prompt text without running it)
- Dockable auxiliary tabs across the run sidebar and right sidebar
- Commands panel for creating/editing/deleting reusable prompt snippets
- Local Agent panel for group orchestration controls
- Contextual Git panel per active path-group (branches, commits, staging, commit/tag/checkout/worktree actions)

## Architecture

- UI: Dioxus Desktop + CSS layout (`src/ui.rs`, `src/ui/`, `src/style/`)
- Session/Group/Command state: `src/state.rs`
- Terminal runtime: PTY + VT100 (`src/terminal.rs`)
- Native spike runtime: feature-gated Alacritty semantics + renderer-facing frame model (`src/terminal_native/`)
- Orchestration scaffolding: `src/orchestrator/`
- Workspace persistence: atomic save/load + schema versioning (`src/persistence/`)

## Insert Commands Workflow

1. Focus any terminal pane.
2. Press `Insert` to enter command mode.
3. Type to filter commands and use arrow keys to change selection.
4. Press `Enter` to paste the selected command prompt into the terminal input line.
5. Press `Esc` or `Insert` to exit command mode without inserting.

## Documentation

- Orchestration API for local agents: [ORCHESTRATION-API.md](ORCHESTRATION-API.md)

## Developer Launcher

`launcher.sh` is the canonical local workflow entry point. It wraps build, test,
perf, and release smoke commands and uses repo-local state dirs by default so
development runs do not pollute the host's normal Gestalt state. Persistent app
launches share one repo-local state directory, while perf/test/smoke actions
use disposable isolated state.

```bash
./launcher.sh --run
```

Common workflows:

- `./launcher.sh --test`
- `./launcher.sh --perf`
- `./launcher.sh --release-smoke`
- `./launcher.sh --run-release`
- `cargo run --features terminal-native-spike --bin terminal_native_spike`

State isolation controls:

- `GESTALT_LAUNCHER_ISOLATE_STATE=1` enables repo-local isolated state dirs
  under `.launcher-state/` (default)
- `GESTALT_LAUNCHER_ISOLATE_STATE=0` uses the host's normal state locations
- `GESTALT_LAUNCHER_STATE_ROOT=/custom/path` overrides the launcher-managed
  state root

If you need the raw Cargo entrypoint instead of the launcher:

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
./launcher.sh --perf
```

If PTY access is unavailable in your environment:

```bash
GESTALT_SKIP_PERF_GATE=1 ./launcher.sh --perf
```

## Current Limitations

- Session status is runtime-activity derived (terminal I/O), not full process/job derived.
- Orchestration API is currently in-process Rust only (no HTTP/IPC transport yet).
- Resume restores groups/sessions/selection and saved terminal snapshots, but live shell processes do not survive machine crash/reboot without an external persistent backend (e.g. tmux).
