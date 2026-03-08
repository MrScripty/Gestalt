/// Insert-command models, matching, and validation helpers.
pub mod commands;
/// Emily integration bridge for ingest/query APIs.
pub mod emily_bridge;
/// Deterministic Emily inspection helpers for host-side diagnostics.
pub mod emily_inspect;
/// Deterministic Emily seed corpus support for diagnostics and host acceptance tests.
pub mod emily_seed;
/// Git repository query and mutation services.
pub mod git;
/// SQLite-backed local terminal restore projection.
pub mod local_restore;
/// SQLite-backed durable command/event/receipt timelines for orchestrated actions.
pub mod orchestration_log;
/// Group orchestration snapshot and broadcast helpers.
pub mod orchestrator;
/// Pantograph workflow host bootstrap for Emily embedding provider wiring.
pub(crate) mod pantograph_host;
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
/// Dioxus desktop UI composition and interaction handling.
pub mod ui;
