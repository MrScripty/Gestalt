# Gestalt Standards

Project-specific engineering standards for Gestalt (Rust + Dioxus Desktop + PTY terminals).

This file adapts and narrows the shared standards in:

- `/media/jeremy/OrangeCream/Linux Software/Coding-Standards/`

Use this as the default implementation standard for this repository.

## Scope

Applies to:

- Rust source (`src/**/*.rs`)
- Desktop UI behavior (Dioxus)
- Terminal runtime and orchestration APIs
- Tests, docs, and commits

## Stack and Constraints

- Language: Rust (stable)
- UI: Dioxus Desktop
- Terminal backend: `portable_pty` + `vt100`
- Architecture style: single-process desktop app with explicit module boundaries

This is not a web frontend project; web-specific examples in shared standards must be translated to Rust/Desktop equivalents.

## Architecture Rules

Gestalt uses the following practical layers:

1. `ui` (presentation and input handling)
2. `orchestrator` (group-level terminal coordination API)
3. `state` (pure app/session/group models and transitions)
4. `terminal` (PTY lifecycle, IO, snapshots, resize)

Rules:

- `state` must remain framework-independent and side-effect free.
- `terminal` must not depend on `ui`.
- `orchestrator` may depend on `state` and `terminal`, not `ui`.
- `ui` may call `orchestrator` and `state`; keep direct `terminal` usage minimal and migrate through orchestrator where practical.

## File and Module Size

Targets:

- Rust files: <= 500 lines
- CSS files: <= 500 lines

If a file exceeds target:

1. Extract coherent modules/components first (not random splits).
2. Keep each extracted file single-responsibility.
3. Add/update module-level docs for discoverability.

## Source Tree Documentation

When `src/` has 3+ files, include `src/README.md` with:

- purpose of each module
- ownership boundaries
- data-flow overview (`ui -> orchestrator -> terminal/state`)

Update this file when adding/removing major modules.

## Rust Coding Rules

- Run `cargo fmt` before commit.
- Run `cargo clippy --all-targets -- -D warnings` before merge.
- Prefer explicit types at API boundaries.
- Avoid `unwrap`/`expect` in non-test code unless failure is truly unrecoverable and justified.
- Use named constants for non-trivial literals (timing, sizing, limits).
- Keep public functions and types documented with `///` comments.

## Concurrency and Async Rules

- Never hold a `Mutex` lock across `.await`.
- Keep lock scope minimal; lock, do work, unlock.
- Background loops must use named polling intervals/constants.
- Any long-lived async task must have a clear ownership/lifecycle point in `App`.

## Terminal/PTY Rules

- Terminal behavior must be emulator-accurate where possible (PTY resize, vt100 state).
- UI-only visual hacks are acceptable only as temporary fallbacks and must be called out in docs/comments.
- Any API that writes to multiple sessions returns per-session results (no all-or-nothing assumptions).
- Interrupt behavior is standardized on `Ctrl+C` (`0x03`) unless explicitly configurable later.

## Orchestration API Rules

- Group orchestration surface belongs in `src/orchestrator.rs` (or its submodules after refactor).
- Snapshot APIs must include enough state for agents to act safely:
  - terminal identity
  - cwd
  - runtime readiness
  - latest round block
  - selected/focused/activity signals
- Keep orchestration transport-agnostic so IPC/HTTP wrappers can be added without rewriting core logic.

Reference:

- `ORCHESTRATION-API.md`

## Error Handling

- Validate input at boundaries (UI input, file paths, external process operations).
- Return `Result<_, String>` only for internal prototypes; migrate to typed error enums as modules stabilize.
- On partial failure (broadcast/write-many), report per-session outcomes and do not hide failures.

## Security and Safety

- All shell path changes must remain quoted/escaped (`shell_quote` pattern).
- Do not execute arbitrary shell fragments from UI without clear intent.
- Do not introduce network listeners (HTTP/IPC) without explicit auth/trust model documented first.

## Accessibility and UX

- All clickable controls must be keyboard-reachable.
- Keep visible focus indicators for active inputs/panes.
- Preserve copy/paste and text selection workflows in terminal history.
- Keyboard shortcuts must not silently break terminal-native behavior without explicit UX reason.

## Testing Standard for Gestalt

Minimum before merge:

1. `cargo test -q`
2. Add unit tests for non-trivial pure logic:
   - prompt/round parsing
   - orchestrator snapshot derivation
   - state transitions/reordering
3. For UI/input behavior that is hard to unit-test, document manual verification steps in PR/commit notes.

## Tooling and Automation

Recommended local check sequence:

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test -q
```

If pre-commit hooks are introduced, they should run the same sequence (or a fast subset for commit + full checks pre-push).

## Distribution and Installer Standards

- `launcher.sh` is mandatory for developer/operator workflows and must comply with shared `LAUNCHER-STANDARDS.md`.
- `launcher.sh` is not the end-user installer entry point.
- End-user releases must be OS-native installers produced by the bundler, not custom install scripts.
- Required installer outputs:
  - Linux: `.deb`
  - Windows: `.msi`
  - macOS: `.dmg`
- Installer/icon metadata must be maintained in `Dioxus.toml`.
- Shortcut icon source is `assets/Gestalt.png`; platform packaging must use this asset.
- Release artifact naming must follow:
  - `Gestalt-Setup-<version>-x86_64-unknown-linux-gnu.deb`
  - `Gestalt-Setup-<version>-x86_64-pc-windows-msvc.msi`
  - `Gestalt-Setup-<version>-aarch64-apple-darwin.dmg`
- CI must:
  1. Run lint/test quality gates
  2. Build native installers for Linux, Windows, and macOS on `v*` tags
  3. On `v*` tags, create a draft GitHub release with installer artifacts and `checksums-sha256.txt`

## Commit Standard (Gestalt)

Use conventional commits:

- `feat(scope): ...`
- `fix(scope): ...`
- `docs(scope): ...`
- `refactor(scope): ...`
- `test(scope): ...`
- `chore(scope): ...`

Scopes should map to this repo, e.g.:

- `ui`
- `terminal`
- `state`
- `orchestrator`
- `docs`
- `app`

Agent-produced commits include footer:

```text
Agent: codex
```

## Definition of Done

A change is done when:

1. It follows module/layer ownership rules.
2. Formatting/lint/tests pass.
3. Public APIs added/changed are documented.
4. README/docs are updated if behavior or user workflow changed.
5. Commit message and footer follow commit standards.
