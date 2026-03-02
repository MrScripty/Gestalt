# Standards Remediation Closeout (2026-03-02)

## Scope
This closeout records remediation work executed against:
- `GESTALT-STANDARDS.md`
- `/media/jeremy/OrangeCream/Linux Software/Coding-Standards/`

## Completed Changes
- Cleared `cargo clippy --all-targets -- -D warnings` blockers in terminal and tests.
- Added source-directory README coverage for:
  - `src/commands`, `src/git`, `src/orchestrator`
  - `src/ui`, `src/style`, `src/resource_monitor`, `src/bin`
  - `emily/src`, `emily/src/store`
- Added Git path-mark service API in `git` + orchestrator boundary.
- Removed direct `git` subprocess usage from `ui/file_browser_panel.rs`.
- Added orchestrator boundary validation for new worktree paths.
- Added edge tests for repository path marks and validation behavior.
- Extracted file-browser scan logic into `ui/file_browser_scan.rs`.
- Shifted file-browser refresh to nonce-driven triggers with low-frequency fallback.
- Documented remaining polling exceptions and revisit triggers in `src/ui/README.md`.
- Migrated command validation and local restore APIs to typed errors.
- Migrated terminal runtime API to typed errors with UI/orchestrator string mapping at boundaries.

## Verification
The following checks passed after remediation:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test -q
```

## Remaining Follow-ups
- Several files remain above the 500-line target and should be decomposed further in later slices.
- Existing local user edits in style/UI files were intentionally not rewritten by this remediation pass.

## Accepted Exceptions
- Polling loops documented in `src/ui/README.md` remain until event-driven hooks are available at the runtime/UI boundary.
