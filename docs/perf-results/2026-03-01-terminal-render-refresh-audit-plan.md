# Terminal Render/Refresh Audit + Plan (2026-03-01)

## Objective

Audit terminal draw calls, refresh cadence, and render pipeline bottlenecks, then define a measurable before/after plan focused on UI responsiveness.

## Current Baseline (Before)

Reference benchmark data (latest completed run set):
- `docs/perf-results/2026-03-01-025300-baseline-v2.md`
- `docs/perf-results/2026-03-01-074955-final.md`

Current p95 deltas vs baseline-v2:
- `baseline_total_send_p95_us`: `28 -> 25` (`-10.7%`)
- `render_total_send_p95_us`: `27 -> 25` (`-7.4%`)
- `full_total_send_p95_us`: `27 -> 25` (`-7.4%`)
- `render_pass_p95_us`: `10511 -> 10694` (`+1.7%`)
- `autosave_pass_p95_us`: `6695 -> 7252` (`+8.3%`)

Derived current render-load envelope from code defaults:
- Render window rows per terminal: `512` (`src/ui/terminal_view.rs`, `RENDER_WINDOW_MIN_ROWS`)
- Default visible terminals per group: `3` (Agent A, Agent B, Run/Compile)
- Minimum terminal row nodes per refresh: `1536` line containers
- Minimum text span nodes per refresh: `1536`
- Minimum total nodes touched per refresh: `3072`
- Refresh cadence target: ~`30Hz` (`TERMINAL_REFRESH_POLL_MS = 33ms`)
- Minimum node updates per second (lower bound): ~`92,160`

## Audit Findings

### Finding 1 (High): Full-history scans still run in render-sensitive paths

Evidence:
- `terminal_round_bounds(&terminal.lines, ...)` scans the line buffer every terminal render: `src/ui/terminal_view.rs`.
- `orchestrator::snapshot_group_from_runtime(...)` calls `latest_round_from_lines(...)` which scans line history again: `src/orchestrator/runtime.rs`.
- With `12,000` history lines and `3` terminals, these scans can repeat across frequent refreshes.

Impact:
- CPU spikes and frame jitter under active output.
- Performance scales with history depth, not just viewport size.

### Finding 2 (High): Polling-based refresh remains globally hot

Evidence:
- Terminal revision polling loop every `33ms`: `src/ui.rs` (`TERMINAL_REFRESH_POLL_MS`).
- Resize measurement polling loop every `180ms`: `src/ui.rs` (`TERMINAL_RESIZE_POLL_MS`).
- Autosave loop every `1200ms`: `src/ui.rs` (`AUTOSAVE_POLL_MS`).

Impact:
- Constant wakeups and repeated state/DOM probes even when visible output is stable.
- Difficult to maintain smooth UI under concurrent output + autosave + resize checks.

### Finding 3 (High): Resize measurement likely triggers expensive layout work

Evidence:
- `measure_terminal_viewport(...)` appends a probe element, measures `getBoundingClientRect`, then removes it on every poll: `src/ui/terminal_input.rs`.
- This runs per active terminal in the resize loop.

Impact:
- Repeated forced layout/reflow pressure.
- Avoidable overhead when dimensions are unchanged.

### Finding 4 (High): Autosave still snapshots full terminal history frequently

Evidence:
- Autosave path calls `persistence::build_workspace_snapshot(...)`: `src/ui.rs`.
- Snapshot pulls per-session persisted terminal data including normalized lines: `src/persistence/mod.rs`, `src/terminal.rs::snapshot_for_persist`.
- For 3 terminals x 12,000 lines, that is ~36,000 lines normalized/copied per autosave pass.

Impact:
- Background CPU/memory pressure and lock contention with active terminal rendering.

### Finding 5 (Medium): Scroll behavior observer is broad

Evidence:
- `install_terminal_scroll_behavior(...)` registers a `MutationObserver` with `subtree: true` and `characterData: true`: `src/ui/terminal_input.rs`.

Impact:
- High-frequency mutation callbacks during terminal updates.
- Potential extra style/layout churn while auto-sticking to bottom.

### Finding 6 (Medium): Benchmark blind spot for UI draw pipeline

Evidence:
- `profile_terminal` simulates render using runtime snapshots and orchestrator extraction, but does not measure actual DOM diff/paint/mutation observer cost: `src/bin/profile_terminal.rs`.

Impact:
- Regressions can pass existing benchmark while real UI still feels sluggish.

## Candidate Bottlenecks To Validate

1. Prompt/round extraction complexity in UI render path.
2. Polling cadence and polling scope (revision/resize/autosave).
3. DOM-measurement strategy for terminal viewport sizing.
4. Full-history persistence snapshot cost during autosave.
5. Mutation observer callback volume and scroll-stick behavior.

## Implementation Plan (Before/After Measured)

## Milestone 1: Add Render-Pipeline Instrumentation

Goal:
- Measure true UI hot path cost, not only PTY send cost.

Tasks:
- Add counters/timers for:
  - terminal rows rendered per frame
  - `terminal_round_bounds` duration/count
  - `latest_round_from_lines` duration/count
  - resize measurement duration/count
  - autosave snapshot build duration and lines serialized
- Emit machine-readable summaries (`GESTALT_RENDER_PROFILE_JSON`) similar to current profiler.
- Add one focused benchmark command for render pipeline sampling.

Verification:
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- 10-run instrumentation baseline saved in `docs/perf-results/`

Success criteria:
- Stable 10-run baseline for render-focused metrics (median/p95/p99/max).

## Milestone 2: Remove Full-History Scans From Frequent Render Paths

Goal:
- Decouple render complexity from total history line count.

Tasks:
- Cache/incrementally maintain prompt boundaries per session snapshot revision.
- Replace repeated full scans in `terminal_round_bounds` and orchestrator round extraction with bounded or cached lookups.
- Avoid round preview recomputation unless relevant session revision changes.

Verification:
- Same checks as Milestone 1
- 10-run after measurement vs Milestone 1 baseline

Success criteria:
- Significant improvement in round-extraction p95 (>=10% and meaningful absolute reduction).

## Milestone 3: Rework Refresh/Resize Loops To Event-Driven Or Coalesced Updates

Goal:
- Lower global polling overhead and layout churn.

Tasks:
- Replace 33ms revision polling with event-driven invalidation where feasible.
- Throttle/coalesce resize measurement; avoid probe insertion on unchanged layout.
- Bound refresh work to active/visible panes and changed sessions only.

Verification:
- Same checks
- 10-run after measurement

Success criteria:
- Significant reduction in refresh-loop CPU time and resize-measure p95.

## Milestone 4: Reduce Autosave Snapshot Cost

Goal:
- Make autosave incremental relative to changed sessions/revisions.

Tasks:
- Persist only changed terminal states since last saved signature where possible.
- Cap or delta-encode persisted terminal history for autosave path.
- Keep full-fidelity recovery guarantees explicit and tested.

Verification:
- Same checks
- 10-run after measurement

Success criteria:
- Significant improvement in autosave snapshot p95 and lower contention impact on render metrics.

## Milestone 5: Final Comparison + Recommendation Update

Goal:
- Decide which optimizations are truly impactful and safe.

Tasks:
- Run final 10-run suite in same environment.
- Produce consolidated table: before vs each milestone vs final (`median/p95/p99/max`).
- Classify each milestone: significant / neutral / regression.
- Update `docs/PERFORMANCE-RECOMMENDATIONS.md` from real data.

Verification:
- Final report in `docs/perf-results/`
- Code-quality gates pass

## Proposed Metric Set (Before/After)

Existing:
- `render_pass_p95_us`
- `autosave_pass_p95_us`
- `baseline_total_send_p95_us`
- `render_total_send_p95_us`
- `full_total_send_p95_us`

New (required for this audit):
- `ui_rows_rendered_per_refresh`
- `ui_row_render_pass_us`
- `round_bounds_extract_us`
- `orchestrator_round_extract_us`
- `resize_measure_us`
- `autosave_snapshot_lines_total`
- `autosave_snapshot_build_us`
- `scroll_observer_callbacks_per_sec`

## Significance Rules

- Significant improvement:
  - p95 improves by >=10% with meaningful absolute gain, and
  - no major regression in other key metrics.
- Regression:
  - p95 worsens by >=10% or introduces user-visible lag.
- Neutral:
  - within normal run-to-run variance.

## Coding-Standards Oversights/Conflicts To Address

### 1) Frontend polling guidance exists but is not enforceable in current process

Reference:
- `FRONTEND-STANDARDS.md` rules favor event-driven synchronization and limit high-frequency polling.

Observed conflict:
- Multiple global polling loops remain in core UI path (`33ms`, `180ms`, `1200ms`) without hard budget enforcement.

Recommendation:
- Add mandatory "polling budget + documented justification" checklist item in PR template or lintable architecture checks.

### 2) Performance guidance lacks explicit UI render-budget requirements

Reference:
- `CODING-STANDARDS.md` performance section emphasizes profiling/benchmarks but does not set frontend frame/render budgets.

Observed gap:
- No required thresholds for UI row render cost, DOM mutation volume, or frame-time budgets.

Recommendation:
- Add required UI performance budgets (for example p95 frame budget and max rows rendered per refresh).

### 3) Testing standards mention performance budgets but do not require CI perf gates

Reference:
- `TESTING-STANDARDS.md` includes performance-budget examples.
- `TOOLING-STANDARDS.md` defines mandatory CI gates but no perf gate.

Observed gap:
- Performance regressions can pass CI if functional checks pass.

Recommendation:
- Add non-optional perf regression gate for render/autosave benchmarks once baseline is stable.

### 4) "Don't optimize startup code" guidance can be misapplied to UX-critical startup/render paths

Reference:
- `CODING-STANDARDS.md` "When to optimize" section discourages optimizing startup code.

Observed risk:
- Startup-adjacent work in render-sensitive loops can still materially harm responsiveness.

Recommendation:
- Clarify exception: optimize startup work when it executes in UI/render lifecycle or impacts first-interaction latency.

## Execution Notes

- This document is planning + audit only.
- Milestone 1 completed on 2026-03-01:
  - Instrumentation added in `src/bin/profile_terminal.rs`.
  - Instrumented baseline captured: `docs/perf-results/2026-03-01-082534-render-instrumentation-baseline.md`.
- Milestone 2 completed on 2026-03-01:
  - Removed render-path round-bound extraction and bounded orchestrator prompt scan depth.
  - Measured result: `docs/perf-results/2026-03-01-084224-milestone-2-round-scan-removal.md`.
- Next step is Milestone 3 (reduce row-render workload volume and repeated parsing cost).
