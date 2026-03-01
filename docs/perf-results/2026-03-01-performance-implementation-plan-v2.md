# Plan: Gestalt Performance Implementation v2 (Post-Audit, Pre-SurrealDB)

## Objective

Improve terminal/UI responsiveness further using measured milestones while **deprioritizing**
the current terminal snapshot clone/merge bottleneck because terminal history is planned to
migrate to SurrealDB soon.

## Explicit Scope Decision

### In Scope

- Refresh/polling loop efficiency
- Resize measurement efficiency
- Scroll observer and DOM mutation handling
- Orchestrator/session-clone reduction in render paths
- Autosave fingerprint/serialization overhead
- Git watcher refresh efficiency
- Measurement instrumentation and before/after reporting

### Out of Scope (for this cycle)

- Large refactor of current terminal history clone/merge pipeline in `src/terminal.rs`
  (planned SurrealDB history migration is expected to supersede this work).

## Baseline Protocol (Before)

Use one stable baseline commit before Milestone 1 code changes.

Commands:

```bash
for i in $(seq 1 10); do
  echo "=== run $i ==="
  cargo run --quiet --bin profile_terminal -- --assert --json
done | tee .perf/2026-03-01-<ts>-v2-plan-baseline.txt

./scripts/perf-summary.sh .perf/2026-03-01-<ts>-v2-plan-baseline.txt \
  render_pass_p95_us autosave_pass_p95_us \
  ui_rows_rendered_per_refresh_p95 ui_row_render_pass_p95_us \
  baseline_total_send_p95_us render_total_send_p95_us full_total_send_p95_us
```

Save report:
- `docs/perf-results/2026-03-01-<ts>-v2-plan-baseline.md`

## Metrics

### Existing Core Metrics

- `render_pass_p95_us`
- `autosave_pass_p95_us`
- `ui_rows_rendered_per_refresh_p95`
- `ui_row_render_pass_p95_us`
- `baseline_total_send_p95_us`
- `render_total_send_p95_us`
- `full_total_send_p95_us`

### New Required Metrics (add in Milestone 0)

- `refresh_loop_tick_p95_us`
- `refresh_loop_state_clone_p95_us`
- `resize_measure_p95_us`
- `resize_measure_calls_per_sec`
- `scroll_observer_callbacks_per_sec`
- `orchestrator_snapshot_build_p95_us`
- `autosave_fingerprint_p95_us`
- `git_watcher_poll_cost_p95_us`

## Significance Rules

- Significant improvement: p95 improves by >=10% with meaningful absolute gain and no major
  cross-metric regression.
- Regression: p95 worsens by >=10% on key UX metrics.
- Neutral: within variance/noise band.

## Milestones

### Milestone 0: Instrumentation Expansion

Goal:
- Add missing metrics needed to evaluate non-snapshot bottlenecks.

Scope:
- `src/bin/profile_terminal.rs`
- lightweight probe points in `src/ui.rs`, `src/ui/terminal_input.rs`, `src/ui/workspace.rs`,
  `src/ui/git_refresh.rs` (or equivalent wrappers used by profiler)

Before/After:
- Before: baseline from this plan.
- After: 10-run instrumented baseline with new fields.

Success:
- Stable 10-run output with all required metrics present.

Status:
- Complete
- Result: `docs/perf-results/2026-03-01-091848-v2-milestone-0-baseline.md`

### Milestone 1: Refresh Loop Efficiency

Goal:
- Reduce global poll-loop work and unnecessary full state clones.

Targets:
- `src/ui.rs` refresh and resize loops
- use narrower session-id/revision views instead of cloning full app/session objects

Before/After:
- Before: Milestone 0 output.
- After: 10-run rerun + compare `refresh_loop_tick_p95_us`, render metrics.

Success:
- Significant drop in refresh-loop p95; no render/autosave regression.

Status:
- Complete
- Result: `docs/perf-results/2026-03-01-092907-v2-milestone-1-refresh-loop.md`

### Milestone 2: Resize Measurement Optimization

Goal:
- Reduce forced layout cost and measurement frequency.

Targets:
- `src/ui/terminal_input.rs`
- replace repeated probe insertion with cached metrics + `ResizeObserver`-driven invalidation

Before/After:
- Compare `resize_measure_p95_us`, `resize_measure_calls_per_sec`, render metrics.

Success:
- Significant reduction in resize measure p95 and call volume.

### Milestone 3: Scroll Observer Optimization

Goal:
- Lower mutation callback pressure and scroll-stick overhead.

Targets:
- `src/ui/terminal_input.rs` observer configuration/callback strategy

Before/After:
- Compare `scroll_observer_callbacks_per_sec`, render metrics.

Success:
- Significant callback-rate drop with unchanged scroll behavior.

### Milestone 4: Orchestrator + Session Clone Reduction

Goal:
- Reduce render-path object cloning and repeated snapshot construction.

Targets:
- `src/ui/workspace.rs`
- `src/orchestrator/runtime.rs`
- `src/state.rs` group/session traversal helpers

Before/After:
- Compare `orchestrator_snapshot_build_p95_us`, `render_pass_p95_us`,
  `ui_row_render_pass_p95_us`.

Success:
- Significant orchestrator build p95 drop and measurable render-path improvement.

### Milestone 5: Autosave Fingerprint Cost Reduction

Goal:
- Remove duplicate serialization work in autosave dedupe path.

Targets:
- `src/ui.rs`
- `src/persistence/schema.rs`

Before/After:
- Compare `autosave_fingerprint_p95_us`, `autosave_pass_p95_us`.

Success:
- Significant fingerprint-cost reduction; autosave pass remains stable or better.

### Milestone 6: Git Watcher Efficiency

Goal:
- Reduce active-repo polling command overhead.

Targets:
- `src/orchestrator/repo_watcher.rs`
- `src/git/mod.rs`
- `src/ui/git_refresh.rs`

Before/After:
- Compare `git_watcher_poll_cost_p95_us`.
- Validate no regression in Git panel freshness behavior.

Success:
- Significant watcher-cost reduction with correct refresh behavior.

### Milestone 7: Final Consolidated Rerun + Recommendation Update

Goal:
- Confirm end-to-end impact and refresh recommendations with real data.

Tasks:
- 10-run final rerun on same environment.
- Produce consolidated table baseline -> each milestone -> final.
- Classify each milestone: significant / neutral / regression.
- Update `docs/PERFORMANCE-RECOMMENDATIONS.md`.

Output:
- `docs/perf-results/2026-03-01-<ts>-v2-final-comparison.md`

## Verification Gates (Per Milestone)

- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- 10-run perf capture + summary

## Commit Strategy

- One atomic commit per completed milestone.
- Keep instrumentation commits separate from optimization commits where practical.

## Re-Plan Triggers

- SurrealDB terminal-history migration lands and changes metric semantics.
- Any milestone introduces a key metric regression (>=10% p95 worsening).
- Probe semantics change in a way that breaks comparability without a documented mapping.

## Execution Notes

- 2026-03-01: Milestone 0 completed.
  - Added new profiler metrics in `src/bin/profile_terminal.rs`:
    - `refresh_loop_tick_p95_us`
    - `refresh_loop_state_clone_p95_us`
    - `resize_measure_p95_us`
    - `resize_measure_calls_per_sec_p95`
    - `scroll_observer_callbacks_per_sec_p95`
    - `orchestrator_snapshot_build_p95_us`
    - `autosave_fingerprint_p95_us`
    - `git_watcher_poll_cost_p95_us`
  - Captured 10-run baseline:
    - `.perf/2026-03-01-091848-profile-terminal-v2-plan-baseline.txt`
    - `docs/perf-results/2026-03-01-091848-v2-milestone-0-baseline.md`
  - Next step: Milestone 1 (refresh loop efficiency).
- 2026-03-01: Milestone 1 completed.
  - Removed full `AppState` clones from refresh/resize polling paths in `src/ui.rs`.
  - Added lightweight state projections in `src/state.rs`:
    - `session_ids_in_group`
    - `workspace_session_ids_for_group`
  - Updated profiler refresh-loop probe in `src/bin/profile_terminal.rs` to match the
    session-id projection strategy.
  - Captured 10-run post-change metrics:
    - `.perf/2026-03-01-092907-profile-terminal-v2-m1-refresh.txt`
    - `docs/perf-results/2026-03-01-092907-v2-milestone-1-refresh-loop.md`
  - Outcome: significant improvement on target metric (`refresh_loop_tick_p95_us`: 21 -> 18,
    -14.3%) with no >=10% key metric regression.
  - Next step: Milestone 2 (resize measurement optimization).
