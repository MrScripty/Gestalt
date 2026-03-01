# Milestone 0 Baseline 2026-03-01-091848

## Metadata

- commit: `457d382c727ce67b03d2be7fc2ffb8b2d266ca1e`
- rustc: `rustc 1.92.0 (ded5c06cf 2025-12-08)`
- cargo: `cargo 1.92.0 (344c4567c 2025-10-21)`
- kernel: `Linux 6.17.0-14-generic x86_64 GNU/Linux`
- cpu: `Intel(R) Core(TM) Ultra 9 275HX`
- runs: `10`
- raw log: `.perf/2026-03-01-091848-profile-terminal-v2-plan-baseline.txt`

## Scope

Baseline capture after Milestone 0 instrumentation expansion for v2 plan metrics.

## Aggregated Summary

```text
metric                               runs     median      p95      p99      min      max
---------------------------------- ------ ---------- -------- -------- -------- --------
render_pass_p95_us                     10     3993.0     4036     4036     3133     4162
autosave_pass_p95_us                   10     1806.0     1837     1837     1789     1986
ui_rows_rendered_per_refresh_p95       10     1008.0     1008     1008     1008     1008
ui_row_render_pass_p95_us              10      837.0      852      852      541      863
round_bounds_extract_p95_us            10     3059.0     3184     3184     2585     3458
orchestrator_round_extract_p95_us      10       15.0       16       16       10       16
refresh_loop_tick_p95_us               10       18.0       21       21       18       21
refresh_loop_state_clone_p95_us        10        0.0        0        0        0        0
resize_measure_p95_us                  10        3.0        3        3        3        3
resize_measure_calls_per_sec_p95       10       15.0       15       15       15       15
scroll_observer_callbacks_per_sec_p95     10       90.0       90       90       90       90
orchestrator_snapshot_build_p95_us     10        9.0       10       10        9       11
autosave_fingerprint_p95_us            10    41143.5    41785    41785    40599    46945
git_watcher_poll_cost_p95_us           10     8797.5    10248    10248     8058    11044
autosave_snapshot_lines_total_p95      10    12000.0    12000    12000    12000    12000
autosave_snapshot_build_p95_us         10     1806.0     1837     1837     1789     1986
baseline_total_send_p95_us             10       25.0       26       26       20       27
render_total_send_p95_us               10       24.0       27       27       20       29
full_total_send_p95_us                 10       23.0       30       30       19       37
```

## Notes

- `resize_measure_p95_us`, `resize_measure_calls_per_sec_p95`, and
  `scroll_observer_callbacks_per_sec_p95` are **headless probe metrics** from
  `profile_terminal` (estimation/simulation path), not live DOM callbacks from a running
  Dioxus desktop session.
- `autosave_fingerprint_p95_us` shows fingerprint serialization currently contributes a
  measurable non-trivial autosave cost component.
- `git_watcher_poll_cost_p95_us` establishes a useful baseline for watcher/poll optimization.

## Commands

```bash
for i in $(seq 1 10); do
  echo "=== run $i ==="
  cargo run --quiet --bin profile_terminal -- --assert --json
done | tee .perf/2026-03-01-091848-profile-terminal-v2-plan-baseline.txt

./scripts/perf-summary.sh .perf/2026-03-01-091848-profile-terminal-v2-plan-baseline.txt \
  render_pass_p95_us autosave_pass_p95_us \
  ui_rows_rendered_per_refresh_p95 ui_row_render_pass_p95_us \
  round_bounds_extract_p95_us orchestrator_round_extract_p95_us \
  refresh_loop_tick_p95_us refresh_loop_state_clone_p95_us \
  resize_measure_p95_us resize_measure_calls_per_sec_p95 \
  scroll_observer_callbacks_per_sec_p95 orchestrator_snapshot_build_p95_us \
  autosave_fingerprint_p95_us git_watcher_poll_cost_p95_us \
  autosave_snapshot_lines_total_p95 autosave_snapshot_build_p95_us \
  baseline_total_send_p95_us render_total_send_p95_us full_total_send_p95_us
```
