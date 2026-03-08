# run_checkpoints

## Purpose
`run_checkpoints` persists lightweight Git-backed baselines for orchestrated runs so Gestalt can answer "what changed since this run started?" without adopting full event sourcing.

## Contents
| File/Folder | Description |
| ----------- | ----------- |
| `mod.rs` | Public checkpoint capture and run-review API |
| `model.rs` | Baseline, stored checkpoint, and review DTOs |
| `store.rs` | SQLite-backed checkpoint persistence and retention |
| `error.rs` | Typed checkpoint and review errors |

## Problem
Gestalt can show current Git status and orchestration command history, but it cannot attribute current repo changes to the start of a specific orchestrated run.

## Constraints
- Must use Git as the source of truth for repo state.
- Must remain restart-safe.
- Must not introduce UI dependencies.
- Must distinguish pre-existing dirty files from run-era changes.

## Decision
Store one lightweight checkpoint per run with pre-run dirty-file metadata and derive reviews by comparing the stored baseline against current Git state on demand.

## Alternatives Rejected
- Reusing orchestration receipts for repo baselines: rejected because command lifecycle history and repo-state checkpoints have different retention and query patterns.
- Full event sourcing for filesystem changes: rejected as too large for the initial capability.

## Invariants
- Checkpoint capture happens from Git-backed repo state only.
- Baseline files store both status metadata and blob hashes needed to detect further edits on already-dirty files.
- Review derivation compares current repo state against the stored baseline instead of against `HEAD` alone.

## Revisit Triggers
- Run review needs rollback or patch application.
- UI needs browsing across many historical checkpoints instead of latest-per-group.
- Checkpoint storage needs to merge with a broader persisted run domain.

## Dependencies
**Internal:** `git`, `state`  
**External:** `rusqlite`, `serde`, `serde_json`, `thiserror`, `uuid`

## Related ADRs
- None identified as of 2026-03-08.
- Reason: This is a focused infrastructure addition within existing repo/orchestrator boundaries.
- Revisit trigger: Checkpoints expand into a broader persisted run model or rollback system.

## Usage Examples
```rust
use crate::run_checkpoints;

let checkpoint = run_checkpoints::capture_run_checkpoint(7, "/tmp/repo", "cargo check")?;
let review = run_checkpoints::load_latest_run_review_for_group_path("/tmp/repo")?;
# let _ = (checkpoint, review);
# Ok::<(), crate::run_checkpoints::RunCheckpointError>(())
```

## API Consumer Contract
- `capture_run_checkpoint` returns `Ok(None)` for non-repo paths and a stored checkpoint for Git-backed paths.
- `load_latest_run_review_for_group_path` returns the latest checkpoint-derived review for the provided group path or `Ok(None)` when no checkpoint exists.
- Callers must treat review loading as I/O and run it off UI-sensitive blocking paths.
- Checkpoint capture errors are fatal for workflows that require run attribution.

## Structured Producer Contract
- Stored checkpoint rows contain stable fields for run identity, group identity, repo identity, and baseline file metadata.
- `baseline_files` is a JSON array of `RunCheckpointFile` values; omitted blob hashes mean the worktree or index had no content snapshot for that path at capture time.
- Retention is bounded per `group_path`; older rows may be pruned after successful inserts.
- Review DTOs are derived, not persisted, and may be regenerated from the latest stored checkpoint plus current Git state.
