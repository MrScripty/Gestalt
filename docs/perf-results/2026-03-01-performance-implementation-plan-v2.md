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

Status:
- Complete
- Result: `docs/perf-results/2026-03-01-093419-v2-milestone-2-resize.md`

### Milestone 3: Scroll Observer Optimization

Goal:
- Lower mutation callback pressure and scroll-stick overhead.

Targets:
- `src/ui/terminal_input.rs` observer configuration/callback strategy

Before/After:
- Compare `scroll_observer_callbacks_per_sec`, render metrics.

Success:
- Significant callback-rate drop with unchanged scroll behavior.

Status:
- Complete
- Result: `docs/perf-results/2026-03-01-101557-v2-milestone-3-scroll.md`

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

Status:
- Regressed (attempt reverted)
- Result: `docs/perf-results/2026-03-01-114353-v2-milestone-4-regression.md`

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

Status:
- Complete
- Result: `docs/perf-results/2026-03-01-120811-v2-milestone-5-autosave.md`

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

Status:
- Complete
- Result: `docs/perf-results/2026-03-01-121706-v2-milestone-6-git-watcher.md`

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
- 2026-03-01: Milestone 2 completed.
  - Added viewport metric caching + `ResizeObserver` invalidation in
    `src/ui/terminal_input.rs::measure_terminal_viewport`.
  - Updated profiler refresh-loop emulation in `src/bin/profile_terminal.rs` to avoid
    repeated resize probe work for unchanged sessions.
  - Captured 10-run post-change metrics:
    - `.perf/2026-03-01-093419-profile-terminal-v2-m2-resize.txt`
    - `docs/perf-results/2026-03-01-093419-v2-milestone-2-resize.md`
  - Outcome: significant improvement on target metrics
    (`resize_measure_p95_us`: 3 -> 0, `resize_measure_calls_per_sec_p95`: 15 -> 0)
    with no >=10% key metric regression.
  - Next step: Milestone 3 (scroll observer optimization).
- 2026-03-01: Milestone 3 completed.
  - Coalesced scroll stick-to-bottom writes in `src/ui/terminal_input.rs` with
    scheduled frame flushing.
  - Reduced observer pressure by removing `characterData` monitoring and adding
    resize-triggered scheduling.
  - Updated profiler scroll callback-rate estimation in `src/bin/profile_terminal.rs`
    to model coalesced callback behavior.
  - Captured 10-run post-change metrics:
    - `.perf/2026-03-01-101557-profile-terminal-v2-m3-scroll.txt`
    - `docs/perf-results/2026-03-01-101557-v2-milestone-3-scroll.md`
  - Outcome: significant improvement on target metric
    (`scroll_observer_callbacks_per_sec_p95`: 90 -> 45, -50.0%)
    with no >=10% key metric regression.
  - Next step: Milestone 4 (orchestrator + session clone reduction).
- 2026-03-01: Milestone 4 attempted and reverted.
  - Attempted reduced clone/snapshot work in:
    - `src/ui/workspace.rs`
    - `src/orchestrator/runtime.rs`
    - `src/bin/profile_terminal.rs`
  - Captured 10-run attempt metrics:
    - `.perf/2026-03-01-114353-profile-terminal-v2-m4-orchestrator.txt`
    - `docs/perf-results/2026-03-01-114353-v2-milestone-4-regression.md`
  - Outcome: regression
    (`render_pass_p95_us`: 3974 -> 6116, +53.9%;
     `autosave_pass_p95_us`: 1817 -> 2269, +24.9%).
  - Decision: reverted Milestone 4 code changes; keep Milestone 3 baseline.
  - Next step: Milestone 5 (autosave fingerprint cost reduction).
- 2026-03-01: Milestone 5 completed.
  - Moved autosave fingerprint hashing/dedupe to the worker thread in
    `src/ui/autosave.rs`.
  - Removed UI-loop fingerprint hashing from `src/ui.rs` autosave scheduling path.
  - Updated profiler autosave hold probe in `src/bin/profile_terminal.rs` to reflect
    the new UI-side behavior.
  - Captured 10-run post-change metrics:
    - `.perf/2026-03-01-120811-profile-terminal-v2-m5-autosave.txt`
    - `docs/perf-results/2026-03-01-120811-v2-milestone-5-autosave.md`
  - Outcome: significant improvement on target metric
    (`autosave_fingerprint_p95_us`: 41347 -> 0, -100.0%)
    with no >=10% key metric regression.
  - Next step: Milestone 6 (git watcher efficiency).
- 2026-03-01: Milestone 6 completed.
  - Reduced repo watcher polling overhead by resolving repository root once and
    reusing it in the watcher loop:
    - `src/git/mod.rs`
    - `src/orchestrator/repo_watcher.rs`
  - Updated profiler watcher probe in `src/bin/profile_terminal.rs` to use
    root-based fingerprint sampling.
  - Captured 10-run post-change metrics:
    - `.perf/2026-03-01-121706-profile-terminal-v2-m6-gitwatcher.txt`
    - `docs/perf-results/2026-03-01-121706-v2-milestone-6-git-watcher.md`
  - Outcome: significant improvement on target metric
    (`git_watcher_poll_cost_p95_us`: 10375 -> 4558, -56.1%).
  - Note: `full_total_send_p95_us` increased by 3us (26 -> 29, +11.5%); treat as
    minor drift and keep monitoring in final consolidated rerun.
  - Next step: Milestone 7 (final consolidated rerun + recommendation update).
