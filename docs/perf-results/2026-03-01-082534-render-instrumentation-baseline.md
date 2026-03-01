# Render Instrumentation Baseline 2026-03-01-082534

## Metadata
- commit: `9e894bf23241d688edb608783fd5b9ada510ae67`
- rustc: `rustc 1.92.0 (ded5c06cf 2025-12-08)`
- cargo: `cargo 1.92.0 (344c4567c 2025-10-21)`
- kernel: `Linux 6.17.0-14-generic #14~24.04.1-Ubuntu SMP PREEMPT_DYNAMIC Thu Jan 15 15:52:10 UTC 2 x86_64 GNU/Linux`
- cpu: `Intel(R) Core(TM) Ultra 9 275HX`
- runs: `10`
- raw log: `.perf/2026-03-01-082534-profile-terminal-render-instrumented-baseline.txt`

## Scope

Milestone 1 instrumentation baseline for terminal render/refresh suspects using extended
`profile_terminal` metrics.

Note:
- This baseline includes new render-path instrumentation and a render simulation aligned to
  current runtime/orchestrator data flow. Numbers are not directly comparable to earlier
  pre-instrumentation render-pass measurements.

## Aggregated Summary (Existing Metrics)

```text
metric                               runs     median      p95      p99      min      max
---------------------------------- ------ ---------- -------- -------- -------- --------
render_pass_p95_us                     10     3557.5     4429     4429     3358     4543
autosave_pass_p95_us                   10     5317.5     5647     5647     5268     5737
baseline_total_send_p95_us             10       21.5       24       24       17       27
render_total_send_p95_us               10       20.0       25       25       17       26
full_total_send_p95_us                 10       21.5       27       27       18       27
```

## Aggregated Summary (New Suspect Metrics)

```text
metric                               runs     median      p95      p99      min      max
---------------------------------- ------ ---------- -------- -------- -------- --------
ui_rows_rendered_per_refresh_p95       10     1536.0     1536     1536     1536     1536
ui_row_render_pass_p95_us              10      951.0     1284     1284      805     1324
round_bounds_extract_p95_us            10        3.0        4        4        3        4
orchestrator_round_extract_p95_us      10        9.0       15       15        7       15
autosave_snapshot_lines_total_p95      10    36009.0    36009    36009    36009    36009
autosave_snapshot_build_p95_us         10     5317.5     5647     5647     5268     5737
```

## Commands

```bash
for i in $(seq 1 10); do
  cargo run --quiet --bin profile_terminal -- --assert --json
done | tee .perf/2026-03-01-082534-profile-terminal-render-instrumented-baseline.txt

scripts/perf-summary.sh .perf/2026-03-01-082534-profile-terminal-render-instrumented-baseline.txt
scripts/perf-summary.sh .perf/2026-03-01-082534-profile-terminal-render-instrumented-baseline.txt \
  ui_rows_rendered_per_refresh_p95 \
  ui_row_render_pass_p95_us \
  round_bounds_extract_p95_us \
  orchestrator_round_extract_p95_us \
  autosave_snapshot_lines_total_p95 \
  autosave_snapshot_build_p95_us
```

## Initial Readout

- Rendered rows are fixed at `1536` p95 in this baseline (3 terminals x 512-row window).
- Row-render work p95 (`1284us`) is substantially larger than round-bound extraction p95 (`4us`).
- Autosave snapshot line volume is constant at `36009` lines per pass and remains expensive (`5647us` p95).
- Next milestone should target row-render pass and autosave snapshot payload reduction first.
