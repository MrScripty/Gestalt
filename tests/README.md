# tests

## Purpose
Integration tests for persistence and resume behavior.

## Contents
| File | Description |
| ---- | ----------- |
| `persistence_roundtrip.rs` | Save/load roundtrip contract tests |
| `persistence_recovery.rs` | Corrupt-file and backup recovery behavior |
| `emily_inspect_corpus.rs` | Emily inspection snapshot coverage through public read APIs |
| `emily_local_agent_context.rs` | Emily-backed local-agent prompt assembly and display-line logging coverage |
| `emily_local_agent_episode.rs` | Emily local-agent episode recording and seeded EARL gate coverage |
| `resume_startup.rs` | Startup resume flow with workspace state restored but terminal history omitted |
| `emily_seed_corpus.rs` | Emily seed corpus acceptance coverage through the public runtime facade |
| `git_panel_ops.rs` | Git panel orchestration action flow tests against temp repos |
| `git_panel_context_switch.rs` | Repo/non-repo context switching behavior tests |

## Notes
- Tests use `GESTALT_WORKSPACE_PATH` to isolate persistence files under `tmp`.
- Environment variable mutation is guarded by a process-wide mutex to avoid test races.
