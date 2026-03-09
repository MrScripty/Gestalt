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
| `emily_local_agent_membrane.rs` | Local-agent membrane adoption coverage through the Emily bridge, including remote and fallback cases |
| `emily_membrane_dev.rs` | Dev-only membrane execution flow coverage through the Gestalt host helper |
| `emily_semantic_retrieval.rs` | Bridge-level semantic retrieval and lexical fallback coverage against seeded Emily corpora |
| `pantograph_host_reasoning.rs` | Pantograph reasoning config and provider-registry mapping coverage through Gestalt's host adapter |
| `resume_startup.rs` | Startup resume flow with workspace state restored but terminal history omitted |
| `emily_seed_corpus.rs` | Emily seed corpus acceptance coverage through the public runtime facade |
| `git_panel_ops.rs` | Git panel orchestration action flow tests against temp repos |
| `git_panel_context_switch.rs` | Repo/non-repo context switching behavior tests |

## Notes
- Tests use `GESTALT_WORKSPACE_PATH` to isolate persistence files under `tmp`.
- Environment variable mutation is guarded by a process-wide mutex to avoid test races.
- Emily retrieval tests seed deterministic corpora first, then verify host-side queries only through public Emily runtime or bridge APIs.
- Local-agent membrane tests verify local-only adoption, remote route/remote episode/validation persistence, and timeout-style local fallback through the bridge-backed membrane path instead of opening a second Emily runtime in the host flow.
- Pantograph host tests set workflow env vars explicitly so reasoning workflow ids and node bindings remain host-owned configuration, not reusable Emily crate contracts.
