mod component;
mod constants;
mod frame;
mod glyph_atlas;
mod paint;
mod renderer;
mod scene;

pub(crate) use component::NativeTerminalBody;

pub(crate) const PILOT_ENV_VAR: &str = "GESTALT_NATIVE_TERMINAL_PILOT";
const PILOT_SCOPE_ENV_VAR: &str = "GESTALT_NATIVE_TERMINAL_PILOT_SCOPE";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeTerminalPilotScope {
    Selected,
    Visible,
}

pub(crate) fn native_terminal_pilot_enabled() -> bool {
    std::env::var(PILOT_ENV_VAR)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn native_terminal_pilot_scope() -> NativeTerminalPilotScope {
    match std::env::var(PILOT_SCOPE_ENV_VAR) {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "visible" | "all-visible" | "all" => NativeTerminalPilotScope::Visible,
            _ => NativeTerminalPilotScope::Selected,
        },
        Err(_) => NativeTerminalPilotScope::Selected,
    }
}

pub(crate) fn native_terminal_pilot_active_for_pane(is_selected: bool) -> bool {
    if !native_terminal_pilot_enabled() {
        return false;
    }

    match native_terminal_pilot_scope() {
        NativeTerminalPilotScope::Selected => is_selected,
        NativeTerminalPilotScope::Visible => true,
    }
}

#[cfg(test)]
mod tests {
    use super::{PILOT_ENV_VAR, PILOT_SCOPE_ENV_VAR};

    #[test]
    fn pilot_flag_accepts_enabled_values() {
        unsafe { std::env::set_var(PILOT_ENV_VAR, "true") };
        assert!(super::native_terminal_pilot_enabled());
        unsafe { std::env::remove_var(PILOT_ENV_VAR) };
    }

    #[test]
    fn pilot_flag_defaults_disabled() {
        unsafe { std::env::remove_var(PILOT_ENV_VAR) };
        assert!(!super::native_terminal_pilot_enabled());
    }

    #[test]
    fn pilot_scope_defaults_to_selected() {
        unsafe { std::env::remove_var(PILOT_SCOPE_ENV_VAR) };
        assert_eq!(
            super::native_terminal_pilot_scope(),
            super::NativeTerminalPilotScope::Selected
        );
    }

    #[test]
    fn visible_scope_activates_unselected_panes() {
        unsafe { std::env::set_var(PILOT_ENV_VAR, "true") };
        unsafe { std::env::set_var(PILOT_SCOPE_ENV_VAR, "visible") };
        assert!(super::native_terminal_pilot_active_for_pane(false));
        unsafe { std::env::remove_var(PILOT_SCOPE_ENV_VAR) };
        unsafe { std::env::remove_var(PILOT_ENV_VAR) };
    }
}
