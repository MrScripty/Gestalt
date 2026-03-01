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

## 2026-03-01 Terminal Render/Refresh Audit (Latest)

Data sources:
- Baseline: `docs/perf-results/2026-03-01-082534-render-instrumentation-baseline.md`
- Milestone 2: `docs/perf-results/2026-03-01-084224-milestone-2-round-scan-removal.md`
- Milestone 3: `docs/perf-results/2026-03-01-084627-milestone-3-render-window.md`
- Milestone 4: `docs/perf-results/2026-03-01-090832-milestone-4-autosave-history-cap.md`
- Final comparison: `docs/perf-results/2026-03-01-091603-milestone-5-final-comparison.md`

### Net p95 Delta (Baseline -> Final)

| Metric | Baseline | Final | Delta | Result |
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

## What Worked

1. Reducing render window workload (Milestone 3) materially reduced rows processed per refresh and row-render pass cost.
2. Capping periodic autosave history to `4000` lines/session (Milestone 4) delivered the largest win and remained stable in final reruns.
3. Keeping full-fidelity save on shutdown preserved clean-exit persistence quality while reducing periodic autosave overhead.

## What Did Not Work

1. Removing round scans from hot path (Milestone 2) was directionally correct but did not deliver a large end-to-end p95 win on its own.
2. Send-latency metrics were noisy and are not a reliable primary proxy for render smoothness; improvements came from render/autosave workload metrics instead.

## Re-Ranked Priorities (Evidence-Based)

1. Keep the autosave history-cap path and tune cap by UX/recovery requirements (`4000` is current measured sweet spot).
2. Maintain row-render workload controls (window sizing) and avoid reintroducing full-window redraw pressure.
3. Add event-driven/coalesced refresh work next (poll-loop reduction remains the largest unimplemented audit item).
4. Keep render/autosave suspect metrics in routine perf checks, not just send-latency metrics.

## Standards Shortcomings Revealed By Data

1. `FRONTEND-STANDARDS.md` encourages event-driven sync but does not enforce polling budgets.
   Impact: frequent global polling loops can survive review and reintroduce UI work churn.
   Recommendation: add mandatory per-loop budget and justification requirements.

2. `CODING-STANDARDS.md` and `TESTING-STANDARDS.md` discuss profiling but do not define required UI budgets.
   Impact: regressions can pass review without explicit render/autosave thresholds.
   Recommendation: add explicit p95 budgets for render pass, autosave pass, and row workload.

3. `TOOLING-STANDARDS.md` lacks a mandatory perf gate.
   Impact: CI can pass while user-visible performance regresses.
   Recommendation: require a benchmark gate using `profile_terminal` summaries with threshold checks.

4. Persistence standards do not separate autosave durability policy from clean-shutdown fidelity.
   Impact: autosave previously defaulted to full-history snapshots, causing avoidable periodic cost.
   Recommendation: formalize dual policy in standards: bounded periodic autosave + full-fidelity explicit save path.

5. Benchmark protocol standardization is incomplete for probe comparability.
   Impact: some metric probe semantics changed between milestones (`round_bounds_extract`) and required manual interpretation.
   Recommendation: require probe-definition changelog and comparability notes in every perf milestone report.
