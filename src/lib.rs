/// Git repository query and mutation services.
pub mod git;
/// Group orchestration snapshot and broadcast helpers.
pub mod orchestrator;
/// Input validation helpers for filesystem path boundaries.
pub(crate) mod path_validation;
/// Workspace load/save schema and storage routines.
pub mod persistence;
/// Core workspace/group/session state model.
pub mod state;
/// PTY-backed terminal runtime and snapshots.
pub mod terminal;
/// Dioxus desktop UI composition and interaction handling.
pub mod ui;
