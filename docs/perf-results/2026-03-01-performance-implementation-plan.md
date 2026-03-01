# Plan: Gestalt Performance Implementation (Baseline-Driven)

## Objective

Improve Gestalt responsiveness by implementing targeted performance changes and proving impact with before/after measurements.

## Scope

### In Scope

- Correct and harden performance measurement so baseline/after comparisons are trustworthy.
- Reduce UI/render hot-path work (clone/materialization and rerender cost).
- Isolate blocking operations from async/UI-sensitive paths.
- Measure each milestone and produce significance-based recommendations from real data.

### Out of Scope

- New user-facing features unrelated to performance.
- Large architectural rewrites not tied to measured bottlenecks.
- Cross-platform packaging/release changes.

## Inputs

### Problem

The app feels sluggish. Current profiling indicates low terminal send latency, but audit findings show likely UI/render bottlenecks and a profiler correctness gap that can hide real contention.

### Constraints

- Preserve existing behavior and UX.
- Keep changes maintainable and aligned with current architecture standards.
- Use measured deltas for decisions; no optimization-by-guessing.
- Use the same environment/protocol for before and after comparisons.

### Assumptions

- PTY profiling remains available for repeated benchmark runs.
- Perceived sluggishness is primarily from UI/render and synchronization overhead, not PTY write latency.
- 10-run samples are enough to establish stable medians/p95 for decision making.

### Dependencies

- Baseline report: [`docs/perf-results/2026-03-01-024409-baseline.md`](/media/jeremy/OrangeCream/Linux Software/Gestalt/docs/perf-results/2026-03-01-024409-baseline.md)
- Raw run data: [`.perf/2026-03-01-024409-profile-terminal.txt`](/media/jeremy/OrangeCream/Linux Software/Gestalt/.perf/2026-03-01-024409-profile-terminal.txt)
- Process guide: [`docs/PERFORMANCE-RECOMMENDATIONS.md`](/media/jeremy/OrangeCream/Linux Software/Gestalt/docs/PERFORMANCE-RECOMMENDATIONS.md)

### Risks

| Risk | Impact | Mitigation |
| ---- | ------ | ---------- |
| Profiler fixes change metric scale and invalidate prior thresholds | High | Treat current baseline as provisional; capture corrected baseline immediately after Milestone 1 |
| Performance change introduces behavior regressions | High | Keep milestones small; run tests and targeted manual checks per milestone |
| Improvements in one scenario regress another | Medium | Compare all tracked scenarios (baseline/render/render+autosave) before merging milestone |
| Noise from environment variance obscures real deltas | Medium | Use 10-run samples and median+p95 significance criteria |

## Definition of Done

- Measurement harness is trustworthy for before/after comparisons.
- Each implemented milestone has a recorded after-measurement file.
- Final report includes baseline vs final deltas and significance decisions.
- At least one high-priority bottleneck shows significant improvement (>=10% p95 improvement with meaningful absolute gain) without introducing major regressions.
- Recommendations are updated based on observed results, not assumptions.

## Milestones

### Milestone 1: Measurement Hardening

**Goal:** Ensure benchmark numbers reflect real contention and workload.

**Tasks:**
- [ ] Fix `profile_terminal` lock-wait measurement so it captures actual wait/hold costs.
- [ ] Fix warmup readiness criteria so target history load is genuinely reached before sampling.
- [ ] Add/standardize machine-readable output parsing for repeated-run summaries.
- [ ] Capture corrected 10-run baseline (`before-v2`) using identical environment metadata.

**Verification:**
- `cargo run --quiet --bin profile_terminal -- --assert`
- 10-run baseline capture with median/p95 summary saved under `docs/perf-results/`
- Sanity-check that measured values change when synthetic contention is increased

**Status:** Complete

### Milestone 2: Render Data-Path De-duplication

**Goal:** Reduce repeated full-buffer cloning/materialization in workspace/orchestrator paths.

**Tasks:**
- [ ] Remove redundant snapshot/line cloning between workspace and orchestrator views.
- [ ] Introduce revision-aware sharing/reuse of runtime view data for active sessions.
- [ ] Keep latest-round extraction work bounded to required panes only.

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- 10-run milestone measurement with baseline comparison

**Status:** Complete

### Milestone 3: Terminal Render Work Reduction

**Goal:** Lower per-refresh terminal UI rendering cost.

**Tasks:**
- [ ] Reduce per-row allocation/cloning in terminal line render pipeline.
- [ ] Introduce viewport-based rendering/windowing for terminal lines.
- [ ] Ensure focused-caret and selection behavior remain correct after render changes.

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- Manual UX check: typing, scrollback, selection, insert-mode behavior
- 10-run milestone measurement with baseline comparison

**Status:** Complete

### Milestone 4: Async/UI Path Isolation

**Goal:** Prevent blocking or side-effect-heavy work from degrading UI responsiveness.

**Tasks:**
- [ ] Move session startup side effects out of render paths into controlled lifecycle hooks.
- [ ] Isolate Git refresh loading (`git` command fan-out) from async/UI-sensitive loop via blocking worker strategy.
- [ ] Revisit polling cadence and event-driven triggers for refresh paths.

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- Manual UX check: active-group switches, Git panel refresh, startup behavior
- 10-run milestone measurement with baseline comparison

**Status:** Complete

### Milestone 5: Final Measurement and Recommendation Review

**Goal:** Quantify total impact and update recommendations from evidence.

**Tasks:**
- [x] Run final 10-run benchmark suite in the same environment.
- [x] Produce consolidated before/after table (median, p95, p99, max).
- [x] Classify each milestone as significant/neutral/regression.
- [x] Update [`docs/PERFORMANCE-RECOMMENDATIONS.md`](/media/jeremy/OrangeCream/Linux Software/Gestalt/docs/PERFORMANCE-RECOMMENDATIONS.md) with what worked vs did not.

**Verification:**
- Final report saved under `docs/perf-results/`
- All code-quality gates pass
- Recommendations file updated with data-backed ranking

**Status:** Complete

## Execution Notes

Update during implementation:
- 2026-03-01: Plan created from initial baseline and audit findings.
- 2026-03-01: Milestone 1 completed. Profiling harness corrected and baseline-v2 captured.
- 2026-03-01: Milestone 2 completed. Snapshot cloning reduced and runtime sharing introduced.
- 2026-03-01: Milestone 3 completed. Terminal rendering path reduced cloning and added row windowing.
- 2026-03-01: Milestone 4 completed. Startup session initialization moved off render path and Git refresh loading isolated via blocking worker.
- 2026-03-01: Milestone 5 completed. Final 10-run benchmark captured and significance review documented; recommendations updated from measured data.

## Commit Cadence Notes

- Commit each completed milestone slice only after verification.
- Keep performance changes and measurement tooling changes in separate commits when practical.

## Re-Plan Triggers

- Corrected baseline contradicts initial bottleneck ranking.
- A milestone shows neutral/negative impact after 10-run measurement.
- A regression in terminal behavior, Git workflows, or startup lifecycle is found.
- Scope grows beyond measurable bottleneck-driven changes.

## Recommendations (Only If Better Option Exists)

- Recommendation 1: If Milestone 2 and 3 changes become tightly coupled, use a short-lived feature flag branch to isolate and A/B test each change independently. Impact: slightly longer timeline, higher attribution confidence.
- Recommendation 2: If benchmark variance remains high after 10 runs, raise sample count to 20 for final comparison only. Impact: longer measurement cycle, stronger significance confidence.

## Completion Summary

### Completed

- Milestone 1: Measurement Hardening
- Milestone 2: Render Data-Path De-duplication
- Milestone 3: Terminal Render Work Reduction
- Milestone 4: Async/UI Path Isolation
- Milestone 5: Final Measurement and Recommendation Review

### Deviations

- N/A

### Follow-Ups

- Add frame-time focused instrumentation and benchmarks for render/autosave workloads.
- Add CI/perf gating so regressions are caught before merge.

### Verification Summary

- Baseline captured (10 runs): [`docs/perf-results/2026-03-01-024409-baseline.md`](/media/jeremy/OrangeCream/Linux Software/Gestalt/docs/perf-results/2026-03-01-024409-baseline.md)
- Corrected baseline-v2 (10 runs): [`docs/perf-results/2026-03-01-025300-baseline-v2.md`](/media/jeremy/OrangeCream/Linux Software/Gestalt/docs/perf-results/2026-03-01-025300-baseline-v2.md)
- Milestone 2 measurement (10 runs): [`docs/perf-results/2026-03-01-025719-milestone-2.md`](/media/jeremy/OrangeCream/Linux Software/Gestalt/docs/perf-results/2026-03-01-025719-milestone-2.md)
- Milestone 3 measurement (10 runs): [`docs/perf-results/2026-03-01-030134-milestone-3.md`](/media/jeremy/OrangeCream/Linux Software/Gestalt/docs/perf-results/2026-03-01-030134-milestone-3.md)
- Milestone 4 measurement (10 runs): [`docs/perf-results/2026-03-01-030953-milestone-4.md`](/media/jeremy/OrangeCream/Linux Software/Gestalt/docs/perf-results/2026-03-01-030953-milestone-4.md)
- Final measurement and review (10 runs): [`docs/perf-results/2026-03-01-074955-final.md`](/media/jeremy/OrangeCream/Linux Software/Gestalt/docs/perf-results/2026-03-01-074955-final.md)

### Traceability Links

- Module README updated: N/A
- ADR added/updated: N/A
- PR notes completed per `templates/PULL_REQUEST_TEMPLATE.md`: Pending
