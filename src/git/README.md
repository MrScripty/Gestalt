# git

## Purpose
`git` encapsulates command execution, parsing, typed errors, and repository snapshot models for all Git operations.

## Contents
| File/Folder | Description |
| ----------- | ----------- |
| `mod.rs` | Public Git service API |
| `command.rs` | `git` process invocation wrapper |
| `parse.rs` | Parsing for log/status/branches/tags output |
| `model.rs` | DTOs for repo snapshots and operations |
| `error.rs` | Typed Git error definitions |

## Problem
The app needs consistent Git behavior across UI features without scattering subprocess and parser logic.

## Constraints
- Must return typed failures.
- Must support non-repo paths gracefully.
- Must keep parser behavior deterministic under varied output.

## Decision
Centralize Git access in one module and expose structured results to orchestrator/UI.

## Alternatives Rejected
- Running Git directly from UI modules: rejected due to layering drift.
- Shell-script wrappers: rejected due to weaker typing and testability.

## Invariants
- All Git subprocesses run through `run_git`.
- Public APIs return `GitError` on failure.
- Parsing is isolated from UI formatting concerns.

## Revisit Triggers
- Need asynchronous streaming Git commands.
- Need multi-repo batch operations.

## Dependencies
**Internal:** `orchestrator`  
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
