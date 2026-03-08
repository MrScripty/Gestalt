# Plan: Emily Membrane Crate Design

## Objective

Define a standards-aligned design and implementation plan for a sibling
`emily-membrane` crate that realizes Emily's sovereign-dispatch layer without
polluting the existing `emily` core crate with transport, provider, or membrane
runtime concerns.

## Scope

### In Scope

- Defining the architectural role of a sibling membrane crate
- Defining the boundary between `emily` and `emily-membrane`
- Designing the initial public contracts for bounded task compilation, routing,
  dispatch, validation, and local reconstruction
- Planning the first implementation milestones for a membrane runtime that can
  consume `emily` and persist sovereign records back into it
- Aligning the crate plan with shared coding standards and Gestalt repo
  standards

### Out of Scope

- Full implementation of the March 2026 sovereign-cognition architecture in one
  pass
- Changing the current `emily` crate into a transport or provider runtime
- Gestalt UI redesign
- Public network listeners or external service deployment
- Finalizing every product-policy decision for routing, retry, or trust logic

## Inputs

### Problem

The repo now has a significantly hardened `emily` core crate for durable
memory, retrieval, episode/state tracking, `EARL`, `ECGL`, sovereign records,
and write-side audits. The missing architectural layer is the sovereign
dispatch runtime described in the March 2026 paper set: bounded membrane IR,
provider routing, local validation, local rendering, and transport-aware remote
reasoning orchestration. That layer should exist, but it should not be placed
inside `emily/src` because it has a different dependency profile, faster rate
of change, and more volatile runtime concerns.

### Constraints

- The new membrane layer must be a sibling crate, not a new directory under
  `emily/src`.
- `emily` remains the durable source of truth for memory, policy, sovereign
  records, and audit state.
- The membrane crate must depend on `emily`; `emily` must not depend on the
  membrane crate.
- The membrane crate should stay lean enough to remain reviewable, but it may
  carry heavier orchestration dependencies than `emily` because it is a leaf
  orchestration/runtime layer.
- Rust files should target `<= 500` lines and trigger decomposition review if
  they exceed the threshold.
- Public contracts and crate boundaries must be documented.
- Async/runtime ownership must follow `CONCURRENCY-STANDARDS.md`:
  - explicit startup and shutdown ownership
  - no lock held across `.await`
  - bounded queues
  - message passing preferred over shared mutable state
  - restart and overlap races must be called out in advance
- Verification must follow current repo standards:
  - `cargo fmt`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test -q`
- The membrane crate should not assume Gestalt is the only host forever, even
  if Gestalt is the first consumer.

### Assumptions

- The membrane layer is required for the full intended sovereign-cognition
  architecture, but `emily` remains useful without it.
- The membrane crate is an application/orchestration layer, not a core/domain
  layer.
- `Semantic Membrane` IR, provider routing, and local reconstruction should be
  treated as executable boundary contracts, not just informal types.
- Early membrane work should prefer local process orchestration and existing
  workspace dependencies over introducing new external services.
- The first membrane implementation should integrate with Emily's current
  sovereign records:
  - routing decisions
  - remote episodes
  - validation outcomes
  - sovereign audit records

### Dependencies

- Research docs in `docs/emily-research/`, especially:
  - `01-executive-summary.md`
  - `02-architecture-reconstruction.md`
  - `03-local-reproduction-blueprint.md`
  - `04-evidence-and-sources.md`
  - `09-emily-crate-continuation-plan.md`
- Shared standards in `/media/jeremy/OrangeCream/Linux Software/Coding-Standards/`
- Repo-specific rules in [GESTALT-STANDARDS.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/GESTALT-STANDARDS.md)
- The current `emily` crate public surface
- Existing Pantograph crates already used by Gestalt

### Affected Structured Contracts

- New crate boundary: `emily-membrane`
- `emily::api::EmilyApi` as the durable state dependency
- Future membrane contracts for:
  - membrane compile request/result
  - routing plan / routing target selection
  - dispatch request/result
  - validation request/result
  - reconstruction/render request/result
  - leakage-budget accounting
- Gestalt integration points that choose when to invoke the membrane runtime

### Affected Persisted Artifacts

- No new persisted artifacts are required for the plan itself.
- Implementation is expected to reuse and extend Emily-owned persisted records:
  - routing decisions
  - remote episodes
  - validation outcomes
  - sovereign audits
- If membrane-specific durable artifacts become necessary later, they should be
  justified explicitly and preferably persisted through `emily`.

### Concurrency And Race-Risk Review

The membrane crate will introduce more volatile runtime behavior than `emily`.
Implementation work must explicitly review:

- who owns membrane runtime startup and shutdown
- whether one host can run multiple membrane sessions concurrently
- queue capacity and drop policy for dispatch work
- overlap prevention for duplicate dispatches of the same episode
- cancellation signaling for in-flight remote tasks
- stale result handling when a provider returns after cancellation
- lifecycle of ephemeral legend/handle state
- destruction timing for membrane-local mapping state
- whether validation and reconstruction are synchronous or worker-backed

### Risks

| Risk | Impact | Mitigation |
| ---- | ------ | ---------- |
| Membrane logic leaks provider/runtime concerns into `emily` | High | Keep the membrane as a sibling crate that depends on `emily`, not the reverse |
| The membrane crate becomes Gestalt-specific too early | High | Define host-facing contracts that are transport-agnostic and keep Gestalt mapping in adapters |
| Early IR design calcifies before real dispatch flows exist | High | Keep initial IR narrow, executable, and append-only; defer speculative fields |
| Audit and lifecycle semantics drift between crates | High | Require membrane writes to flow through Emily's public sovereign APIs |
| Runtime task ownership becomes unclear | High | Document startup/shutdown/cancellation ownership before implementation |
| Dependency growth becomes uncontrolled | Medium | Prefer existing workspace crates, feature-gate heavy integrations, and justify new dependencies in writing |
| Security assumptions are implied rather than encoded | Medium | Treat boundary contracts as explicit and document what is local-only vs provider-visible |

## Definition of Done

- The membrane crate has a documented package role and a clear dependency
  direction relative to `emily`.
- The plan defines a credible first implementation slice without forcing
  speculative product-policy decisions.
- The plan records concurrency ownership, dependency expectations, and re-plan
  triggers.
- The plan is aligned with shared and repo-specific standards.
- The Emily docs and continuation plan no longer leave the membrane-vs-core
  boundary ambiguous.

## Ownership And Lifecycle Note

The membrane runtime should be owned by the host application that composes it.

That means:

- the host creates the membrane runtime
- the host injects an `EmilyApi` dependency into it
- the host starts any background dispatch workers
- the host owns cancellation signaling and shutdown order
- the membrane runtime must not start detached tasks without tracked ownership

No implementation milestone should introduce background dispatch or retry loops
without documenting:

1. who starts them
2. who stops them
3. how cancellation propagates
4. how duplicate dispatch overlap is prevented
5. what membrane-local state is destroyed vs persisted

## Public Facade Preservation Note

This plan assumes facade-first preservation for `emily`.

That means:

- new membrane behavior should sit behind new membrane-crate contracts
- `emily` remains append-only where possible
- membrane logic uses Emily's public APIs rather than reaching into store
  internals
- if membrane requirements force breaking changes in `emily`, stop and re-plan
  explicitly before implementation

## Package Role And Boundary

Recommended package name: `emily-membrane`

Recommended role: application/orchestration crate inside the repo, not a
library-core crate.

Boundary rule:

- `emily` owns durable memory, retrieval, policy state, sovereign records, and
  audits
- `emily-membrane` owns bounded task compilation, provider routing, dispatch
  execution, validation orchestration, reconstruction orchestration, and
  membrane-local ephemeral state

Dependency direction:

- `gestalt` -> `emily`
- `gestalt` -> `emily-membrane`
- `emily-membrane` -> `emily`
- `emily` -/-> `emily-membrane`

## Recommended Internal Structure

Initial structure should stay small and role-based:

- `contracts/`
  - membrane IR DTOs
  - routing plan DTOs
  - dispatch result DTOs
  - validation envelopes
  - reconstruction inputs/outputs
- `runtime/`
  - membrane runtime facade
  - worker ownership and lifecycle
  - dispatch orchestration
  - retry/cancel coordination
- `compiler/`
  - bounded membrane IR compilation from host task + Emily context
- `router/`
  - local-only vs remote selection
  - provider target selection
- `validator/`
  - local structural validation
  - `ECCR`-style placeholder hooks
- `renderer/`
  - local reconstruction / legend mapping interfaces
- `providers/`
  - provider adapter trait
  - Pantograph-backed first implementation if selected

The first implementation should avoid creating all of these unless needed.
Start narrow and split only when responsibilities become real.

## Milestones

### Milestone 1: Boundary And Crate Skeleton

**Goal:** Create the crate boundary and minimal contracts without committing to
provider-specific runtime behavior.

**Tasks:**
- [ ] Add a sibling crate directory and Cargo manifest for `emily-membrane`
- [ ] Define the crate role in README/module docs
- [ ] Add a minimal runtime facade that depends on `emily::api::EmilyApi`
- [ ] Define the first boundary contracts:
  - membrane compile request/result
  - routing plan
  - dispatch intent
  - validation envelope
  - reconstruction request/result
- [ ] Keep contracts append-only and executable

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- review crate README and package boundary against `ARCHITECTURE-PATTERNS.md`

**Status:** Completed on 2026-03-08 through Milestone 1 implementation commits

### Milestone 2: Local-Only Membrane Path

**Goal:** Prove the membrane facade can orchestrate a task without remote
dispatch.

**Tasks:**
- [ ] Implement a local-only routing path that produces a routing decision in
  Emily
- [ ] Compile a bounded membrane request shape even when no remote call occurs
- [ ] Produce validation and audit writes through Emily's public sovereign APIs
- [ ] Return a reconstruction/result envelope to the host
- [ ] Add acceptance tests for the local-only path

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- acceptance test from membrane facade input to Emily sovereign writes

**Status:** Completed on 2026-03-08 through Milestone 1 local-only runtime commits

### Milestone 3: First Remote Dispatch Adapter

**Goal:** Add one provider-backed dispatch path without making the crate
 provider-specific overall.

**Tasks:**
- [ ] Define a provider adapter trait owned by the membrane crate
- [ ] Add one first adapter using existing workspace capabilities
- [ ] Persist route, remote episode, validation outcome, and audits through
  Emily APIs only
- [ ] Keep remote closure deterministic and replay-safe
- [ ] Add cancellation and stale-result handling rules

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- acceptance test for route -> remote episode -> validation -> reconstruction
- idempotency/cancellation coverage

**Status:** Not started

### Milestone 4: Validation And Reconstruction Hardening

**Goal:** Keep final answer assembly local and auditable.

**Tasks:**
- [ ] Add local validation helpers that consume remote results
- [ ] Add reconstruction/render contracts that preserve local authority over the
  final answer
- [ ] Define how legend/handle state is created and destroyed
- [ ] Persist meaningful validation and boundary audit records through Emily
- [ ] Document what remains local-only vs provider-visible

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- acceptance tests for validation failure and reconstruction fallback
- audit coverage tests

**Status:** Not started

### Milestone 5: Host Integration Slice

**Goal:** Prove one real Gestalt flow can use the membrane crate without
 coupling layers incorrectly.

**Tasks:**
- [ ] Choose one Gestalt flow as the first membrane consumer
- [ ] Add an adapter from Gestalt orchestration state into membrane input
- [ ] Ensure Gestalt remains the composition root
- [ ] Add diagnostics for routing path, validation result, and Emily writes
- [ ] Document manual verification for the first host-integrated flow

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- at least one cross-layer acceptance path from Gestalt host input to Emily
  sovereign records

**Status:** Not started

## Execution Notes

Update during implementation:
- 2026-03-08: Initial membrane-crate design plan written after reviewing shared
  standards, Gestalt standards, and Emily research docs.
- 2026-03-08: First adapter decision recorded:
  - start with a narrow internal test adapter
  - defer Pantograph-backed provider work to the later remote-adapter milestone

## Commit Cadence Notes

- Commit when a logical slice is complete and verified.
- Keep membrane feature commits separate from documentation plan updates.
- Follow commit format/history cleanup rules from `COMMIT-STANDARDS.md`.

## Optional Subagent Assignment

| Owner/Agent | Scope | Output Contract | Handoff Checkpoint |
| ----------- | ----- | --------------- | ------------------ |
| None | None assigned | Reason: boundary design is still the dominant task | Revisit if implementation splits into low-coupling streams such as contracts vs provider adapter work |

## Re-Plan Triggers

- The first real provider adapter requires dependencies too heavy for the
  membrane crate as currently designed
- Gestalt proves to be too opinionated for the initial membrane contracts
- Emily requires breaking API changes to support membrane work
- Membrane-local state proves it must be durably persisted outside current Emily
  records
- The local-only slice fails to provide a useful executable contract
- The security boundary requires a separate contracts crate instead of a single
  membrane crate

## Recommendations

- Prefer `emily-membrane` as a sibling crate rather than adding a directory
  under `emily/src`; the boundary is architectural, not just organizational.
- Prefer a local-only membrane path before the first remote adapter so the
  compile/route/validate/reconstruct contract can stabilize without provider
  noise.
- Prefer writing membrane outputs back through Emily's public APIs instead of
  creating membrane-owned durable stores early.

## Completion Summary

### Completed

- Initial membrane-crate design plan drafted
- Sibling-crate boundary recommendation recorded

### Deviations

- No implementation has started yet.

### Follow-Ups

- Decide whether the first membrane integration target in Gestalt should be the
  local-agent path or a separate bounded experiment path.
- Decide whether membrane contracts should live entirely in `emily-membrane` or
  whether a tiny `contracts` crate becomes necessary later.

### Verification Summary

- Standards review against:
  - `PLAN-STANDARDS.md`
  - `ARCHITECTURE-PATTERNS.md`
  - `DEPENDENCY-STANDARDS.md`
  - `CONCURRENCY-STANDARDS.md`
  - [GESTALT-STANDARDS.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/GESTALT-STANDARDS.md)
- Research review against:
  - [01-executive-summary.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/01-executive-summary.md)
  - [02-architecture-reconstruction.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/02-architecture-reconstruction.md)
  - [03-local-reproduction-blueprint.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/03-local-reproduction-blueprint.md)
  - [04-evidence-and-sources.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/04-evidence-and-sources.md)
  - [09-emily-crate-continuation-plan.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/09-emily-crate-continuation-plan.md)

### Traceability Links

- Module README updated: N/A
- ADR added/updated: N/A
- PR notes completed per `templates/PULL_REQUEST_TEMPLATE.md`: N/A

## Brevity Note

This plan is more explicit than a normal short plan because the membrane crate
introduces a new package boundary, new runtime lifecycle concerns, and a new
dependency direction for the Emily architecture.
