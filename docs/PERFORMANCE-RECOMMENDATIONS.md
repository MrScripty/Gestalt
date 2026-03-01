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
