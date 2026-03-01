# Terminal Render/Refresh Baseline 2026-03-01-081702

## Metadata
- commit: `53199d2e68bf8ff90a4e2560a73eb7218f679051`
- rustc: `rustc 1.92.0 (ded5c06cf 2025-12-08)`
- cargo: `cargo 1.92.0 (344c4567c 2025-10-21)`
- kernel: `Linux 6.17.0-14-generic #14~24.04.1-Ubuntu SMP PREEMPT_DYNAMIC Thu Jan 15 15:52:10 UTC 2 x86_64 GNU/Linux`
- cpu: `Intel(R) Core(TM) Ultra 9 275HX`
- profiler runs: `10`

## Commands Run

```bash
# 10-run baseline on current harness
for i in $(seq 1 10); do
  cargo run --quiet --bin profile_terminal -- --assert --json
done | tee .perf/2026-03-01-081702-profile-terminal-render-baseline.txt

# aggregate summary
scripts/perf-summary.sh .perf/2026-03-01-081702-profile-terminal-render-baseline.txt

# system-level CPU/memory fallback baseline
/usr/bin/time -v cargo run --quiet --bin profile_terminal -- --assert --json \
  > .perf/2026-03-01-081702-timev-profile-terminal.txt 2>&1

# attempted perf stat
perf stat -d -- cargo run --quiet --bin profile_terminal -- --assert --json \
  > .perf/2026-03-01-081702-perf-stat-profile-terminal.txt 2>&1
```

## Aggregated Summary (10 Runs)

```text
metric                               runs     median      p95      p99      min      max
---------------------------------- ------ ---------- -------- -------- -------- --------
render_pass_p95_us                     10     9566.5    10610    10610     8613    10843
autosave_pass_p95_us                   10     7148.0     7226     7226     6985     8541
baseline_total_send_p95_us             10       23.0       25       25       21       27
render_total_send_p95_us               10       23.0       24       24       20       24
full_total_send_p95_us                 10       23.5       25       25       21       25
```

## /usr/bin/time -v Baseline (single representative run)

Source: `.perf/2026-03-01-081702-timev-profile-terminal.txt`

- user time: `9.57s`
- system time: `1.10s`
- cpu utilization: `85%`
- elapsed wall time: `12.45s`
- max RSS: `212,204 KB`
- voluntary context switches: `5,223`
- involuntary context switches: `196`

## perf stat Status

`perf stat` was attempted but blocked by kernel policy in this environment:
- `perf_event_paranoid = 4`
- error captured in `.perf/2026-03-01-081702-perf-stat-profile-terminal.txt`

## Artifacts
- `.perf/2026-03-01-081702-profile-terminal-render-baseline.txt`
- `.perf/2026-03-01-081702-timev-profile-terminal.txt`
- `.perf/2026-03-01-081702-perf-stat-profile-terminal.txt`
