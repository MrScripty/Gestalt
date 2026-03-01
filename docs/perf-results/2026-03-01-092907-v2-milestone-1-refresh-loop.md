# Milestone 1 Results 2026-03-01-092907

## Metadata

- baseline commit: `e5c233406bef0d8f71cf84990ef64441514acdc9`
- rustc: `rustc 1.92.0 (ded5c06cf 2025-12-08)`
- cargo: `cargo 1.92.0 (344c4567c 2025-10-21)`
- kernel: `Linux 6.17.0-14-generic x86_64 GNU/Linux`
- cpu: `Intel(R) Core(TM) Ultra 9 275HX`
- runs: `10`
- baseline log: `.perf/2026-03-01-091848-profile-terminal-v2-plan-baseline.txt`
- milestone 1 log: `.perf/2026-03-01-092907-profile-terminal-v2-m1-refresh.txt`

## Scope

Milestone 1 focused on refresh/resize loop efficiency by removing full `AppState` clones
from hot polling paths and switching to lightweight session-id projections.

## Baseline vs After (p95)

| metric | baseline p95 | milestone 1 p95 | delta |
| --- | ---:| ---:| ---:|
| `refresh_loop_tick_p95_us` | 21 | 18 | -14.3% |
| `refresh_loop_state_clone_p95_us` | 0 | 0 | n/a |
| `render_pass_p95_us` | 4036 | 3972 | -1.6% |
| `autosave_pass_p95_us` | 1837 | 1804 | -1.8% |
| `orchestrator_snapshot_build_p95_us` | 10 | 10 | 0.0% |
| `resize_measure_p95_us` | 3 | 3 | 0.0% |
| `resize_measure_calls_per_sec_p95` | 15 | 15 | 0.0% |
| `scroll_observer_callbacks_per_sec_p95` | 90 | 90 | 0.0% |
| `autosave_fingerprint_p95_us` | 41785 | 41172 | -1.5% |
| `git_watcher_poll_cost_p95_us` | 10248 | 10519 | +2.6% |
| `baseline_total_send_p95_us` | 26 | 28 | +7.7% |
| `render_total_send_p95_us` | 27 | 28 | +3.7% |
| `full_total_send_p95_us` | 30 | 27 | -10.0% |

## Outcome

- **Classification:** Significant improvement (target metric met)
- `refresh_loop_tick_p95_us` improved by `14.3%` (meets >=10% threshold).
- No major cross-metric regression observed (no key metric worsened by >=10%).
- Minor variance increases in watcher and send metrics remained below regression threshold.

## Commands

```bash
for i in $(seq 1 10); do
  echo "=== run $i ==="
  cargo run --quiet --bin profile_terminal -- --assert --json
done | tee .perf/2026-03-01-092907-profile-terminal-v2-m1-refresh.txt

./scripts/perf-summary.sh .perf/2026-03-01-091848-profile-terminal-v2-plan-baseline.txt \
  render_pass_p95_us autosave_pass_p95_us refresh_loop_tick_p95_us \
  refresh_loop_state_clone_p95_us resize_measure_p95_us \
  resize_measure_calls_per_sec_p95 scroll_observer_callbacks_per_sec_p95 \
  orchestrator_snapshot_build_p95_us autosave_fingerprint_p95_us \
  git_watcher_poll_cost_p95_us baseline_total_send_p95_us \
  render_total_send_p95_us full_total_send_p95_us

./scripts/perf-summary.sh .perf/2026-03-01-092907-profile-terminal-v2-m1-refresh.txt \
  render_pass_p95_us autosave_pass_p95_us refresh_loop_tick_p95_us \
  refresh_loop_state_clone_p95_us resize_measure_p95_us \
  resize_measure_calls_per_sec_p95 scroll_observer_callbacks_per_sec_p95 \
  orchestrator_snapshot_build_p95_us autosave_fingerprint_p95_us \
  git_watcher_poll_cost_p95_us baseline_total_send_p95_us \
  render_total_send_p95_us full_total_send_p95_us
```
