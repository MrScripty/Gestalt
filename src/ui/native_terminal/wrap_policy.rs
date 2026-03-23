const DEFAULT_UNWRAPPED_COLS_MULTIPLIER: u16 = 8;
const MIN_UNWRAPPED_TERMINAL_COLS: u16 = 512;
const MAX_UNWRAPPED_TERMINAL_COLS: u16 = 4096;

pub(crate) fn default_unwrapped_terminal_cols(viewport_cols: u16) -> u16 {
    viewport_cols
        .saturating_mul(DEFAULT_UNWRAPPED_COLS_MULTIPLIER)
        .max(MIN_UNWRAPPED_TERMINAL_COLS)
        .min(MAX_UNWRAPPED_TERMINAL_COLS)
}

#[cfg(test)]
mod tests {
    use super::default_unwrapped_terminal_cols;

    #[test]
    fn default_unwrapped_cols_has_floor() {
        assert_eq!(default_unwrapped_terminal_cols(24), 512);
    }

    #[test]
    fn default_unwrapped_cols_scales_with_viewport() {
        assert_eq!(default_unwrapped_terminal_cols(96), 768);
    }

    #[test]
    fn default_unwrapped_cols_has_ceiling() {
        assert_eq!(default_unwrapped_terminal_cols(600), 4096);
    }
}
