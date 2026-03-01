# Milestone 5 Final Comparison 2026-03-01-091603

## Scope

Consolidated final rerun and significance review across the terminal render/refresh audit milestones:

- Baseline (instrumented): `.perf/2026-03-01-082534-profile-terminal-render-instrumented-baseline.txt`
- Milestone 2: `.perf/2026-03-01-084224-profile-terminal-m2-fullscan-reduced-v2.txt`
- Milestone 3: `.perf/2026-03-01-084627-profile-terminal-m3-render-window.txt`
- Milestone 4: `.perf/2026-03-01-090832-profile-terminal-m4-autosave-history-cap.txt`
- Final rerun (current): `.perf/2026-03-01-091603-profile-terminal-m5-final-rerun.txt`

Sample count is `10` runs for each set.

## Consolidated p95 Table

| Metric | Baseline | M2 | M3 | M4 | Final (M5) |
| --- | --- | --- | --- | --- | --- |
| `render_pass_p95_us` | `4429` | `4396` | `4101` | `4039` | `4065` |
| `autosave_pass_p95_us` | `5647` | `5320` | `5335` | `1785` | `1784` |
| `ui_rows_rendered_per_refresh_p95` | `1536` | `1536` | `1008` | `1008` | `1008` |
| `ui_row_render_pass_p95_us` | `1284` | `1276` | `844` | `796` | `801` |
| `autosave_snapshot_lines_total_p95` | `36009` | `36009` | `36009` | `12000` | `12000` |
| `autosave_snapshot_build_p95_us` | `5647` | `5320` | `5335` | `1785` | `1784` |
| `baseline_total_send_p95_us` | `24` | `22` | `22` | `25` | `25` |
| `render_total_send_p95_us` | `25` | `25` | `23` | `25` | `26` |
| `full_total_send_p95_us` | `27` | `23` | `24` | `27` | `27` |

`round_bounds_extract_p95_us` is not compared across all milestones because probe semantics changed
after Milestone 1.

## Net Delta (Baseline -> Final)

| Metric | Baseline | Final | Delta | Classification |
| --- | --- | --- | --- | --- |
| `autosave_pass_p95_us` | `5647` | `1784` | `-68.4%` | Significant improvement |
| `autosave_snapshot_build_p95_us` | `5647` | `1784` | `-68.4%` | Significant improvement |
| `autosave_snapshot_lines_total_p95` | `36009` | `12000` | `-66.7%` | Significant improvement |
| `ui_rows_rendered_per_refresh_p95` | `1536` | `1008` | `-34.4%` | Significant improvement |
| `ui_row_render_pass_p95_us` | `1284` | `801` | `-37.6%` | Significant improvement |
| `render_pass_p95_us` | `4429` | `4065` | `-8.2%` | Neutral improvement |
| `baseline_total_send_p95_us` | `24` | `25` | `+4.2%` | Neutral |
| `render_total_send_p95_us` | `25` | `26` | `+4.0%` | Neutral |
| `full_total_send_p95_us` | `27` | `27` | `0.0%` | Neutral |

## Milestone Verdicts

1. Milestone 2 (`perf(render): remove round scans from hot path`)
   - Result: mostly neutral in end-to-end metrics; useful local reduction in orchestrator extraction.
2. Milestone 3 (`perf(render): reduce terminal render window workload`)
   - Result: significant reduction in row-render workload; moderate but sub-threshold end-to-end render gain.
3. Milestone 4 (`perf(autosave): cap periodic terminal history snapshots`)
   - Result: largest measured win; autosave cost and snapshot payload reduced by ~2/3.
4. Milestone 5 final rerun
   - Result: confirms Milestone 4 autosave gains are stable; send-latency regression from the first
     M4 run set did not persist as a significant final regression.

## Decision

- Keep Milestones 3 and 4.
- Keep Milestone 2 as a low-risk cleanup but not a primary performance lever.
- Use current implementation as new measured baseline for future audits.
