# Milestone 2 Result 2026-03-01-084224

## Change Scope

Implemented in this slice:
- Removed round-bound scan from terminal render hot path and compute it only on `Ctrl+A` selection action.
  - `src/ui/terminal_view.rs`
- Bounded orchestrator prompt scan window for latest-round extraction (`MAX_PROMPT_SCAN_LINES = 2048`).
  - `src/orchestrator/runtime.rs`
- Updated profiler to keep render-pass timing isolated and measure round-bound extraction as a separate probe.
  - `src/bin/profile_terminal.rs`

## Measurement Protocol

- Before reference: `docs/perf-results/2026-03-01-082534-render-instrumentation-baseline.md`
- After runs: `.perf/2026-03-01-084224-profile-terminal-m2-fullscan-reduced-v2.txt`
- Sample count: `10`

## Aggregated Summary (After)

```text
metric                               runs     median      p95      p99      min      max
---------------------------------- ------ ---------- -------- -------- -------- --------
render_pass_p95_us                     10     3424.5     4396     4396     3375     4422
autosave_pass_p95_us                   10     5298.5     5320     5320     5233     5792
baseline_total_send_p95_us             10       20.0       22       22       17       22
render_total_send_p95_us               10       21.5       25       25       17       28
full_total_send_p95_us                 10       21.0       23       23       18       24
```

```text
metric                               runs     median      p95      p99      min      max
---------------------------------- ------ ---------- -------- -------- -------- --------
ui_rows_rendered_per_refresh_p95       10     1536.0     1536     1536     1536     1536
ui_row_render_pass_p95_us              10      819.0     1276     1276      803     1278
round_bounds_extract_p95_us            10     2597.5     3185     3185     2553     3265
orchestrator_round_extract_p95_us      10       10.0       13       13        8       15
autosave_snapshot_lines_total_p95      10    36009.0    36009    36009    36009    36009
autosave_snapshot_build_p95_us         10     5298.5     5320     5320     5233     5792
```

## Before/After Deltas (p95)

| Metric | Before | After | Delta | Classification |
| --- | --- | --- | --- | --- |
| `render_pass_p95_us` | `4429` | `4396` | `-0.7%` | Neutral |
| `ui_row_render_pass_p95_us` | `1284` | `1276` | `-0.6%` | Neutral |
| `orchestrator_round_extract_p95_us` | `15` | `13` | `-13.3%` | Significant (narrow metric) |
| `autosave_snapshot_build_p95_us` | `5647` | `5320` | `-5.8%` | Neutral |

`round_bounds_extract_p95_us` is not directly comparable to the previous run set because the probe now measures isolated extraction cost outside render-pass timing.

## Outcome

- Removing the render-path round scan and bounding orchestrator scan depth reduced orchestrator extraction p95.
- End-to-end render-pass and row-render p95 were effectively unchanged in this slice.
- The dominant unresolved bottlenecks remain:
  - row-render workload volume (`1536` rows per refresh p95)
  - autosave snapshot payload size (`36009` lines per pass p95)

## Next Focus

- Reduce rows rendered per refresh (window sizing strategy) and avoid redundant row parsing work.
- Reduce autosave snapshot line volume and/or move to incremental terminal persistence.
