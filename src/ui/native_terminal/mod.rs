pub(crate) const PILOT_ENV_VAR: &str = "GESTALT_NATIVE_TERMINAL_PILOT";

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

#[cfg(test)]
mod tests {
    use super::PILOT_ENV_VAR;

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
}
