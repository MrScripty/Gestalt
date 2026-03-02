# Further Performance Audit 2026-03-01

## Scope

Post-milestone audit to identify additional performance opportunities after:
- render-window reduction
- autosave history-cap optimization
- final rerun validation

This audit is code-based (static) and references current implementation hotspots.

## Current Baseline Context

Latest consolidated result is documented in:
- `docs/perf-results/2026-03-01-091603-milestone-5-final-comparison.md`

Net wins are strong in autosave and row-render workload, but there is still meaningful
headroom in render pipeline CPU and global refresh overhead.

## Findings (Prioritized)

### 1) Terminal snapshot rebuild still clones and merges full history on every PTY read (High)

Evidence:
- `src/terminal.rs:403-414` rebuilds snapshot for each read chunk.
- `src/terminal.rs:407-411` clones full `scrollback.lines`.
- `src/terminal.rs:424-429` rebuilds visible rows and merges history.
- `src/terminal.rs:471-485` overlap detection + full `Vec<String>` rebuild.
- `src/terminal.rs:540-544` front-drain on overflow (`Vec::drain(0..overflow)`), O(n).

Why it matters:
- Work scales with history length and output frequency, not just viewport size.
- This is likely one of the remaining core CPU costs under heavy terminal output.

Recommendation:
- Replace `Vec<String>` scrollback with a ring structure (`VecDeque` or fixed-cap ring).
- Track overlap/index incrementally instead of re-merging whole history each read.
- Generate render snapshot as shared slices/chunks (or line handles) to avoid full clone.

### 2) Core UI refresh is still polling-based and clones state each tick (High)

Evidence:
- `src/ui.rs:112` refresh tick every `33ms`.
- `src/ui.rs:114` full `app_state` clone in loop.
- `src/ui.rs:124-137` group session clone/sort each tick.
- `src/ui.rs:156-159` resize loop polls every `180ms` with state clone.
- `src/ui.rs:176-179` DOM viewport measurement call per active terminal.

Why it matters:
- Constant polling causes wakeups and allocations even when nothing changed.
- It limits idle efficiency and competes with render/autosave work.

Recommendation:
- Move to event-driven invalidation from terminal snapshot revision changes.
- Keep polling as fallback only (lower cadence) when event stream is unavailable.
- Derive active session IDs without cloning full `Session` values.

### 3) Resize measurement path forces repeated layout work (High)

Evidence:
- `src/ui/terminal_input.rs:236-246` inserts/removes probe node and reads layout.
- `src/ui/terminal_input.rs:219-263` recomputes metrics every poll cycle.

Why it matters:
- Repeated probe insertion and measurement can trigger reflow pressure.
- This cost grows with active terminals and frequent resize checks.

Recommendation:
- Use `ResizeObserver` for terminal root and cache font metrics per terminal style key.
- Re-measure character width only when font/zoom/style changes, not every loop.

### 4) Scroll management observer is broad and unthrottled (Medium-High)

Evidence:
- `src/ui/terminal_input.rs:126-132` `MutationObserver` with `subtree: true` and `characterData: true`.

Why it matters:
- Terminal updates create many mutations; callback can fire very frequently.
- Immediate `scrollTop = scrollHeight` writes may cause extra layout churn.

Recommendation:
- Observe only `childList` on the line container.
- Coalesce scroll-stick updates via `requestAnimationFrame`.
- Gate observer callback by an atomic "new rows appended" signal where possible.

### 5) Orchestrator round extraction still does repeated scans on render path (Medium)

Evidence:
- `src/ui/workspace.rs:112-120` orchestrator snapshot rebuilt during workspace render.
- `src/orchestrator/runtime.rs:110-117` calls `latest_round_from_lines` for each session.
- `src/orchestrator/runtime.rs:69-71`, `110-112` call `sessions_in_group` (cloned sessions).
- `src/state.rs:550-555` `sessions_in_group` clones `Session` entries.

Why it matters:
- Even bounded scans add per-refresh overhead, especially with multiple panes.
- Repeated allocation/cloning of session lists increases GC/allocator pressure.

Recommendation:
- Cache latest-round metadata per terminal revision and reuse until revision changes.
- Replace `sessions_in_group -> Vec<Session>` with iterator/ID-based traversal APIs.

### 6) Autosave still computes a JSON-based fingerprint before write (Medium)

Evidence:
- `src/ui.rs:341-349` calls `stable_fingerprint` each autosave cycle after building snapshot.
- `src/persistence/schema.rs:35-43` serializes to JSON bytes just to hash.
- `src/persistence/store.rs:32-35` later serializes again for actual save.

Why it matters:
- Duplicate serialization work remains in autosave path.
- Current autosave costs are much lower now, but this is still avoidable overhead.

Recommendation:
- Prefer revision/signature-driven dedupe as primary gate.
- If fingerprint remains required, compute via structured hash over fields without JSON serialization.

### 7) Git watcher fingerprint path is command-heavy at 1s cadence (Medium)

Evidence:
- `src/orchestrator/repo_watcher.rs:6` poll interval `1000ms`.
- `src/orchestrator/repo_watcher.rs:48-50` recomputes full fingerprint every poll.
- `src/git/mod.rs:155-183` fingerprint runs multiple Git commands (`rev-parse`, `status`, `for-each-ref`).
- `src/ui/git_refresh.rs:47-56` coordinator also clones app/group path state every `500ms`.

Why it matters:
- Active repo monitoring can consume non-trivial CPU/process churn in large repos.
- Overlaps with UI refresh and terminal activity on busy projects.

Recommendation:
- Prefer filesystem event-based watcher (`notify`) for active repo root.
- Reduce fallback poll cadence and shrink fingerprint scope.
- Avoid full refs scan every second unless branch/tag panel is open.

### 8) Render tree still performs broad cloning in several UI panels (Medium)

Evidence:
- `src/ui/workspace.rs:55` clones full `AppState`.
- `src/ui/tab_rail.rs:16`, `32`, `47-52` clones app state/groups/sessions during render.
- `src/ui/git_panel.rs:18`, `130`, `177`, `240` clones `RepoContext` and full vectors per section.
- `src/ui/commands_panel.rs:25-35` clones all commands and filtered copies every render.

Why it matters:
- Increases allocation churn and render latency during frequent updates.
- Becomes more visible as command libraries and git histories grow.

Recommendation:
- Introduce lighter-weight view models for render (`Arc`/borrowed slices where feasible).
- Derive filtered lists incrementally/memoized by query + source revision.

## Suggested Next Optimization Order

1. Terminal snapshot pipeline refactor (`src/terminal.rs`)  
   Expected impact: high, system-wide.
2. Refresh/resize event-driven conversion (`src/ui.rs`, `src/ui/terminal_input.rs`)  
   Expected impact: high for perceived smoothness.
3. Round extraction/session clone reduction (`src/orchestrator/runtime.rs`, `src/state.rs`)  
   Expected impact: medium-high.
4. Autosave fingerprint dedupe simplification (`src/ui.rs`, `src/persistence/schema.rs`)  
   Expected impact: medium.
5. Git watcher/fingerprint optimization (`src/orchestrator/repo_watcher.rs`, `src/git/mod.rs`)  
   Expected impact: medium for repo-heavy users.

## Measurement Plan For Next Audit Cycle

Add/track:
- `terminal_snapshot_build_p95_us`
- `scrollback_clone_bytes_per_sec`
- `refresh_loop_tick_cost_p95_us`
- `resize_measure_p95_us` and count/sec
- `mutation_observer_callbacks_per_sec`
- `orchestrator_snapshot_build_p95_us`
- `autosave_fingerprint_p95_us`
- `git_watcher_poll_cost_p95_us`

Run protocol:
- 10-run baseline (`profile_terminal --assert --json`) plus new render-loop probes
- before/after per milestone, same environment, same warmup
- significance threshold unchanged (>=10% p95 with meaningful absolute gain)
