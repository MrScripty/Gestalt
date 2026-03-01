# Milestone 6 Results 2026-03-01-121706

## Metadata

- baseline commit: `51f7a06b4be5236cd9c2050ccac04e0ed73f8f2a`
- baseline log: `.perf/2026-03-01-120811-profile-terminal-v2-m5-autosave.txt`
- milestone 6 log: `.perf/2026-03-01-121706-profile-terminal-v2-m6-gitwatcher.txt`
- runs: `10`

## Scope

Milestone 6 reduced git watcher polling overhead by resolving repository root once
and reusing a root-based fingerprint call in the watcher loop.

## Baseline vs After (p95)

| metric | baseline p95 | milestone 6 p95 | delta |
| --- | ---:| ---:| ---:|
| `git_watcher_poll_cost_p95_us` | 10375 | 4558 | -56.1% |
| `render_pass_p95_us` | 4110 | 3998 | -2.7% |
| `autosave_pass_p95_us` | 1776 | 1798 | +1.2% |
| `refresh_loop_tick_p95_us` | 15 | 15 | 0.0% |
| `scroll_observer_callbacks_per_sec_p95` | 45 | 45 | 0.0% |
| `baseline_total_send_p95_us` | 27 | 28 | +3.7% |
| `render_total_send_p95_us` | 26 | 27 | +3.8% |
| `full_total_send_p95_us` | 26 | 29 | +11.5% |
| `autosave_fingerprint_p95_us` | 0 | 0 | 0.0% |

## Outcome

- **Classification:** Significant improvement (target metric met)
- `git_watcher_poll_cost_p95_us` improved by `56.1%`.
- `full_total_send_p95_us` rose by `3us` (`+11.5%`), so this should be revalidated in
  the final consolidated rerun.

## Commands

```bash
for i in $(seq 1 10); do
  echo "=== run $i ==="
  cargo run --quiet --bin profile_terminal -- --assert --json
done | tee .perf/2026-03-01-121706-profile-terminal-v2-m6-gitwatcher.txt

./scripts/perf-summary.sh .perf/2026-03-01-120811-profile-terminal-v2-m5-autosave.txt \
  render_pass_p95_us autosave_pass_p95_us baseline_total_send_p95_us \
  render_total_send_p95_us full_total_send_p95_us refresh_loop_tick_p95_us \
  scroll_observer_callbacks_per_sec_p95 git_watcher_poll_cost_p95_us \
  autosave_fingerprint_p95_us

./scripts/perf-summary.sh .perf/2026-03-01-121706-profile-terminal-v2-m6-gitwatcher.txt \
  render_pass_p95_us autosave_pass_p95_us baseline_total_send_p95_us \
  render_total_send_p95_us full_total_send_p95_us refresh_loop_tick_p95_us \
  scroll_observer_callbacks_per_sec_p95 git_watcher_poll_cost_p95_us \
  autosave_fingerprint_p95_us
```
