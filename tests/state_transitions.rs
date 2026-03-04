use gestalt::state::{AppState, SessionRole, SessionStatus, VisibleAgentSlot};
use serde_json::Value;

fn seeded_state() -> AppState {
    let mut state = AppState::default();
    let first_group = state.groups[0].id;
    let (second_group, _) = state.create_group_with_defaults("/tmp".to_string());

    state.add_session_with_title_and_role(first_group, "Session A".to_string(), SessionRole::Agent);
    state.add_session_with_title_and_role(first_group, "Session B".to_string(), SessionRole::Agent);
    state.add_session_with_title_and_role(
        second_group,
        "Session C".to_string(),
        SessionRole::Agent,
    );

    state
}

#[test]
fn default_group_has_three_sessions() {
    let state = AppState::default();
    let group_id = state.groups[0].id;
    let sessions = state
        .sessions
        .iter()
        .filter(|session| session.group_id == group_id)
        .count();
    assert_eq!(sessions, 3);
}

#[test]
fn move_session_before_reorders_and_adopts_target_group() {
    let mut state = seeded_state();
    let source = state.sessions[0].id;
    let target = state.sessions[3].id;
    let target_group = state.sessions[3].group_id;

    state.move_session_before(source, target);

    let source_idx = state
        .sessions
        .iter()
        .position(|session| session.id == source)
        .expect("source session to exist");
    let target_idx = state
        .sessions
        .iter()
        .position(|session| session.id == target)
        .expect("target session to exist");

    assert_eq!(source_idx + 1, target_idx);
    assert_eq!(state.sessions[source_idx].group_id, target_group);
}

#[test]
fn move_session_to_group_end_places_session_after_group_tail() {
    let mut state = seeded_state();
    let source = state.sessions[1].id;
    let destination_group = state.sessions[3].group_id;

    state.move_session_to_group_end(source, destination_group);

    let moved_idx = state
        .sessions
        .iter()
        .position(|session| session.id == source)
        .expect("moved session to exist");
    assert_eq!(state.sessions[moved_idx].group_id, destination_group);

    let last_group_idx = state
        .sessions
        .iter()
        .rposition(|session| session.group_id == destination_group)
        .expect("destination group to contain at least one session");
    assert_eq!(moved_idx, last_group_idx);
}

#[test]
fn session_status_cycle_changes_state() {
    let mut state = seeded_state();
    let id = state.sessions[0].id;

    state.set_session_status(id, SessionStatus::Idle);
    state.cycle_session_status(id);

    let status = state
        .sessions
        .iter()
        .find(|session| session.id == id)
        .expect("session to exist")
        .status;
    assert_eq!(status, SessionStatus::Busy);
}

#[test]
fn test_into_restored_with_invalid_selection_selects_first_valid_session() {
    let mut state = AppState::default();
    state.selected_session = Some(u32::MAX);

    let restored = state.into_restored();

    assert!(restored.selected_session.is_some());
    let selected = restored.selected_session.expect("selection exists");
    assert!(
        restored
            .sessions
            .iter()
            .any(|session| session.id == selected)
    );
}

#[test]
fn test_revision_after_mutation_increments() {
    let mut state = AppState::default();
    let before = state.revision();
    let group_id = state.groups[0].id;
    state.add_session(group_id);
    assert!(state.revision() > before);
}

#[test]
fn swap_session_with_top_slot_brings_hidden_tab_into_top_pane() {
    let mut state = AppState::default();
    let group_id = state.groups[0].id;
    let hidden_id =
        state.add_session_with_title_and_role(group_id, "Hidden".to_string(), SessionRole::Agent);

    state.swap_session_with_visible_agent_slot(hidden_id, VisibleAgentSlot::Top);

    let (agents, runner) = state.workspace_sessions_for_group(group_id);
    assert_eq!(agents.len(), 2);
    assert_eq!(agents[0].id, hidden_id);
    assert_eq!(state.selected_session, Some(hidden_id));
    assert_eq!(
        runner.map(|session| session.role),
        Some(SessionRole::Runner)
    );
}

#[test]
fn swap_session_with_bottom_slot_brings_hidden_tab_into_bottom_pane() {
    let mut state = AppState::default();
    let group_id = state.groups[0].id;
    let hidden_id =
        state.add_session_with_title_and_role(group_id, "Hidden".to_string(), SessionRole::Agent);

    state.swap_session_with_visible_agent_slot(hidden_id, VisibleAgentSlot::Bottom);

    let (agents, _) = state.workspace_sessions_for_group(group_id);
    assert_eq!(agents.len(), 2);
    assert_eq!(agents[1].id, hidden_id);
    assert_eq!(state.selected_session, Some(hidden_id));
}

#[test]
fn remove_group_drops_group_and_associated_sessions() {
    let mut state = AppState::default();
    let (remove_group_id, remove_group_sessions) =
        state.create_group_with_defaults("/tmp".to_string());
    let group_sessions = state.sessions_in_group(remove_group_id);
    let removed_session_ids = group_sessions
        .iter()
        .map(|session| session.id)
        .collect::<Vec<_>>();
    assert_eq!(removed_session_ids.len(), remove_group_sessions.len());

    let removed = state.remove_group(remove_group_id);

    assert_eq!(removed, removed_session_ids);
    assert!(state.groups.iter().all(|group| group.id != remove_group_id));
    assert!(
        state
            .sessions
            .iter()
            .all(|session| session.group_id != remove_group_id)
    );
}

#[test]
fn remove_group_updates_selected_session_when_selection_is_removed() {
    let mut state = AppState::default();
    let initial_group_id = state.groups[0].id;
    let (remove_group_id, remove_group_sessions) =
        state.create_group_with_defaults("/tmp".to_string());
    let selected = remove_group_sessions[0];
    state.select_session(selected);

    let removed = state.remove_group(remove_group_id);

    assert!(removed.contains(&selected));
    assert_eq!(
        state.selected_session,
        state.sessions.first().map(|session| session.id)
    );
    assert!(
        state
            .sessions
            .iter()
            .all(|session| session.group_id == initial_group_id)
    );
}

#[test]
fn remove_session_updates_selected_to_same_group_when_available() {
    let mut state = AppState::default();
    let group_id = state.groups[0].id;
    let selected = state
        .sessions
        .iter()
        .find(|session| session.group_id == group_id)
        .map(|session| session.id)
        .expect("default group should have sessions");
    state.select_session(selected);

    let removed = state.remove_session(selected);

    assert!(removed);
    assert_ne!(state.selected_session, Some(selected));
    let next_selected = state.selected_session.expect("selection should remain");
    assert!(
        state
            .sessions
            .iter()
            .any(|session| session.id == next_selected && session.group_id == group_id)
    );
}

#[test]
fn group_layout_updates_are_scoped_per_group() {
    let mut state = AppState::default();
    let first_group = state.groups[0].id;
    let (second_group, _) = state.create_group_with_defaults("/tmp/group-layout".to_string());

    state.set_group_runner_width_px(first_group, 640);
    state.set_group_agent_top_ratio(first_group, 0.63);
    state.set_group_runner_top_ratio(first_group, 0.41);

    let first_layout = state.group_layout(first_group);
    let second_layout = state.group_layout(second_group);

    assert_eq!(first_layout.runner_width_px, 640);
    assert_eq!(first_layout.agent_top_ratio, 0.63);
    assert_eq!(first_layout.runner_top_ratio, 0.41);
    assert_eq!(second_layout.runner_width_px, 340);
    assert_eq!(second_layout.agent_top_ratio, 0.5);
    assert_eq!(second_layout.runner_top_ratio, 0.5);
}

#[test]
fn group_layout_values_are_clamped_and_restored_safely() {
    let mut state = AppState::default();
    let group_id = state.groups[0].id;

    state.set_group_runner_width_px(group_id, 10_000);
    state.set_group_agent_top_ratio(group_id, 99.0);
    state.set_group_runner_top_ratio(group_id, f64::NAN);
    let clamped = state.group_layout(group_id);
    assert_eq!(clamped.runner_width_px, 760);
    assert_eq!(clamped.agent_top_ratio, 0.72);
    assert_eq!(clamped.runner_top_ratio, 0.5);

    state.groups[0].layout.runner_width_px = -100;
    state.groups[0].layout.agent_top_ratio = -1.0;
    state.groups[0].layout.runner_top_ratio = 900.0;
    let restored = state.into_restored();
    let restored_layout = restored.group_layout(group_id);
    assert_eq!(restored_layout.runner_width_px, 260);
    assert_eq!(restored_layout.agent_top_ratio, 0.28);
    assert_eq!(restored_layout.runner_top_ratio, 0.72);
}

#[test]
fn group_layout_defaults_when_loading_legacy_groups_without_layout() {
    let state = AppState::default();
    let mut encoded = serde_json::to_value(&state).expect("state should serialize");
    let groups = encoded
        .get_mut("groups")
        .and_then(Value::as_array_mut)
        .expect("groups should serialize as array");
    for group in groups {
        let object = group
            .as_object_mut()
            .expect("group should serialize as object");
        object.remove("layout");
    }

    let restored: AppState = serde_json::from_value(encoded).expect("legacy state should load");
    let group_id = restored.groups[0].id;
    let layout = restored.group_layout(group_id);
    assert_eq!(layout.runner_width_px, 340);
    assert_eq!(layout.agent_top_ratio, 0.5);
    assert_eq!(layout.runner_top_ratio, 0.5);
}

#[test]
fn set_ui_scale_clamps_and_marks_state_dirty() {
    let mut state = AppState::default();
    let before = state.revision();

    state.set_ui_scale(99.0);

    assert_eq!(state.ui_scale(), 1.8);
    assert!(state.revision() > before);
}

#[test]
fn missing_ui_scale_field_restores_default_scale() {
    let mut payload = serde_json::to_value(AppState::default()).expect("serialize app state");
    let object = payload
        .as_object_mut()
        .expect("app state should serialize to an object");
    object.remove("ui_scale");

    let restored: AppState = serde_json::from_value(payload).expect("deserialize app state");
    assert_eq!(restored.ui_scale(), 1.0);
}

#[test]
fn remove_session_preserves_selection_when_other_tab_removed() {
    let mut state = AppState::default();
    let selected = state.sessions[0].id;
    let other = state.sessions[1].id;
    state.select_session(selected);

    let removed = state.remove_session(other);

    assert!(removed);
    assert_eq!(state.selected_session, Some(selected));
    assert!(state.sessions.iter().all(|session| session.id != other));
}

#[test]
fn remove_session_last_group_terminal_keeps_group_and_clears_selection() {
    let mut state = AppState::default();
    let target_group_id = state.groups[0].id;
    let target_group_sessions = state
        .sessions
        .iter()
        .filter(|session| session.group_id == target_group_id)
        .map(|session| session.id)
        .collect::<Vec<_>>();

    for session_id in target_group_sessions {
        let removed = state.remove_session(session_id);
        assert!(removed);
    }

    assert!(state.groups.iter().any(|group| group.id == target_group_id));
    assert!(state.sessions_in_group(target_group_id).is_empty());
    assert_eq!(state.selected_session, None);
}

#[test]
fn insert_command_crud_updates_state() {
    let mut state = AppState::default();
    let before_revision = state.revision();

    let command_id = state.create_insert_command(
        "Build".to_string(),
        "cargo build".to_string(),
        "Build the project".to_string(),
        vec!["cargo".to_string(), "build".to_string()],
    );
    assert_eq!(state.commands().len(), 1);
    assert!(state.revision() > before_revision);

    let updated = state.update_insert_command(
        command_id,
        "Build Release".to_string(),
        "cargo build --release".to_string(),
        "Build release artifacts".to_string(),
        vec!["cargo".to_string(), "release".to_string()],
    );
    assert!(updated);
    let command = state
        .command_by_id(command_id)
        .expect("command should exist after update");
    assert_eq!(command.name, "Build Release");

    let removed = state.delete_insert_command(command_id);
    assert!(removed);
    assert!(state.commands().is_empty());
}

#[test]
fn restore_repairs_command_library_defaults() {
    let mut state = AppState::default();
    state.command_library.next_command_id = 0;

    let restored = state.into_restored();
    assert!(restored.command_library.next_command_id >= 1);
}
