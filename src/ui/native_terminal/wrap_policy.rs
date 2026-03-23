const DEFAULT_UNWRAPPED_COLS_MULTIPLIER: u16 = 8;
const MIN_UNWRAPPED_TERMINAL_COLS: u16 = 512;
const MAX_UNWRAPPED_TERMINAL_COLS: u16 = 4096;

pub(crate) fn default_unwrapped_terminal_cols(viewport_cols: u16) -> u16 {
    viewport_cols
        .saturating_mul(DEFAULT_UNWRAPPED_COLS_MULTIPLIER)
        .max(MIN_UNWRAPPED_TERMINAL_COLS)
        .min(MAX_UNWRAPPED_TERMINAL_COLS)
}

pub(crate) fn seed_unwrapped_terminal_cols(snapshot_cols: u16, viewport_cols: u16) -> u16 {
    snapshot_cols.max(default_unwrapped_terminal_cols(viewport_cols))
}

pub(crate) fn restore_unwrapped_terminal_cols(
    stored_cols: Option<u16>,
    snapshot_cols: Option<u16>,
    viewport_cols: u16,
) -> u16 {
    stored_cols
        .or(snapshot_cols)
        .map(|cols| cols.max(default_unwrapped_terminal_cols(viewport_cols)))
        .unwrap_or_else(|| default_unwrapped_terminal_cols(viewport_cols))
}

#[cfg(test)]
mod tests {
    use super::{
        default_unwrapped_terminal_cols, restore_unwrapped_terminal_cols,
        seed_unwrapped_terminal_cols,
    };

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

    #[test]
    fn seed_unwrapped_cols_preserves_wider_snapshot() {
        assert_eq!(seed_unwrapped_terminal_cols(900, 80), 900);
    }

    #[test]
    fn restore_unwrapped_cols_prefers_saved_width() {
        assert_eq!(
            restore_unwrapped_terminal_cols(Some(1200), Some(600), 80),
            1200
        );
    }

    #[test]
    fn restore_unwrapped_cols_falls_back_to_snapshot_or_default() {
        assert_eq!(
            restore_unwrapped_terminal_cols(None, Some(700), 80),
            700
        );
        assert_eq!(restore_unwrapped_terminal_cols(None, None, 80), 640);
    }
}
