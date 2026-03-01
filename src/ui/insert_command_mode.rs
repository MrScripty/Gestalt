use crate::commands::{CommandId, CommandMatch, InsertCommand, rank_commands};
use crate::state::SessionId;
use dioxus::prelude::Key;

pub(crate) const INSERT_COMMAND_MATCH_LIMIT: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InsertModeState {
    pub session_id: SessionId,
    pub query: String,
    pub highlighted_index: usize,
}

pub(crate) enum InsertModeOutcome {
    Keep(InsertModeState),
    Close,
    Submit(CommandId),
    Ignore,
}

pub(crate) enum TerminalKeyRoute {
    OpenMode,
    HandleMode(InsertModeOutcome),
    Passthrough,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct KeyModifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InsertModeSelection {
    pub selected_command_id: Option<CommandId>,
    pub match_count: usize,
}

pub(crate) fn is_insert_trigger_key(
    key: &Key,
    ctrl: bool,
    alt: bool,
    shift: bool,
    meta: bool,
) -> bool {
    matches!(key, Key::Insert) && !ctrl && !alt && !shift && !meta
}

pub(crate) fn command_matches(commands: &[InsertCommand], query: &str) -> Vec<CommandMatch> {
    rank_commands(commands, query, INSERT_COMMAND_MATCH_LIMIT)
}

pub(crate) fn selected_command_id(
    matches: &[CommandMatch],
    highlighted_index: usize,
) -> Option<CommandId> {
    if matches.is_empty() {
        return None;
    }

    let index = highlighted_index.min(matches.len().saturating_sub(1));
    matches.get(index).map(|entry| entry.command_id)
}

pub(crate) fn reduce_insert_mode_key(
    mode: &InsertModeState,
    key: &Key,
    modifiers: KeyModifiers,
    selection: InsertModeSelection,
) -> InsertModeOutcome {
    match key {
        Key::Escape => InsertModeOutcome::Close,
        Key::Insert => InsertModeOutcome::Close,
        Key::Enter => selection
            .selected_command_id
            .map(InsertModeOutcome::Submit)
            .unwrap_or(InsertModeOutcome::Ignore),
        Key::ArrowUp => {
            if selection.match_count == 0 {
                return InsertModeOutcome::Ignore;
            }
            let current = mode
                .highlighted_index
                .min(selection.match_count.saturating_sub(1));
            let next = if current == 0 {
                selection.match_count.saturating_sub(1)
            } else {
                current.saturating_sub(1)
            };
            InsertModeOutcome::Keep(InsertModeState {
                session_id: mode.session_id,
                query: mode.query.clone(),
                highlighted_index: next,
            })
        }
        Key::ArrowDown => {
            if selection.match_count == 0 {
                return InsertModeOutcome::Ignore;
            }
            let current = mode
                .highlighted_index
                .min(selection.match_count.saturating_sub(1));
            let next = (current + 1) % selection.match_count;
            InsertModeOutcome::Keep(InsertModeState {
                session_id: mode.session_id,
                query: mode.query.clone(),
                highlighted_index: next,
            })
        }
        Key::Backspace => {
            let mut next_query = mode.query.clone();
            next_query.pop();
            InsertModeOutcome::Keep(InsertModeState {
                session_id: mode.session_id,
                query: next_query,
                highlighted_index: 0,
            })
        }
        Key::Delete => InsertModeOutcome::Keep(InsertModeState {
            session_id: mode.session_id,
            query: String::new(),
            highlighted_index: 0,
        }),
        Key::Character(text) if !modifiers.ctrl && !modifiers.alt && !modifiers.meta => {
            let mut next_query = mode.query.clone();
            next_query.push_str(text);
            if modifiers.shift && text.is_empty() {
                return InsertModeOutcome::Ignore;
            }
            InsertModeOutcome::Keep(InsertModeState {
                session_id: mode.session_id,
                query: next_query,
                highlighted_index: 0,
            })
        }
        _ => InsertModeOutcome::Ignore,
    }
}

pub(crate) fn route_terminal_key(
    mode: Option<&InsertModeState>,
    key: &Key,
    modifiers: KeyModifiers,
    selection: InsertModeSelection,
) -> TerminalKeyRoute {
    if let Some(mode) = mode {
        return TerminalKeyRoute::HandleMode(reduce_insert_mode_key(
            mode, key, modifiers, selection,
        ));
    }

    if is_insert_trigger_key(
        key,
        modifiers.ctrl,
        modifiers.alt,
        modifiers.shift,
        modifiers.meta,
    ) {
        TerminalKeyRoute::OpenMode
    } else {
        TerminalKeyRoute::Passthrough
    }
}

pub(crate) fn mode_after_focus(
    mode: Option<InsertModeState>,
    focused_session: SessionId,
) -> Option<InsertModeState> {
    match mode {
        Some(mode) if mode.session_id != focused_session => None,
        state => state,
    }
}

pub(crate) fn mode_after_blur(
    mode: Option<InsertModeState>,
    blurred_session: SessionId,
) -> Option<InsertModeState> {
    match mode {
        Some(mode) if mode.session_id == blurred_session => None,
        state => state,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        InsertModeOutcome, InsertModeSelection, InsertModeState, KeyModifiers, TerminalKeyRoute,
        mode_after_blur, mode_after_focus, reduce_insert_mode_key, route_terminal_key,
    };
    use dioxus::prelude::Key;

    #[test]
    fn character_input_keeps_session_target() {
        let mode = InsertModeState {
            session_id: 42,
            query: "bu".to_string(),
            highlighted_index: 0,
        };
        let outcome = reduce_insert_mode_key(
            &mode,
            &Key::Character("i".to_string()),
            KeyModifiers {
                ctrl: false,
                alt: false,
                shift: false,
                meta: false,
            },
            InsertModeSelection {
                selected_command_id: None,
                match_count: 0,
            },
        );

        match outcome {
            InsertModeOutcome::Keep(next) => {
                assert_eq!(next.session_id, 42);
                assert_eq!(next.query, "bui");
            }
            _ => panic!("expected keep outcome"),
        }
    }

    #[test]
    fn enter_submits_selected_command() {
        let mode = InsertModeState {
            session_id: 7,
            query: "build".to_string(),
            highlighted_index: 0,
        };
        let outcome = reduce_insert_mode_key(
            &mode,
            &Key::Enter,
            KeyModifiers {
                ctrl: false,
                alt: false,
                shift: false,
                meta: false,
            },
            InsertModeSelection {
                selected_command_id: Some(99),
                match_count: 3,
            },
        );

        match outcome {
            InsertModeOutcome::Submit(command_id) => assert_eq!(command_id, 99),
            _ => panic!("expected submit outcome"),
        }
    }

    #[test]
    fn arrows_wrap_highlight_index() {
        let mode = InsertModeState {
            session_id: 1,
            query: "x".to_string(),
            highlighted_index: 0,
        };

        let up = reduce_insert_mode_key(
            &mode,
            &Key::ArrowUp,
            KeyModifiers {
                ctrl: false,
                alt: false,
                shift: false,
                meta: false,
            },
            InsertModeSelection {
                selected_command_id: Some(1),
                match_count: 3,
            },
        );
        match up {
            InsertModeOutcome::Keep(next) => assert_eq!(next.highlighted_index, 2),
            _ => panic!("expected keep outcome"),
        }

        let down = reduce_insert_mode_key(
            &mode,
            &Key::ArrowDown,
            KeyModifiers {
                ctrl: false,
                alt: false,
                shift: false,
                meta: false,
            },
            InsertModeSelection {
                selected_command_id: Some(1),
                match_count: 3,
            },
        );
        match down {
            InsertModeOutcome::Keep(next) => assert_eq!(next.highlighted_index, 1),
            _ => panic!("expected keep outcome"),
        }
    }

    #[test]
    fn route_insert_opens_mode_when_closed() {
        let route = route_terminal_key(
            None,
            &Key::Insert,
            KeyModifiers {
                ctrl: false,
                alt: false,
                shift: false,
                meta: false,
            },
            InsertModeSelection {
                selected_command_id: None,
                match_count: 0,
            },
        );

        assert!(matches!(route, TerminalKeyRoute::OpenMode));
    }

    #[test]
    fn route_slash_passthrough_when_closed() {
        let route = route_terminal_key(
            None,
            &Key::Character("/".to_string()),
            KeyModifiers {
                ctrl: false,
                alt: false,
                shift: false,
                meta: false,
            },
            InsertModeSelection {
                selected_command_id: None,
                match_count: 0,
            },
        );

        assert!(matches!(route, TerminalKeyRoute::Passthrough));
    }

    #[test]
    fn route_ctrl_c_passthrough_when_closed() {
        let route = route_terminal_key(
            None,
            &Key::Character("c".to_string()),
            KeyModifiers {
                ctrl: true,
                alt: false,
                shift: false,
                meta: false,
            },
            InsertModeSelection {
                selected_command_id: None,
                match_count: 0,
            },
        );

        assert!(matches!(route, TerminalKeyRoute::Passthrough));
    }

    #[test]
    fn shift_insert_does_not_open_command_mode() {
        let route = route_terminal_key(
            None,
            &Key::Insert,
            KeyModifiers {
                ctrl: false,
                alt: false,
                shift: true,
                meta: false,
            },
            InsertModeSelection {
                selected_command_id: None,
                match_count: 0,
            },
        );

        assert!(matches!(route, TerminalKeyRoute::Passthrough));
    }

    #[test]
    fn route_keys_are_consumed_when_mode_open() {
        let mode = InsertModeState {
            session_id: 5,
            query: "d".to_string(),
            highlighted_index: 0,
        };
        let route = route_terminal_key(
            Some(&mode),
            &Key::Character("/".to_string()),
            KeyModifiers {
                ctrl: false,
                alt: false,
                shift: false,
                meta: false,
            },
            InsertModeSelection {
                selected_command_id: None,
                match_count: 1,
            },
        );

        assert!(matches!(route, TerminalKeyRoute::HandleMode(_)));
    }

    #[test]
    fn focus_change_closes_mode_for_other_session() {
        let mode = Some(InsertModeState {
            session_id: 10,
            query: String::new(),
            highlighted_index: 0,
        });

        assert!(mode_after_focus(mode.clone(), 11).is_none());
        assert!(mode_after_focus(mode.clone(), 10).is_some());
        assert!(mode_after_blur(mode, 10).is_none());
    }
}
