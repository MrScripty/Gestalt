# tests

## Purpose
Integration tests for persistence and resume behavior.

## Contents
| File | Description |
| ---- | ----------- |
| `persistence_roundtrip.rs` | Save/load roundtrip contract tests |
| `persistence_recovery.rs` | Corrupt-file and backup recovery behavior |
| `resume_startup.rs` | Startup resume flow with restored terminal history |

## Notes
- Tests use `GESTALT_WORKSPACE_PATH` to isolate persistence files under `tmp`.
- Environment variable mutation is guarded by a process-wide mutex to avoid test races.

