# Terminal Performance and Concurrency Plan

This plan is aligned with:

- `GESTALT-STANDARDS.md` (lock scope, async lifecycle, module boundaries)
- `Coding-Standards/CONCURRENCY-STANDARDS.md` (message passing, mutex selection, bounded queues)

## Goals

1. Typing latency p95 under active output: `< 2 ms`
2. Typing latency p99 under active output: `< 5 ms`
3. Eliminate input stalls caused by render/autosave lock contention
4. Preserve PTY/vt100 correctness

## Current Findings

Profiling shows contention on the shared terminal runtime lock:

- Baseline typing path is very fast
- Under render and autosave workload, lock wait spikes into sub-millisecond to multi-millisecond range
- Render pass is the dominant lock-holder; autosave contributes secondary pressure

## Phase Plan

### Phase 1: Lock Sharding and Input Path Decoupling

1. Replace global `Arc<Mutex<TerminalManager>>` UI usage with `Arc<TerminalManager>`
2. Move synchronization inside `TerminalManager`:
   - `RwLock<HashMap<SessionId, TerminalRuntime>>`
   - per-session locks for parser/writer/master/cwd
3. Keep lock scope minimal for each operation (`send_input`, `snapshot`, `resize`)
4. Keep all operations synchronous (no lock held across `.await`)

Expected impact:

- Typing no longer waits behind full-manager lock held by render/autosave readers

### Phase 2: Snapshot Caching and Render Work Reduction

1. Add per-session snapshot cache + revision
2. Reader thread marks sessions dirty on parser updates
3. UI consumes only changed session snapshots instead of resnapshotting all panes every refresh
4. Remove duplicate snapshot work between workspace pane and orchestrator pane

Expected impact:

- Lower mutex hold time and reduced repeated cloning/formatting

### Phase 3: Event-Driven Refresh, Not Global 33ms Full Repaint

1. Replace unconditional 33ms global tick with dirty-session driven refresh
2. Keep throttling/coalescing window (16–33ms), but update only affected panes
3. Isolate terminal pane rendering from global app tree rerenders

Expected impact:

- Reduced UI thread work and lower input-to-paint delay

### Phase 4: Autosave Pipeline Refactor

1. Trigger autosave from state/snapshot revisions with debounce
2. Build snapshot from cached terminal states
3. Serialize/write on dedicated background worker
4. Track background task lifecycle and clean shutdown

Expected impact:

- Avoid autosave-induced stalls while preserving data safety

### Phase 5: Guardrails and Regression Checks

1. Keep `src/bin/profile_terminal.rs` as repeatable profiling harness
2. Add perf regression checks around lock wait and hold-time percentiles
3. Enforce standard checks:
   - `cargo fmt`
   - `cargo clippy --all-targets -- -D warnings`
   - `cargo test -q`

## Implementation Order

1. Phase 1
2. Phase 2
3. Phase 3
4. Phase 4
5. Phase 5

Each phase should land with measurable before/after metrics from `profile_terminal`.

## Status (Current)

- Phase 1: implemented
  - Internal lock sharding in `TerminalManager`
  - UI no longer holds a global `Mutex<TerminalManager>`
- Phase 2: implemented
  - Per-session snapshot cache + revision counter
  - Reader thread updates cache/revision
  - Orchestrator now consumes precomputed runtime view in UI path
- Phase 3: implemented
  - Replaced unconditional 33ms global tick with revision-driven tick
  - Workspace terminal rendering moved into dedicated component scope to reduce full-shell rerenders
- Phase 4: implemented
  - Autosave now skips expensive snapshot rebuilds when app/session revisions are unchanged
  - Dedicated save worker + shutdown-coordinated lifecycle implemented
  - Queue coalescing keeps only latest pending snapshot while save is in flight
- Phase 5: implemented
  - Profiling harness includes `--assert` regression thresholds
  - Automation integration point: `scripts/perf-gate.sh` for PTY-capable CI/local runners
