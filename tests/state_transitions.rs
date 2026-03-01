use gestalt::state::{AppState, SessionRole, SessionStatus, VisibleAgentSlot};

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
