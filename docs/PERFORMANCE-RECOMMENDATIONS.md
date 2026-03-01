# Performance Recommendations

This document defines how we will improve Gestalt performance using measured data, not assumptions.

## Objective

Improve perceived responsiveness and throughput while keeping behavior correct and maintainable.

## Operating Rule

No performance change is considered successful unless we have:

1. A measured baseline (`before`) from the same environment.
2. A measured result (`after`) from the same test protocol.
3. A delta analysis showing what changed and by how much.

## Workflow

## 1. Capture Baseline (Before)

Do this before any performance code changes.

- Record metadata:
- Date/time
- Commit SHA
- OS + kernel
- CPU model
- Build profile (`debug`/`release`)
- Scenario notes (number of sessions, terminal history size, active repo size)

- Run the current terminal harness:
- `cargo run --quiet --bin profile_terminal -- --assert`

- Run each measurement multiple times:
- Minimum: 5 runs
- Recommended: 10 runs
- Report median, p95, p99, max

- Store baseline report in:
- `docs/perf-results/<YYYY-MM-DD>-baseline.md`

## 2. Create the Improvement Plan

Create a plan with small, testable milestones. Each milestone should target one primary bottleneck.

Required per milestone:

- Hypothesis (what should improve and why)
- Scope (files/modules)
- Expected metric impact
- Risk/regression notes
- Verification commands

## 3. Implement in Small Slices

- Change one bottleneck at a time.
- Keep commits focused and reversible.
- Avoid bundling unrelated refactors with perf changes.

## 4. Measure After Each Slice

After each milestone:

- Re-run the exact same benchmark protocol as baseline.
- Keep environment as close as possible.
- Save results to:
- `docs/perf-results/<YYYY-MM-DD>-milestone-<N>.md`

## 5. Determine Significance

Use both absolute and relative change.

- Significant improvement:
- p95 improves by at least 10% and by a meaningful absolute amount
- no major regression in other tracked metrics

- Neutral:
- change is within normal run-to-run variance

- Regression:
- p95 worsens by at least 10% or introduces new UX-visible lag

If results are ambiguous, run more samples before deciding.

## 6. Review and Update Recommendations

Once implementation is complete:

- Compare final metrics against baseline.
- Rank changes by measured impact.
- Keep recommendations that produced meaningful wins.
- Demote or remove recommendations that did not move metrics.

Update this file with:

- What worked
- What did not
- What remains highest-impact next

## Result Template

Use this structure in each results file:

```md
# Perf Result - <label>

## Metadata
- commit:
- date:
- environment:
- scenario:

## Metrics
| Metric | Before (median/p95) | After (median/p95) | Delta | Significance |
|---|---|---|---|---|
| terminal total-send us |  |  |  |  |
| render pass us |  |  |  |  |
| autosave pass us |  |  |  |  |

## Notes
- Observations:
- Regressions:
- Decision:
```

## Current Priority Areas (From Audit)

Initial targets to validate with data:

1. Reduce repeated full-buffer cloning in workspace/orchestrator render paths.
2. Reduce terminal UI render cost (avoid re-rendering full line sets when unchanged).
3. Move blocking Git refresh work off async/UI-sensitive paths.
4. Remove side-effectful startup work from render paths.

These priorities must be re-ranked after baseline and milestone measurements.

## 2026-03-01 Measured Review

Data source:
- Baseline: `docs/perf-results/2026-03-01-025300-baseline-v2.md`
- Final: `docs/perf-results/2026-03-01-074955-final.md`

| Metric | Baseline (median/p95/p99/max) | Final (median/p95/p99/max) | p95 delta | Result |
| --- | --- | --- | --- | --- |
| `baseline_total_send_p95_us` | `25.0 / 28 / 28 / 29` | `22.0 / 25 / 25 / 27` | `-10.7%` | Significant improvement |
| `render_total_send_p95_us` | `25.5 / 27 / 27 / 27` | `24.0 / 25 / 25 / 26` | `-7.4%` | Neutral |
| `full_total_send_p95_us` | `25.0 / 27 / 27 / 27` | `22.5 / 25 / 25 / 25` | `-7.4%` | Neutral |
| `render_pass_p95_us` | `9608.5 / 10511 / 10511 / 10527` | `10435.5 / 10694 / 10694 / 10759` | `+1.7%` | Neutral regression |
| `autosave_pass_p95_us` | `6600.5 / 6695 / 6695 / 6726` | `7209.0 / 7252 / 7252 / 7265` | `+8.3%` | Neutral regression |

## What Worked

1. Isolating startup side effects and Git refresh from render-sensitive paths delivered the only clear significant gain (`baseline_total_send_p95_us` improved by `10.7%`).
2. Final combined change set also reduced worst-case p95 totals in all send scenarios, but only one met the significance threshold.

## What Did Not Work

1. Clone/dedupe and terminal windowing milestones did not produce significant p95 wins in this harness.
2. Render/autosave heavy-path metrics remain worse than baseline, so the perceived sluggishness root cause is likely still in render/autosave workloads rather than PTY send latency.

## Re-Ranked Priorities (Evidence-Based)

1. Add render-frame and autosave workload profiling (frame time, lock hold, line materialization counts) as first-class benchmark outputs.
2. Optimize autosave/render heavy paths directly; do not use send-latency deltas alone as proxy for UI smoothness.
3. Keep startup/refresh isolation in place; it is currently the highest-confidence win.

## Standards Shortcomings Revealed By Data

The coding standards discuss profiling and hot paths, but this implementation showed gaps that can still allow performance regressions:

1. No required CI performance gate or budget check.
   Impact: regressions in render/autosave metrics were not blocked.
   Recommendation: add a mandatory perf benchmark gate with tracked budget thresholds.
2. No mandatory render-path side-effect audit checklist.
   Impact: startup and refresh work reached render-adjacent loops before being isolated.
   Recommendation: add a rule that render paths must be side-effect free and non-blocking, with explicit lifecycle ownership.
3. No enforced benchmark protocol schema in standards.
   Impact: baseline correctness had to be repaired midstream (lock-wait and warmup readiness).
   Recommendation: standardize required metadata, warmup criteria, sample count, and significance rules in one benchmark template.
