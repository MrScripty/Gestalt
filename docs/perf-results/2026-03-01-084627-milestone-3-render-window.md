# Milestone 3 Result 2026-03-01-084627

## Change Scope

Implemented in this slice:
- Reduced terminal render window sizing constants in UI hot path:
  - `RENDER_WINDOW_MULTIPLIER: 12 -> 8`
  - `RENDER_WINDOW_MIN_ROWS: 512 -> 256`
  - `src/ui/terminal_view.rs`
- Mirrored render window constants in profiler to keep benchmark modeling aligned.
  - `src/bin/profile_terminal.rs`

## Measurement Protocol

- Before reference: `docs/perf-results/2026-03-01-084224-milestone-2-round-scan-removal.md`
- After runs: `.perf/2026-03-01-084627-profile-terminal-m3-render-window.txt`
- Sample count: `10`

## Aggregated Summary (After)

```text
metric                               runs     median      p95      p99      min      max
---------------------------------- ------ ---------- -------- -------- -------- --------
render_pass_p95_us                     10     3962.0     4101     4101     3158     5808
autosave_pass_p95_us                   10     5292.0     5335     5335     5233     5343
baseline_total_send_p95_us             10       20.5       22       22       18       22
render_total_send_p95_us               10       21.5       23       23       17       24
full_total_send_p95_us                 10       20.5       24       24       17       25
```

```text
metric                               runs     median      p95      p99      min      max
---------------------------------- ------ ---------- -------- -------- -------- --------
ui_rows_rendered_per_refresh_p95       10     1008.0     1008     1008     1008     1008
ui_row_render_pass_p95_us              10      837.5      844      844      551     1073
round_bounds_extract_p95_us            10     3067.5     3235     3235     2547     3537
orchestrator_round_extract_p95_us      10       13.0       17       17       10       19
autosave_snapshot_lines_total_p95      10    36009.0    36009    36009    36009    36009
autosave_snapshot_build_p95_us         10     5292.0     5335     5335     5233     5343
```

## Before/After Deltas (p95)

| Metric | Before | After | Delta | Classification |
| --- | --- | --- | --- | --- |
| `ui_rows_rendered_per_refresh_p95` | `1536` | `1008` | `-34.4%` | Significant |
| `ui_row_render_pass_p95_us` | `1276` | `844` | `-33.9%` | Significant |
| `render_pass_p95_us` | `4396` | `4101` | `-6.7%` | Neutral |
| `autosave_snapshot_build_p95_us` | `5320` | `5335` | `+0.3%` | Neutral |
| `render_total_send_p95_us` | `25` | `23` | `-8.0%` | Neutral |
| `full_total_send_p95_us` | `23` | `24` | `+4.3%` | Neutral |

## Outcome

- Render workload volume dropped materially (`1536 -> 1008` rows per refresh p95).
- Row-render pass p95 improved significantly.
- End-to-end render pass showed moderate p95 improvement but did not cross the
  project significance threshold.
- Autosave path remained effectively unchanged and is still the major unresolved
  background cost.

## Next Focus

- Move to Milestone 4: reduce autosave snapshot cost by reusing unchanged
  session persistence state and/or making autosave serialization incremental.
