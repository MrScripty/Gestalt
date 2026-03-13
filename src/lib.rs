/// Insert-command models, matching, and validation helpers.
pub mod commands;
/// Emily integration bridge for ingest/query APIs.
pub mod emily_bridge;
/// Deterministic Emily inspection helpers for host-side diagnostics.
pub mod emily_inspect;
/// Dev-only membrane execution helpers for controlled Gestalt host flows.
pub mod emily_membrane_dev;
/// Deterministic Emily seed corpus support for diagnostics and host acceptance tests.
pub mod emily_seed;
/// Git repository query and mutation services.
pub mod git;
/// Host-side local-agent prompt assembly backed by Emily context reads.
pub mod local_agent_context;
/// Host-side Emily episode recording and gate interpretation for local-agent runs.
pub mod local_agent_episode;
/// Host-side local-only membrane execution for the local-agent flow.
pub mod local_agent_membrane;
/// SQLite-backed local terminal restore projection.
pub mod local_restore;
/// Host-side orchestration activity views enriched with Emily episode state.
pub mod orchestration_activity;
/// SQLite-backed durable command/event/receipt timelines for orchestrated actions.
pub mod orchestration_log;
/// Group orchestration snapshot and broadcast helpers.
pub mod orchestrator;
/// Pantograph workflow host bootstrap for Emily embedding and membrane provider wiring.
pub mod pantograph_host;
/// Live Pantograph reasoning diagnostic over the real membrane remote path.
pub mod pantograph_reasoning_probe;
/// Input validation helpers for filesystem path boundaries.
pub(crate) mod path_validation;
/// Workspace load/save schema and storage routines.
pub mod persistence;
/// Cross-platform system and process resource sampling.
pub mod resource_monitor;
/// Durable Git-backed run checkpoints and review diffs.
pub mod run_checkpoints;
/// Core workspace/group/session state model.
pub mod state;
/// PTY-backed terminal runtime and snapshots.
pub mod terminal;
/// Feature-gated Alacritty-backed native terminal spike runtime and render model.
#[cfg(feature = "terminal-native-spike")]
pub mod terminal_native;
/// Dioxus desktop UI composition and interaction handling.
pub mod ui;
