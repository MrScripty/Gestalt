# Plan: Emily Crate Continuation

## Objective

Continue the `emily` crate as the canonical, host-agnostic Emily core for memory, retrieval, episode modeling, and policy runtime while keeping Gestalt-specific adapters outside the crate and keeping future sovereign-dispatch work architecturally separable.

## Scope

### In Scope

- Evolving `emily` beyond terminal-history storage into a reusable core crate
- Tightening crate boundaries so host-specific logic stays outside the crate
- Building a phased path for retrieval, episodes, `EARL`, and `ECGL`
- Aligning the plan with Gestalt and shared coding standards so implementation remains strongly bounded and reviewable
- Defining verification, lifecycle ownership, and re-plan triggers before implementation begins

### Out of Scope

- Immediate implementation of the full March 2026 sovereign-dispatch architecture
- Gestalt UI redesign or terminal UX changes
- Adding network listeners or remote transports
- Final provider-routing or `Semantic Membrane` execution design
- Commit-by-commit implementation details

## Inputs

### Problem

The repo already contains a reusable `emily` crate, but it currently behaves as a narrow memory and vectorization core while Gestalt uses it mainly as a terminal-history backend. The crate needs a continuation plan that keeps it reusable, aligns it with the Emily research documents, and enforces the standards required for a stable long-term codebase.

### Constraints

- `emily` is a library/core crate and should remain lean per `DEPENDENCY-STANDARDS.md`.
- Gestalt layer rules in [GESTALT-STANDARDS.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/GESTALT-STANDARDS.md) remain authoritative for repo integration.
- Public crate APIs and types must stay documented and typed.
- Rust files should target `<= 500` lines and trigger decomposition review if they exceed that threshold.
- Directory READMEs must stay current for `src/` trees and any non-obvious multi-file directory.
- Async/runtime work must follow `CONCURRENCY-STANDARDS.md`:
  - prefer message passing over shared mutable state
  - keep related state under one lock
  - bound queues
  - never hold sync locks across `.await`
  - track `JoinHandle`s and shutdown ownership for spawned tasks
- Verification must follow repo and shared testing/tooling standards:
  - `cargo fmt`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test -q`
- The current `emily` crate must remain usable by Gestalt while its internals evolve.

### Assumptions

- The `emily` crate remains the canonical place for reusable Emily core logic.
- Gestalt remains an application host and composition root, not the place where core Emily logic should accumulate.
- `EARL` and `ECGL` belong in the crate once the required data model exists.
- Sovereign-dispatch execution may end up in either `emily` or a sibling crate, depending on how host-agnostic the resulting contracts really are.

### Dependencies

- Current crate structure in `emily/src/`
- Gestalt adapter layer in `src/emily_bridge.rs`
- Research docs in `docs/emily-research/`
- Shared standards in `/media/jeremy/OrangeCream/Linux Software/Coding-Standards/`
- Repo-specific standards in [GESTALT-STANDARDS.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/GESTALT-STANDARDS.md)

### Affected Structured Contracts

- `emily::api::EmilyApi`
- `emily::model::*` request/response and stored-object types
- `emily::store::EmilyStore`
- Gestalt bridge expectations in `src/emily_bridge.rs`

### Affected Persisted Artifacts

- Surreal-backed text objects
- text vectors
- text edges
- vectorization config
- future episode, outcome, audit, and policy records

### Concurrency And Race-Risk Review

The crate already contains long-lived runtime state, vectorization jobs, async locks, and queue-like behavior. Implementation work must explicitly review:

- queue capacity and drop policy for ingest and background work
- lifecycle ownership for vectorization and future policy workers
- startup and shutdown ordering
- overlapping job prevention
- stale-result handling for background jobs
- lock selection and lock scope around store and runtime state

### Risks

| Risk | Impact | Mitigation |
| ---- | ------ | ---------- |
| The crate absorbs too much Gestalt-specific behavior | High | Preserve host-agnostic API boundaries and push host mapping into adapters |
| Placeholder policy fields become de facto semantics | High | Phase 1 removes or downgrades misleading defaults before new product behavior depends on them |
| Retrieval changes break Gestalt history behavior | High | Preserve facade compatibility first and add cross-layer acceptance checks |
| Async workers become hard to shut down or reason about | High | Require explicit lifecycle ownership, bounded queues, cancellation paths, and tracked handles |
| The crate becomes too large or muddled | Medium | Split by responsibility, enforce decomposition review, keep README and module docs current |
| Dependency growth turns the core crate into a heavy leafless dependency | Medium | Keep dependencies lean, prefer stdlib or small focused crates, justify any heavy additions in writing |

## Definition of Done

- The plan is aligned with shared and repo-specific standards.
- Milestones preserve a reusable crate boundary rather than baking Gestalt assumptions into the core.
- Each milestone has explicit tasks, verification, and compatibility expectations.
- Concurrency ownership and shutdown expectations are recorded before implementation.
- The plan is specific enough that implementation can proceed milestone by milestone without architectural ambiguity.

## Ownership And Lifecycle Note

For runtime-owned background work inside `emily`:

- the runtime type owns worker startup
- the runtime type owns cancellation signaling
- the runtime type owns `JoinHandle` tracking or equivalent lifecycle tracking
- shutdown behavior must be explicit, testable, and idempotent
- overlapping vectorization or policy jobs must use single-owner coordination, not ad hoc shared flags

No milestone should introduce a long-lived task without documenting:

1. who starts it
2. who stops it
3. how cancellation is signaled
4. how restart overlap is prevented
5. what state is durable across restart

## Public Facade Preservation Note

This plan assumes facade-first preservation, not an API-breaking rewrite.

That means:

- preserve `EmilyApi` as the host-facing facade while internals evolve
- prefer append-only API growth where possible
- isolate breaking storage or policy changes behind migrations or compatibility layers
- if a milestone requires a breaking API or persisted-schema change, stop and re-plan explicitly before implementation

## Standards Alignment Rules For All Milestones

Every implementation milestone under this plan must also satisfy:

- architecture separation from `ARCHITECTURE-PATTERNS.md`
- repo layer boundaries from [GESTALT-STANDARDS.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/GESTALT-STANDARDS.md)
- documentation requirements from `DOCUMENTATION-STANDARDS.md`
- testing requirements from `TESTING-STANDARDS.md`
- concurrency requirements from `CONCURRENCY-STANDARDS.md`
- dependency discipline from `DEPENDENCY-STANDARDS.md`

In practice that means:

- domain and policy logic stay independent of Gestalt UI and transport concerns
- public types and functions are documented with `///`
- module and directory READMEs are updated when structure changes
- no `unwrap` or `expect` in non-test code unless failure is truly unrecoverable and justified
- non-trivial literals become named constants
- cross-layer work gets at least one acceptance check from producer input to consumer output
- replay, recovery, and idempotency checks are added when durable commands, workers, or projections are introduced

## Milestones

### Milestone 1: Baseline Alignment And Boundary Cleanup

**Goal:** Make the crate honest about current behavior and safe to extend.

**Tasks:**
- [ ] Audit all public crate claims against real behavior in `api`, `model`, `runtime`, and `store`
- [ ] Remove or downgrade misleading defaults such as implicit `integrated = true`
- [ ] Decide which current policy-related fields remain placeholders and document their status
- [ ] Make runtime health surfaces report only real counters
- [ ] Review current file sizes and split modules that already exceed standards thresholds or hold multiple responsibilities
- [ ] Update crate and source-tree docs so boundary and invariants are explicit

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- review updated READMEs for required sections and crate boundary accuracy
- add or update tests that prove restore/history behavior and current health counters remain correct

**Status:** Complete

### Milestone 2: Retrieval Core

**Goal:** Replace provisional lexical retrieval with a real semantic retrieval foundation.

**Tasks:**
- [ ] Implement embedding-driven retrieval over stored vectors
- [ ] Persist semantic edges instead of only linear edges
- [ ] Make `neighbor_depth` and provenance expansion real
- [ ] Introduce deterministic ranking with named constants and documented factors
- [ ] Define lexical or mixed fallback behavior when vectors are absent
- [ ] Decide whether retrieval logic belongs in existing modules or a dedicated retrieval submodule
- [ ] Add acceptance tests that cover producer-to-consumer retrieval behavior through the public facade

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- unit tests for ranking, fallback, edge expansion, and deterministic ordering
- integration or acceptance check through `EmilyApi::query_context`
- replay or recovery check if retrieval depends on persisted vectors and edges

**Status:** Complete

### Milestone 3: Episode And Outcome Contracts

**Goal:** Introduce the host-agnostic data model needed for policy runtimes.

**Tasks:**
- [ ] Define episode and outcome contracts as typed public models
- [ ] Add append-oriented APIs for episode creation, outcome ingestion, and linkage
- [ ] Add persisted records for episodes, outcomes, and audit trails
- [ ] Decide whether new contracts stay in `model.rs` or move into focused modules
- [ ] Document compatibility and migration behavior for new persisted artifacts
- [ ] Add recovery and idempotency rules for repeated or partial episode ingestion

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- roundtrip tests for new model types
- integration tests for episode persistence, replay, and duplicate handling
- README updates covering new consumer contracts and structured producer contracts

**Status:** Complete

### Milestone 4: EARL Runtime

**Goal:** Add the first active, host-agnostic `EARL` capability to the crate.

**Tasks:**
- [ ] Define an `EARL` signal contract and evaluation result model
- [ ] Implement evaluator logic separate from host UI behavior
- [ ] Record gating decisions in durable audit artifacts
- [ ] Prevent blocked episodes from contaminating integrated memory state
- [ ] Define host-facing error and retry semantics for gated episodes
- [ ] Add lifecycle ownership notes if `EARL` introduces background processing

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- unit tests for `OK`, `CAUTION`, and `REFLEX` outcomes
- integration test proving gate results flow through the public API without host-specific coupling
- acceptance check showing blocked episodes do not become integrated memory

**Status:** Complete

### Milestone 5: ECGL Runtime

**Goal:** Turn stored policy fields into an actual memory-integration policy system.

**Tasks:**
- [ ] Introduce explicit memory states such as pending, integrated, quarantined, and deferred
- [ ] Implement named and documented factor calculations for confidence, outcome, novelty, and stability
- [ ] Add learning-weight, gate, quarantine, and reintegration behavior
- [ ] Add `CI` computation and durable reporting
- [ ] Decide whether ECGL evaluation is synchronous, asynchronous, or hybrid, and document the lifecycle implications
- [ ] Add recovery rules for partially evaluated or quarantined items after restart

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- unit tests for state transitions and factor calculations
- integration tests for quarantine and reintegration paths
- replay and recovery test for persisted pending/quarantined state

**Status:** Complete

### Milestone 6: Core And Host Separation Hardening

**Goal:** Ensure the crate remains reusable as functionality expands.

**Tasks:**
- [ ] Review crate APIs for hidden Gestalt assumptions
- [ ] Move host-specific convenience logic out of the crate where needed
- [ ] Add or update crate-level docs describing stable host responsibilities
- [ ] Review dependencies added so far against library-core standards
- [ ] Split modules if feature growth has created multi-responsibility files or packages

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- doc review of crate README and `emily/src/README.md`
- at least one integration path proving Gestalt can consume the crate without depending on internal modules

**Status:** Complete

### Milestone 7: Sovereign-Dispatch Preparation

**Goal:** Prepare contracts for later sovereign Emily work without prematurely committing to a full remote architecture.

**Tasks:**
- [ ] Define which sovereign concepts belong in the crate as host-agnostic contracts
- [ ] Add extension-friendly types for remote episodes, routing decisions, validation outcomes, and audit metadata if still consistent with the crate boundary
- [ ] Decide whether `Semantic Membrane` contracts belong here or in a sibling crate
- [ ] Document a clear revisit trigger for when the crate should split or grow
- [ ] Preserve append-only public facade evolution where possible

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- contract review against `docs/emily-research/02-architecture-reconstruction.md`
- README and traceability updates describing the new boundary decision

**Status:** Complete

## Execution Notes

Update during implementation:

- 2026-03-08: Plan rewritten to conform to shared and repo-specific standards before implementation begins.
- 2026-03-08: Milestone 1 completed through two commits:
  - `f57bb6c` `fix(emily): align ingest defaults with runtime behavior`
  - `c134880` `refactor(emily): split runtime and inference modules`
- 2026-03-08: Milestone 2 completed through one commit:
  - `04a3d45` `feat(emily): add semantic retrieval and edge traversal`
- 2026-03-08: Milestone 3 completed through one commit:
  - `ef520e0` `feat(emily): add episode and outcome contracts`
- 2026-03-08: Milestone 4 completed through one commit:
  - `bbea8df` `feat(emily): add EARL episode gating`
- 2026-03-08: Milestone 5 completed through one commit:
  - `da0422e` `feat(emily): add ECGL memory integration`
- 2026-03-08: Milestone 6 completed through one commit:
  - `c706018` `refactor(emily): harden host-agnostic runtime boundary`
- 2026-03-08: Milestone 7 completed through one commit:
  - `21fbc20` `feat(emily): add sovereign dispatch contracts`
- 2026-03-08: Follow-on sovereign persistence slice completed through one commit:
  - `e1ce9e8` `feat(emily): persist sovereign runtime records`
- 2026-03-08: Follow-on sovereign query slice completed through one commit:
  - `cae97c7` `feat(emily): expose sovereign record queries`
- 2026-03-08: Follow-on automatic sovereign audit slice completed through one commit:
  - `ff1db62` `feat(emily): auto-generate sovereign audits`
- 2026-03-08: Sovereign audit scope decision recorded:
  - automatic audit generation remains write-side only for now
  - read/query access remains unaudited until a real boundary-crossing host flow requires it
- 2026-03-08: Follow-on bounded sovereign lifecycle slice completed through one commit:
  - `507be15` `feat(emily): add bounded sovereign lifecycle policy`
- 2026-03-08: Follow-on episode-read slice completed through one commit:
  - `fc8ca99` `feat(emily): expose episode reads`
- 2026-03-08: Follow-on explicit remote-state slice completed through one commit:
  - `8f239c4` `feat(emily): add remote episode state transitions`

## Commit Cadence Notes

- Commit when a logical slice is complete and verified.
- Keep commits atomic and aligned with one milestone or sub-slice.
- Follow commit format/history rules from `COMMIT-STANDARDS.md` and Gestalt commit standards.

## Optional Subagent Assignment

| Owner/Agent | Scope | Output Contract | Handoff Checkpoint |
| ----------- | ----- | --------------- | ------------------ |
| None | None assigned | Reason: current work is still plan-shaping and boundary review | Revisit trigger: implementation splits into low-coupling streams such as retrieval vs policy |

## Re-Plan Triggers

- A milestone requires a breaking change to `EmilyApi`
- A persisted-schema change cannot be handled with compatibility or migration
- A new dependency materially increases core-crate weight
- A concurrency design introduces unclear ownership, shutdown, or overlap behavior
- Sovereign-dispatch requirements force a different package boundary than currently assumed
- Gestalt integration needs invalidate the host-agnostic crate assumption

## Recommendations

- Prefer introducing new focused modules before `model.rs` or `runtime.rs` become catch-all files.
- Prefer acceptance and recovery tests early, because this crate owns persisted artifacts and long-lived runtime state.
- Prefer a sibling crate for sovereign-dispatch execution if the implementation starts pulling in transport, routing, or provider-specific policy that would make `emily` too heavy.

## Completion Summary

### Completed

- Milestone 1: Baseline Alignment And Boundary Cleanup
- Milestone 2: Retrieval Core
- Milestone 3: Episode And Outcome Contracts
- Milestone 4: EARL Runtime
- Milestone 5: ECGL Runtime
- Milestone 6: Core And Host Separation Hardening
- Milestone 7: Sovereign-Dispatch Preparation

### Deviations

- Full Gestalt workspace acceptance remains pending while unrelated UI worktree changes break `cargo check -q`.

### Follow-Ups

- Decide whether the next sovereign slice belongs inside `emily` as richer policy/runtime behavior or in a sibling membrane crate.
- Decide whether future sovereign record types should inherit automatic audit generation by default inside `emily`.
- Decide whether Emily should expose episode lists or stream-scoped episode queries now that single-episode reads exist through the public facade.
- Decide whether explicit remote-state transitions should later support non-terminal transitions or remain limited to terminal closure events.
- Decide whether Emily should keep the current explicit sovereign query facade or later add generic query primitives above the same persisted records.

### Verification Summary

- Milestone 1 verification:
  - `cargo fmt --manifest-path emily/Cargo.toml`
  - `cargo test --manifest-path emily/Cargo.toml -q`
  - `cargo clippy --manifest-path emily/Cargo.toml --all-targets -- -D warnings`
- Milestone 2 verification:
  - `cargo fmt --manifest-path emily/Cargo.toml`
  - `cargo test --manifest-path emily/Cargo.toml -q`
  - `cargo clippy --manifest-path emily/Cargo.toml --all-targets -- -D warnings`
- Milestone 3 verification:
  - `cargo fmt --manifest-path emily/Cargo.toml`
  - `cargo test --manifest-path emily/Cargo.toml -q`
  - `cargo clippy --manifest-path emily/Cargo.toml --all-targets -- -D warnings`
- Milestone 4 verification:
  - `cargo fmt --manifest-path emily/Cargo.toml`
  - `cargo test --manifest-path emily/Cargo.toml -q`
  - `cargo clippy --manifest-path emily/Cargo.toml --all-targets -- -D warnings`
- Milestone 5 verification:
  - `cargo fmt --manifest-path emily/Cargo.toml`
  - `cargo test --manifest-path emily/Cargo.toml -q`
  - `cargo clippy --manifest-path emily/Cargo.toml --all-targets -- -D warnings`
- Milestone 6 verification:
  - `cargo fmt --manifest-path emily/Cargo.toml`
  - `cargo test --manifest-path emily/Cargo.toml -q`
  - `cargo clippy --manifest-path emily/Cargo.toml --all-targets -- -D warnings`
  - Bridge/API boundary review against `src/emily_bridge.rs`
  - `cargo check -q` attempted for Gestalt integration and blocked by unrelated UI compile errors in `src/ui/sidebar_panel_host.rs` / `src/ui/local_agent_panel.rs`
- Milestone 7 verification:
  - `cargo fmt --manifest-path emily/Cargo.toml`
  - `cargo test --manifest-path emily/Cargo.toml -q`
  - `cargo clippy --manifest-path emily/Cargo.toml --all-targets -- -D warnings`
  - Contract review against `docs/emily-research/02-architecture-reconstruction.md`
  - Boundary decision recorded in `emily/README.md` and `emily/src/README.md`
- Sovereign persistence slice verification:
  - `cargo fmt --manifest-path emily/Cargo.toml`
  - `cargo test --manifest-path emily/Cargo.toml -q`
  - `cargo clippy --manifest-path emily/Cargo.toml --all-targets -- -D warnings`
  - Runtime acceptance coverage in `emily/src/runtime/sovereign_tests.rs`
  - Surreal roundtrip coverage in `emily/src/store/surreal/tests.rs`
- Sovereign query slice verification:
  - `cargo fmt --manifest-path emily/Cargo.toml`
  - `cargo test --manifest-path emily/Cargo.toml -q`
  - `cargo clippy --manifest-path emily/Cargo.toml --all-targets -- -D warnings`
  - Runtime query coverage in `emily/src/runtime/sovereign_tests.rs`
- Automatic sovereign audit slice verification:
  - `cargo fmt --manifest-path emily/Cargo.toml`
  - `cargo test --manifest-path emily/Cargo.toml -q`
  - `cargo clippy --manifest-path emily/Cargo.toml --all-targets -- -D warnings`
  - Runtime replay/idempotency coverage in `emily/src/runtime/sovereign_tests.rs`
- Bounded sovereign lifecycle slice verification:
  - `cargo fmt --manifest-path emily/Cargo.toml`
  - `cargo test --manifest-path emily/Cargo.toml -q`
  - `cargo clippy --manifest-path emily/Cargo.toml --all-targets -- -D warnings`
  - Runtime policy and lifecycle coverage in `emily/src/runtime/sovereign_tests.rs`
- Episode-read slice verification:
  - `cargo fmt --manifest-path emily/Cargo.toml`
  - `cargo test --manifest-path emily/Cargo.toml -q`
  - `cargo clippy --manifest-path emily/Cargo.toml --all-targets -- -D warnings`
  - Runtime episode-read coverage in `emily/src/runtime/episode_tests.rs`
- Explicit remote-state slice verification:
  - `cargo fmt --manifest-path emily/Cargo.toml`
  - `cargo test --manifest-path emily/Cargo.toml -q`
  - `cargo clippy --manifest-path emily/Cargo.toml --all-targets -- -D warnings`
  - Runtime remote-state coverage in `emily/src/runtime/sovereign_tests.rs`
- Plan reviewed against:
  - `PLAN-STANDARDS.md`
  - `ARCHITECTURE-PATTERNS.md`
  - `CODING-STANDARDS.md`
  - `DOCUMENTATION-STANDARDS.md`
  - `TESTING-STANDARDS.md`
  - `CONCURRENCY-STANDARDS.md`
  - `DEPENDENCY-STANDARDS.md`
  - [GESTALT-STANDARDS.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/GESTALT-STANDARDS.md)

### Traceability Links

- Module README updated:
  - `emily/src/README.md`
  - `emily/src/model/README.md`
  - `emily/src/runtime/README.md`
  - `emily/src/inference/README.md`
  - `emily/src/inference/pantograph/README.md`
  - `emily/src/store/README.md`
  - `emily/src/store/surreal/README.md`
- ADR added/updated: None identified as of 2026-03-08.
- Reason: current work preserved the public facade while performing internal decomposition.
- Revisit trigger: first milestone that changes public facade or package structure.
- PR notes completed per `templates/PULL_REQUEST_TEMPLATE.md`: N/A for this planning step

## Brevity Note

This plan is intentionally more explicit than the default because the crate owns persisted artifacts, async runtime behavior, and future cross-layer contracts. Further detail should be added only when a milestone reaches active implementation.
