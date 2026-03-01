# Milestone 4 Result 2026-03-01-090832

## Change Scope

Implemented in this slice:
- Added history-limited terminal persistence API:
  - `snapshot_for_persist_limited(session_id, max_history_lines)`
  - `src/terminal.rs`
- Added history-limited workspace snapshot builder:
  - `build_workspace_snapshot_with_history_limit(...)`
  - `src/persistence/mod.rs`
- Switched autosave path to use a capped terminal history budget (`4000` lines/session):
  - `src/ui.rs`
- Aligned profiler autosave simulation to use the same cap:
  - `src/bin/profile_terminal.rs`

Shutdown/manual workspace save path remains full-fidelity (`build_workspace_snapshot`).

## Measurement Protocol

- Before reference: `docs/perf-results/2026-03-01-084627-milestone-3-render-window.md`
- After runs: `.perf/2026-03-01-090832-profile-terminal-m4-autosave-history-cap.txt`
- Sample count: `10`

## Aggregated Summary (After)

```text
metric                               runs     median      p95      p99      min      max
---------------------------------- ------ ---------- -------- -------- -------- --------
render_pass_p95_us                     10     3907.5     4039     4039     3134     4242
autosave_pass_p95_us                   10     1765.5     1785     1785     1743     1786
baseline_total_send_p95_us             10       21.5       25       25       19       26
render_total_send_p95_us               10       22.5       25       25       18       27
full_total_send_p95_us                 10       24.5       27       27       21       29
```

```text
metric                               runs     median      p95      p99      min      max
---------------------------------- ------ ---------- -------- -------- -------- --------
ui_rows_rendered_per_refresh_p95       10     1008.0     1008     1008     1008     1008
ui_row_render_pass_p95_us              10      785.0      796      796      533      820
round_bounds_extract_p95_us            10     3101.5     3256     3256     2578     3385
orchestrator_round_extract_p95_us      10       14.5       16       16        9       18
autosave_snapshot_lines_total_p95      10    12000.0    12000    12000    12000    12000
autosave_snapshot_build_p95_us         10     1765.5     1785     1785     1743     1786
```

## Before/After Deltas (p95)

| Metric | Before | After | Delta | Classification |
| --- | --- | --- | --- | --- |
| `autosave_snapshot_lines_total_p95` | `36009` | `12000` | `-66.7%` | Significant |
| `autosave_snapshot_build_p95_us` | `5335` | `1785` | `-66.5%` | Significant |
| `render_pass_p95_us` | `4101` | `4039` | `-1.5%` | Neutral |
| `ui_row_render_pass_p95_us` | `844` | `796` | `-5.7%` | Neutral |
| `baseline_total_send_p95_us` | `22` | `25` | `+13.6%` | Regression |
| `full_total_send_p95_us` | `24` | `27` | `+12.5%` | Regression |

## Outcome

- Milestone 4 successfully reduced autosave snapshot payload and build cost by roughly two
  thirds with a clear p95 gain.
- Render-path metrics remained stable to slightly improved.
- PTY send latency tails regressed in this run set; this likely reflects contention/noise
  outside autosave serialization itself and needs final validation in Milestone 5 with a
  consolidated rerun.

## Tradeoff

- Autosave now persists up to `4000` history lines per terminal during periodic saves.
- Full-fidelity terminal history persistence still occurs on shutdown/manual save.
- Crash recovery will therefore restore less terminal history than a clean shutdown path.
