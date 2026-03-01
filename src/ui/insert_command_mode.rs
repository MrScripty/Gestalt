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
