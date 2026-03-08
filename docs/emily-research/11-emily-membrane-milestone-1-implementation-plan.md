# Plan: Emily Membrane Milestone 1 Implementation

## Objective

Implement Milestone 1 of the membrane work by creating a sibling
`emily-membrane` crate with a minimal, standards-aligned boundary and a narrow
internal test adapter that proves the crate shape without introducing real
provider transport complexity yet.

## Scope

### In Scope

- Creating the `emily-membrane` crate as a sibling package in the Gestalt repo
- Defining the first membrane contracts as executable DTOs
- Adding a minimal membrane runtime facade that depends on `emily::api::EmilyApi`
- Adding a narrow internal test adapter for local contract proving
- Adding crate and `src/` READMEs required by the documentation standards
- Adding tests that prove the crate boundary and local-only execution path

### Out of Scope

- Real provider-backed remote dispatch
- Pantograph integration
- Gestalt host integration
- New persisted artifact families beyond current Emily-owned records
- Final routing or retry policy semantics
- Membrane-local background workers beyond what is needed for the minimal facade

## Inputs

### Problem

The membrane design plan now establishes that sovereign-dispatch execution
belongs in a sibling crate above `emily`. The next task is to implement the
crate boundary itself in a way that is strongly aligned with shared standards,
preserves the current `emily` contract, and avoids premature commitment to one
provider runtime. Starting with a narrow internal test adapter reduces design
risk and keeps the first slice focused on contracts, lifecycle, and layering.

### Constraints

- The crate must live as a sibling package, not inside `emily/src`.
- The crate must depend on `emily`; `emily` must not depend on it.
- Public contracts must be documented and typed.
- Direct provider or transport dependencies should be deferred in this
  milestone.
- Rust files should target `<= 500` lines and be decomposed by responsibility.
- Every `src/` directory in the new crate must include a `README.md`.
- Verification must include:
  - `cargo fmt`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test -q`
- Cross-layer acceptance checks are required because this work crosses a crate
  boundary and consumes Emily's public API.

### Assumptions

- The first membrane runtime can be useful even if it only executes a local-only
  path.
- A narrow internal test adapter is the right first adapter because the goal of
  this milestone is boundary validation, not provider realism.
- The membrane runtime should write all durable sovereign state through Emily's
  public APIs rather than creating direct store dependencies.
- The first host of the membrane crate will still be Gestalt, but the crate
  should not encode Gestalt-specific UI or transport assumptions.

### Dependencies

- [10-emily-membrane-crate-design-plan.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/10-emily-membrane-crate-design-plan.md)
- [09-emily-crate-continuation-plan.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/09-emily-crate-continuation-plan.md)
- `emily` crate public surface
- Shared standards in `/media/jeremy/OrangeCream/Linux Software/Coding-Standards/`
- Repo rules in [GESTALT-STANDARDS.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/GESTALT-STANDARDS.md)

### Affected Structured Contracts

- New crate package boundary: `emily-membrane`
- New public contracts for:
  - membrane task request
  - membrane compile result
  - routing plan
  - local dispatch result
  - validation envelope
  - reconstruction result
- New runtime facade contract for the membrane crate
- Emily API consumption for:
  - route writes
  - validation writes
  - sovereign audit writes

### Affected Persisted Artifacts

- No membrane-owned persisted artifact families in this milestone
- Emily-owned persisted records written through the membrane facade:
  - routing decisions
  - validation outcomes
  - sovereign audits

### Concurrency And Race-Risk Review

Milestone 1 should avoid unnecessary background concurrency. The minimal rules
for this slice are:

- no long-lived worker loops
- no provider transport sessions
- no detached background tasks
- synchronous orchestration through async facade methods only
- no overlapping local-only dispatch for the same membrane call inside one
  runtime method

If implementation pressure pushes toward background workers or retry loops, stop
and re-plan because that is outside Milestone 1.

### Risks

| Risk | Impact | Mitigation |
| ---- | ------ | ---------- |
| The crate starts as a disguised Gestalt adapter | High | Keep inputs generic and avoid UI/session assumptions in public contracts |
| Contracts become too speculative | High | Limit to the minimum local-only path needed for compile/route/validate/reconstruct |
| The internal test adapter becomes production architecture by accident | Medium | Keep it in clearly named internal/test-oriented modules and document Pantograph as later work |
| README requirements are missed during fast crate setup | Medium | Create README files as part of the initial skeleton task, not as cleanup |
| Membrane writes bypass Emily public APIs | High | Add acceptance tests that verify writes flow through Emily facade usage |

## Definition of Done

- `emily-membrane` exists as a sibling crate with a documented package role.
- The crate builds and tests pass under repo standards.
- The first public contracts are narrow, documented, and executable.
- The runtime facade depends on `emily::api::EmilyApi`, not Emily internals.
- A local-only acceptance test proves the membrane runtime can produce durable
  Emily sovereign writes without remote transport.
- Required README files exist and satisfy documentation standards.

## Ownership And Lifecycle Note

Milestone 1 keeps lifecycle simple:

- the host constructs the membrane runtime
- the host injects an Emily API dependency
- each membrane call runs to completion in one async path
- no background workers are introduced
- no persistent in-memory legend/handle cache survives beyond the call

If any implementation slice needs:

1. background queues
2. retry loops
3. cancellation tokens for in-flight provider work
4. stateful mapping caches across calls

then that work belongs in a later milestone and must be re-planned.

## Public Facade Preservation Note

This implementation plan assumes:

- no breaking changes to `emily`
- append-only membrane contracts
- no direct dependency on `emily` store implementations
- Emily remains the only durable state authority in this slice

## Milestones

### Milestone 1A: Crate Skeleton

**Goal:** Create the package, manifest, documentation skeleton, and public
entrypoints.

**Tasks:**
- [ ] Add `emily-membrane/` with `Cargo.toml`
- [ ] Add `src/lib.rs`
- [ ] Add crate README and `src/README.md`
- [ ] Decide on initial module tree:
  - `contracts`
  - `runtime`
  - `test_support` if needed
- [ ] Keep the initial dependency set minimal and justified

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- README review against `DOCUMENTATION-STANDARDS.md`

**Status:** Completed on 2026-03-08 in commit `e95f341`

### Milestone 1B: Minimal Contracts

**Goal:** Define the narrow executable contracts for local-only membrane work.

**Tasks:**
- [ ] Add DTOs for:
  - membrane task request
  - compile result
  - routing plan
  - dispatch result
  - validation envelope
  - reconstruction result
- [ ] Keep fields narrow and append-only
- [ ] Document omitted-field semantics where defaults matter
- [ ] Add serialization roundtrip tests for public DTOs

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- DTO roundtrip tests

**Status:** Completed on 2026-03-08 in commit `a629856`

### Milestone 1C: Runtime Facade And Internal Test Adapter

**Goal:** Add the minimal membrane runtime plus a narrow internal adapter that
proves the crate shape.

**Tasks:**
- [ ] Define a membrane runtime facade that accepts an `EmilyApi` dependency
- [ ] Add a narrow internal adapter for local-only dispatch simulation
- [ ] Ensure the adapter is explicitly documented as non-provider infrastructure
- [ ] Keep all logic deterministic and replay-safe
- [ ] Avoid background workers and transport state

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- unit tests for local-only routing and reconstruction behavior

**Status:** Completed on 2026-03-08 in commit `bab8c90`

### Milestone 1D: Emily Write Path Acceptance

**Goal:** Prove the membrane crate can emit a useful local-only sovereign flow
through Emily's public APIs.

**Tasks:**
- [ ] Use Emily APIs to record a local-only routing decision
- [ ] Use Emily APIs to record validation and audit artifacts for the local-only
  flow
- [ ] Return a reconstruction/result envelope to the caller
- [ ] Add at least one cross-crate acceptance test from membrane input to Emily
  sovereign writes
- [ ] Ensure no direct use of Emily store internals in the acceptance path

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- one acceptance test from membrane request to persisted Emily sovereign state
- one replay/idempotency test for repeated local-only membrane execution

**Status:** Not started

## Execution Notes

Update during implementation:
- 2026-03-08: Milestone 1 implementation plan written.
- 2026-03-08: First adapter decision fixed to a narrow internal test adapter.
- 2026-03-08: Milestone 1A crate skeleton implemented in commit `e95f341`.
- 2026-03-08: Verification passed with:
  - `cargo fmt --manifest-path emily-membrane/Cargo.toml`
  - `cargo clippy --manifest-path emily-membrane/Cargo.toml --all-targets --offline -- -D warnings`
  - `cargo test --manifest-path emily-membrane/Cargo.toml -q --offline`
- 2026-03-08: Milestone 1B minimal contracts implemented in commit `a629856`.
- 2026-03-08: DTO verification passed with:
  - `cargo fmt --manifest-path emily-membrane/Cargo.toml`
  - `cargo clippy --manifest-path emily-membrane/Cargo.toml --all-targets --offline -- -D warnings`
  - `cargo test --manifest-path emily-membrane/Cargo.toml -q --offline`
- 2026-03-08: Milestone 1C runtime facade and internal test adapter implemented
  in commit `bab8c90`.
- 2026-03-08: Runtime verification passed with:
  - `cargo fmt --manifest-path emily-membrane/Cargo.toml`
  - `cargo clippy --manifest-path emily-membrane/Cargo.toml --all-targets --offline -- -D warnings`
  - `cargo test --manifest-path emily-membrane/Cargo.toml -q --offline`

## Commit Cadence Notes

- Commit after each logical sub-slice is complete and verified.
- Keep crate-skeleton, contract, runtime, and documentation commits reviewable.
- Keep documentation-only commits separate from feature commits.
- Follow `COMMIT-STANDARDS.md` and include `Agent: codex`.

## Optional Subagent Assignment

| Owner/Agent | Scope | Output Contract | Handoff Checkpoint |
| ----------- | ----- | --------------- | ------------------ |
| None | None assigned | Reason: Milestone 1 is still tightly coupled around one crate boundary | Revisit if contracts and runtime split into independent low-coupling streams |

## Re-Plan Triggers

- Creating the crate requires breaking `emily` API changes
- The minimal local-only path requires provider-specific assumptions
- The internal test adapter needs durable state outside Emily-owned records
- Background workers become necessary before Milestone 1 acceptance is complete
- The public contract set starts expanding beyond the narrow local-only use case

## Recommendations

- Prefer mirrored test placement for integration/acceptance checks under a
  dedicated `tests/` directory if the crate grows past a handful of modules.
- Prefer colocated unit tests for small pure contract helpers if they remain
  near the defining module.
- Prefer constants for enum labels, event names, and default routing tags rather
  than ad hoc string literals.

## Completion Summary

### Completed

- Milestone 1 implementation plan drafted

### Deviations

- No implementation has started yet.

### Follow-Ups

- Decide the exact crate name in Cargo before code is written:
  - `emily-membrane`
  - or another sibling name if workspace naming conventions suggest otherwise
- Decide whether Milestone 1 acceptance tests should live inside the crate or in
  a top-level workspace test harness.

### Verification Summary

- Standards review against:
  - `PLAN-STANDARDS.md`
  - `TESTING-STANDARDS.md`
  - `DOCUMENTATION-STANDARDS.md`
  - `CONCURRENCY-STANDARDS.md`
  - `DEPENDENCY-STANDARDS.md`
  - `COMMIT-STANDARDS.md`
  - [GESTALT-STANDARDS.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/GESTALT-STANDARDS.md)
- Research alignment review against:
  - [10-emily-membrane-crate-design-plan.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/10-emily-membrane-crate-design-plan.md)
  - [02-architecture-reconstruction.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/02-architecture-reconstruction.md)
  - [03-local-reproduction-blueprint.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/03-local-reproduction-blueprint.md)

### Traceability Links

- Module README updated: N/A
- ADR added/updated: N/A
- PR notes completed per `templates/PULL_REQUEST_TEMPLATE.md`: N/A

## Brevity Note

This plan is intentionally more detailed than the design plan because it is the
first implementation slice for a new crate boundary and needs concrete
standards-aligned execution guardrails.
