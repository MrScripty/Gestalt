# src

## Purpose
Application source modules for Gestalt's state model, command library, terminal runtime, orchestration, Git domain, UI, and persistence.

## Contents
| File/Folder | Description |
| ----------- | ----------- |
| `lib.rs` | Library module exports for integration tests and reuse |
| `main.rs` | Program entry point and module wiring |
| `commands/` | Insert-command models, matching, and validation helpers |
| `emily_bridge.rs` | Gestalt adapter for Emily memory ingest/query APIs |
| `emily_inspect/` | Deterministic Emily inspection helpers for host-side diagnostics |
| `emily_membrane_dev.rs` | Dev-only membrane execution helpers for controlled Gestalt host flows |
| `emily_seed/` | Deterministic Emily seed corpus helpers for diagnostics and acceptance tests |
| `local_agent_context.rs` | Host-side Emily-backed prompt assembly for the local-agent flow |
| `local_agent_episode.rs` | Host-side Emily episode recording and gate interpretation for local-agent runs |
| `local_agent_membrane.rs` | Host-side membrane execution helper for the local-agent flow, with gated remote support |
| `pantograph_host.rs` | Gestalt-owned Pantograph composition for Emily embedding and membrane reasoning providers |
| `local_restore.rs` | SQLite-backed restore projection for terminal UI/runtime metadata |
| `orchestration_log/` | Durable SQLite command/event/receipt timelines for orchestrated actions |
| `run_checkpoints/` | Durable Git-backed run baselines and derived review diffs |
| `state/` | Core workspace, knowledge, and command state models plus transitions |
| `terminal.rs` | PTY lifecycle, input/output, and snapshots |
| `orchestrator/` | Group-level terminal orchestration helpers |
| `git/` | Git repository query/mutation service layer used by contextual UI controls |
| `ui.rs` + `ui/` | Dioxus desktop presentation, interaction handling, and autosave workflow |
| `style/` | UI styling split by layout concerns |
| `persistence/` | Workspace load/save infrastructure |

## Design Decisions
- `state` stays framework-agnostic and serializable, with workspace, knowledge,
  and command domains split behind an aggregate facade for persistence and callers.
- `terminal` owns live runtime processes and exposes snapshots.
- `terminal` should avoid redundant scrollback cloning on hot PTY read and resize paths; snapshot rebuild work should use locked line views where possible before publishing a new immutable snapshot.
- `emily_bridge` adapts terminal line events into Emily generic text objects.
- `emily_inspect` gathers deterministic host-side snapshots from Emily public reads for debug loops.
- `emily_membrane_dev` hosts development-only membrane runs against isolated Emily databases and inspects the resulting sovereign artifacts.
- `emily_seed` owns synthetic host-side Emily fixture datasets, including semantic retrieval corpora, and seeds them only through Emily public facades.
- `local_agent_context` keeps Emily-backed local-agent prompt assembly out of the Dioxus component layer and preserves the human-entered command separately from the dispatched prompt payload.
- `local_agent_episode` records real local-agent actions as Emily episodes and interprets episode plus EARL state into a narrow host-facing gate.
- `local_agent_membrane` keeps membrane execution out of the Dioxus component layer, reuses the existing Emily bridge instead of opening a second runtime, keeps local-only adoption behind `GESTALT_ENABLE_LOCAL_AGENT_MEMBRANE=1`, and gates the first Pantograph-backed remote path behind `GESTALT_ENABLE_LOCAL_AGENT_REMOTE_MEMBRANE=1` with explicit local-only fallback.
- `pantograph_host` keeps workflow ids, node bindings, provider-registry composition, and embedding vectorization defaults inside Gestalt so reusable Emily crates stay host-agnostic.
- `pantograph_host` also owns Pantograph-specific embedding workflow maintenance and validation helpers, including graph-edit updates for the `puma-lib` node, the measured `Qwen3-Embedding-4B-GGUF` default of `2560` dimensions, and warm-session reuse diagnostics for the session-backed embedding provider.
- `emily_bridge` surfaces bridge worker failures as request errors and keeps recent-history reads failure-tolerant by degrading to an empty chunk instead of panicking.
- `orchestration_log` persists exact command lifecycles using timeline sequence plus timestamps.
- `run_checkpoints` persists coarse repo baselines so run review can compare current Git state against the moment a run started.
- `emily_bridge` can inject an optional Emily embedding provider at worker startup and exposes vectorization control commands.
- Pantograph embedding bootstrap is deferred so provider validation does not block initial UI interaction, but successful bootstrap now applies host-owned vectorization defaults immediately so retrieval can become semantic without leaking Pantograph config into Emily crates.
- Terminal history source-of-truth is Emily; workspace persistence stores terminal projection metadata only.
- `local_restore` persists lightweight terminal projection state for startup fidelity.
- `persistence` is isolated infrastructure with a versioned schema.

## Dependencies
**Internal:** `commands`, `state`, `terminal`, `emily_bridge`, `emily_inspect`, `emily_membrane_dev`, `emily_seed`, `local_agent_context`, `local_agent_episode`, `local_agent_membrane`, `pantograph_host`, `orchestrator`, `orchestration_log`, `run_checkpoints`, `git`, `persistence`  
**External:** `dioxus`, `portable-pty`, `vt100`, `serde`, `emily`, `emily-membrane`, `rusqlite`
