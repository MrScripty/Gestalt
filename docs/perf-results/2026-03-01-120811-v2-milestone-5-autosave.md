# Milestone 5 Results 2026-03-01-120811

## Metadata

- baseline commit: `f5625847af68d972c8faeb6584ac5752c98c8f83`
- baseline log: `.perf/2026-03-01-101557-profile-terminal-v2-m3-scroll.txt`
- milestone 5 log: `.perf/2026-03-01-120811-profile-terminal-v2-m5-autosave.txt`
- runs: `10`

## Scope

Milestone 5 reduced autosave fingerprint overhead by moving fingerprint hashing and
dedupe decisions out of the UI autosave loop and into the autosave worker thread.

## Baseline vs After (p95)

| metric | baseline p95 | milestone 5 p95 | delta |
| --- | ---:| ---:| ---:|
| `autosave_fingerprint_p95_us` | 41347 | 0 | -100.0% |
| `autosave_pass_p95_us` | 1817 | 1776 | -2.3% |
| `render_pass_p95_us` | 3974 | 4110 | +3.4% |
| `refresh_loop_tick_p95_us` | 15 | 15 | 0.0% |
| `orchestrator_snapshot_build_p95_us` | 9 | 9 | 0.0% |
| `scroll_observer_callbacks_per_sec_p95` | 45 | 45 | 0.0% |
| `baseline_total_send_p95_us` | 28 | 27 | -3.6% |
| `render_total_send_p95_us` | 28 | 26 | -7.1% |
| `full_total_send_p95_us` | 27 | 26 | -3.7% |

## Outcome

- **Classification:** Significant improvement (target metric met)
- `autosave_fingerprint_p95_us` improved by `100.0%`.
- No key metric regressed by >=10%.

## Commands

```bash
for i in $(seq 1 10); do
  echo "=== run $i ==="
  cargo run --quiet --bin profile_terminal -- --assert --json
done | tee .perf/2026-03-01-120811-profile-terminal-v2-m5-autosave.txt

./scripts/perf-summary.sh .perf/2026-03-01-101557-profile-terminal-v2-m3-scroll.txt \
  render_pass_p95_us autosave_pass_p95_us autosave_fingerprint_p95_us \
  refresh_loop_tick_p95_us resize_measure_p95_us \
  scroll_observer_callbacks_per_sec_p95 orchestrator_snapshot_build_p95_us \
  baseline_total_send_p95_us render_total_send_p95_us full_total_send_p95_us

./scripts/perf-summary.sh .perf/2026-03-01-120811-profile-terminal-v2-m5-autosave.txt \
  render_pass_p95_us autosave_pass_p95_us autosave_fingerprint_p95_us \
  refresh_loop_tick_p95_us resize_measure_p95_us \
  scroll_observer_callbacks_per_sec_p95 orchestrator_snapshot_build_p95_us \
  baseline_total_send_p95_us render_total_send_p95_us full_total_send_p95_us
```
