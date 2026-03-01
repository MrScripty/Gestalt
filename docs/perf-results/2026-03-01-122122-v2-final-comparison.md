# Final Comparison 2026-03-01-122122 (v2)

## Metadata

- baseline commit: `e5c23347d76cd9aa5973ed2383661bbfaf3d6020`
- final commit: `e0b4c1810df2531d9055474cfe90df219a626f53`
- baseline log: `.perf/2026-03-01-091848-profile-terminal-v2-plan-baseline.txt`
- final log: `.perf/2026-03-01-122122-profile-terminal-v2-final.txt`
- runs: `10` baseline + `10` final
- discarded run (invalid due sandbox PTY permissions): `.perf/2026-03-01-122105-profile-terminal-v2-final.txt`

## Milestone Classification

| milestone | primary target | before p95 | after p95 | delta | classification |
| --- | --- | ---:| ---:| ---:| --- |
| 1 | `refresh_loop_tick_p95_us` | 21 | 18 | -14.3% | Significant improvement |
| 2 | `resize_measure_p95_us` | 3 | 0 | -100.0% | Significant improvement |
| 3 | `scroll_observer_callbacks_per_sec_p95` | 90 | 45 | -50.0% | Significant improvement |
| 4 (attempt) | `render_pass_p95_us` | 3974 | 6116 | +53.9% | Regression (reverted) |
| 5 | `autosave_fingerprint_p95_us` | 41347 | 0 | -100.0% | Significant improvement |
| 6 | `git_watcher_poll_cost_p95_us` | 10375 | 4558 | -56.1% | Significant improvement |
| 7 | consolidated rerun | n/a | n/a | n/a | Validation complete |

## Baseline vs Final (p95)

| metric | baseline p95 | final p95 | delta | result |
| --- | ---:| ---:| ---:| --- |
| `refresh_loop_tick_p95_us` | 21 | 15 | -28.6% | Significant improvement |
| `resize_measure_p95_us` | 3 | 0 | -100.0% | Significant improvement |
| `scroll_observer_callbacks_per_sec_p95` | 90 | 45 | -50.0% | Significant improvement |
| `autosave_fingerprint_p95_us` | 41785 | 0 | -100.0% | Significant improvement |
| `git_watcher_poll_cost_p95_us` | 10248 | 4486 | -56.2% | Significant improvement |
| `autosave_pass_p95_us` | 1837 | 1771 | -3.6% | Neutral improvement |
| `render_pass_p95_us` | 4036 | 4149 | +2.8% | Neutral drift |
| `baseline_total_send_p95_us` | 26 | 27 | +3.8% | Neutral |
| `render_total_send_p95_us` | 27 | 27 | 0.0% | Neutral |
| `full_total_send_p95_us` | 30 | 26 | -13.3% | Significant improvement |

## Outcome

- v2 cycle achieved its main goals on refresh loop, resize measurement, scroll callback pressure,
  autosave fingerprinting, and git watcher poll cost.
- Milestone 4 was correctly identified as a regression and reverted.
- Final rerun confirms no persistent send-latency regression from Milestone 6.
- Remaining highest-impact unresolved area is render-path object/snapshot work, but this should be
  reevaluated after the SurrealDB snapshot migration lands.

## Commands

```bash
for i in $(seq 1 10); do
  echo "=== run $i ==="
  cargo run --quiet --bin profile_terminal -- --assert --json
done | tee .perf/2026-03-01-122122-profile-terminal-v2-final.txt

./scripts/perf-summary.sh .perf/2026-03-01-091848-profile-terminal-v2-plan-baseline.txt \
  render_pass_p95_us autosave_pass_p95_us refresh_loop_tick_p95_us \
  resize_measure_p95_us scroll_observer_callbacks_per_sec_p95 \
  autosave_fingerprint_p95_us git_watcher_poll_cost_p95_us \
  baseline_total_send_p95_us render_total_send_p95_us full_total_send_p95_us

./scripts/perf-summary.sh .perf/2026-03-01-122122-profile-terminal-v2-final.txt \
  render_pass_p95_us autosave_pass_p95_us refresh_loop_tick_p95_us \
  resize_measure_p95_us scroll_observer_callbacks_per_sec_p95 \
  autosave_fingerprint_p95_us git_watcher_poll_cost_p95_us \
  baseline_total_send_p95_us render_total_send_p95_us full_total_send_p95_us
```
