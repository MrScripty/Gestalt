# Milestone 4 Regression Report 2026-03-01-114353

## Metadata

- baseline commit: `fdd44c88098f2b6cb99d8e4a5059cfe3f85ef509`
- baseline log: `.perf/2026-03-01-101557-profile-terminal-v2-m3-scroll.txt`
- attempted log: `.perf/2026-03-01-114353-profile-terminal-v2-m4-orchestrator.txt`
- runs: `10`

## Attempt Summary

Milestone 4 attempted to reduce render-path cloning by:

- avoiding full `AppState` clone in `src/ui/workspace.rs`
- reducing session cloning in `src/orchestrator/runtime.rs`
- reducing session cloning in `profile_terminal` orchestrator probe

## Baseline vs Attempt (p95)

| metric | baseline p95 | attempted p95 | delta |
| --- | ---:| ---:| ---:|
| `orchestrator_snapshot_build_p95_us` | 9 | 9 | 0.0% |
| `render_pass_p95_us` | 3974 | 6116 | +53.9% |
| `autosave_pass_p95_us` | 1817 | 2269 | +24.9% |
| `round_bounds_extract_p95_us` | 3072 | 4688 | +52.6% |
| `orchestrator_round_extract_p95_us` | 18 | 21 | +16.7% |
| `baseline_total_send_p95_us` | 28 | 29 | +3.6% |
| `render_total_send_p95_us` | 28 | 28 | 0.0% |
| `full_total_send_p95_us` | 27 | 27 | 0.0% |

## Outcome

- **Classification:** Regression
- Key p95 metrics regressed well beyond the >=10% threshold.
- Milestone 4 code changes were reverted locally and not committed.

## Decision

- Keep Milestone 3 code as the current performance baseline.
- Do not proceed with this Milestone 4 approach in its current form.
