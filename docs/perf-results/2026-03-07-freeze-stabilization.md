# Perf Result - Freeze Stabilization

## Metadata
- commit range: `7f26966..bd3768e`
- date: 2026-03-07
- environment: local developer machine
- build: `cargo test` + `cargo run --quiet --bin profile_terminal -- --assert`
- scenario: Gestalt freeze-stabilization slices covering Emily bridge calls, autosave offloading, shared resource polling, and terminal snapshot clone reduction

## Metrics

| Metric | Before (p95) | After (p95) | Delta | Significance |
| --- | --- | --- | --- | --- |
| `render_pass_p95_us` | `5501` | `5445` | `-1.0%` | Neutral |
| `ui_row_render_pass_p95_us` | `836` | `800` | `-4.3%` | Neutral |
| `round_bounds_extract_p95_us` | `3127` | `3276` | `+4.8%` | Neutral noise |
| `git_watcher_poll_cost_p95_us` | `3420` | `3731` | `+9.1%` | Neutral noise |
| `startup_full_restore_p95_us` | `111028` | `127521` | `+14.9%` | Noisy regression; not targeted in this slice |
| `baseline_total_send_p95_us` | `29` | `27` | `-6.9%` | Neutral |
| `render_total_send_p95_us` | `29` | `27` | `-6.9%` | Neutral |
| `full_total_send_p95_us` | `29` | `26` | `-10.3%` | Directional improvement |

## Notes
- Structural freeze-risk reductions were the primary goal. The benchmark does not model the removed UI-blocking Emily request paths or the autosave projection writes that previously ran on the UI-sensitive autosave future.
- All validation runs completed with `Perf assertions passed`.
- The main confirmed code changes were:
  - UI-facing Emily bridge requests now have async call paths and UI call sites use them.
  - Autosave snapshot construction now runs in a blocking task and projection/workspace writes are worker-owned.
  - Process/resource polling is root-owned instead of duplicated in child UI components.
  - Terminal snapshot rebuilds no longer take an extra full scrollback clone before publishing immutable snapshots.
  - The git refresh coordinator now projects only the active path data it needs instead of cloning full `AppState` snapshots on every tick.
- Follow-up focus should remain on git watcher cost and startup restore latency, since those metrics were not improved by this stabilization pass.

## Verification Summary
- `cargo fmt`
- `cargo test`
- `cargo run --quiet --bin profile_terminal -- --assert`
