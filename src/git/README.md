# git

## Purpose
`git` encapsulates command execution, parsing, typed errors, repository snapshot models, and file-state hashing helpers for all Git operations.

## Contents
| File/Folder | Description |
| ----------- | ----------- |
| `mod.rs` | Public Git service API |
| `command.rs` | `git` process invocation wrapper |
| `parse.rs` | Parsing for log/status/branches/tags output |
| `model.rs` | DTOs for repo snapshots, operations, and file-state hashes |
| `error.rs` | Typed Git error definitions |

## Problem
The app needs consistent Git behavior across UI features and run-review infrastructure without scattering subprocess and parser logic.

## Constraints
- Must return typed failures.
- Must support non-repo paths gracefully.
- Must keep parser behavior deterministic under varied output.
- Must expose stable file-state reads for checkpoint diff derivation without leaking subprocess logic upward.

## Decision
Centralize Git access in one module and expose structured results to orchestrator/UI and run-review services.

## Alternatives Rejected
- Running Git directly from UI modules: rejected due to layering drift.
- Shell-script wrappers: rejected due to weaker typing and testability.

## Invariants
- All Git subprocesses run through `run_git`.
- Public APIs return `GitError` on failure.
- Parsing is isolated from UI formatting concerns.
- File-state snapshots used by run checkpoints are read through this module, not by direct subprocess calls elsewhere.

## Revisit Triggers
- Need asynchronous streaming Git commands.
- Need multi-repo batch operations.

## Dependencies
**Internal:** `run_checkpoints`  
**External:** `thiserror`

## Related ADRs
None.

## Usage Examples
```rust
use crate::git;

let context = git::load_repo_context(".", git::DEFAULT_COMMIT_LIMIT)?;
# let _ = context;
# Ok::<(), crate::git::GitError>(())
```
