# Plan: Emily Membrane Execution Depth

## Objective

Define the next standards-aligned milestone sequence for deepening
`emily-membrane` beyond policy-selected execution plumbing into a more complete
sovereign boundary layer that better matches the Emily research.

## Scope

### In Scope

- Planning the next membrane-depth slices after routing-policy and unified
  execution milestones
- Defining what remains missing in:
  - membrane compilation / IR
  - local validation
  - retry / mutation control
  - provider policy depth
  - multi-target execution
  - reconstruction depth
- Preserving the boundary between `emily` and `emily-membrane`
- Defining milestone order, constraints, verification, and re-plan triggers

### Out of Scope

- Immediate Gestalt host integration work
- Final product policy for all routing, retry, or governance choices
- Turning `emily-membrane` into a full external service
- Reworking `emily` crate ownership boundaries
- Final `AOPO/APC` or full `ECCR` claims before supporting layers exist

## Inputs

### Problem

The membrane crate now has:

- provider registry support
- deterministic routing policy
- Emily-informed `EARL` gating
- a precise remote-only policy-selected helper
- a broader all-path policy execution facade

That is enough for a clean execution shell, but not enough for the deeper
membrane role described in the Emily research. The remaining work is the part
that turns the membrane from a transport-aware wrapper into a real sovereign
boundary layer.

### Constraints

- `emily` remains the durable core for:
  - memory
  - episode state
  - `EARL`
  - `ECGL`
  - sovereign records
  - audits
- `emily-membrane` remains responsible for:
  - bounded task shaping
  - provider routing
  - dispatch
  - local validation
  - local reconstruction
  - membrane-local ephemeral state
- Rust files should target `<= 500` lines and trigger decomposition review when
  exceeded.
- Any background work must follow `CONCURRENCY-STANDARDS.md`.
- Verification must include:
  - `cargo fmt`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test -q`
- New runtime depth should prefer reuse of existing write paths and public
  contracts over duplicating sovereign persistence logic.

### Assumptions

- The next membrane-depth work should prioritize stronger boundary semantics
  before adaptive heuristics.
- Typed membrane IR is a higher-leverage next step than immediately adding more
  routing complexity.
- Validation depth should precede sophisticated retry/mutation control.
- Multi-target execution should follow stronger IR and validation contracts,
  not precede them.
- Research-aligned depth should remain deterministic and explainable until more
  evidence-backed policy semantics are available.

### Dependencies

- [10-emily-membrane-crate-design-plan.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/10-emily-membrane-crate-design-plan.md)
- [12-emily-membrane-milestone-3-implementation-plan.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/12-emily-membrane-milestone-3-implementation-plan.md)
- [13-emily-membrane-routing-policy-plan.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/13-emily-membrane-routing-policy-plan.md)
- [01-executive-summary.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/01-executive-summary.md)
- [02-architecture-reconstruction.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/02-architecture-reconstruction.md)
- Shared standards in `/media/jeremy/OrangeCream/Linux Software/Coding-Standards/`
- Repo rules in [GESTALT-STANDARDS.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/GESTALT-STANDARDS.md)

### Affected Structured Contracts

- Future membrane IR DTOs
- richer validation result DTOs
- retry / mutation policy DTOs
- provider capability / execution metadata
- multi-target routing / execution DTOs
- reconstruction / provenance envelopes
- membrane telemetry / execution snapshots if justified

### Affected Persisted Artifacts

- Prefer existing Emily-owned persisted artifacts:
  - routing decisions
  - remote episodes
  - validation outcomes
  - sovereign audits
- New persisted membrane artifacts should be introduced only if existing Emily
  records cannot carry the needed durable state cleanly.

### Concurrency And Race-Risk Review

The next membrane-depth slices may require more runtime coordination. Plan for:

- duplicate dispatch prevention by episode/task identity
- cancellation handling for in-flight provider work
- stale result handling after cancellation or timeout
- retry overlap prevention
- bounded retry state and explicit ownership
- no hidden background loops without host-owned lifecycle
- no long-lived lock held across `.await`

If a slice requires:

1. background provider-health refresh
2. rolling adaptive ranking state
3. overlapping multi-target fanout with shared mutable state
4. persistent membrane worker pools

then stop and re-plan that slice explicitly before implementation.

### Risks

| Risk | Impact | Mitigation |
| ---- | ------ | ---------- |
| Membrane depth drifts into speculative architecture | High | Sequence by strongly justified layers: IR, validation, then retry/mutation |
| Validation logic overclaims `ECCR` too early | High | Add typed local validation hooks first and label them as first slices |
| Retry logic creates hidden background complexity | High | Keep retry request-scoped at first and make lifecycle explicit |
| Multi-target work complicates the boundary too early | Medium | Delay fanout until IR and validation contracts are stronger |
| Reconstruction remains too shallow to justify membrane complexity | Medium | Add provenance and explicit reconstruction handles before advanced routing |

## Definition of Done

- The plan identifies the missing membrane-depth layers in implementation order.
- The plan preserves the `emily` / `emily-membrane` boundary.
- The plan defines milestone-sized implementation slices that are testable and
  reviewable.
- The plan records lifecycle, verification, and re-plan triggers.

## Ownership And Lifecycle Note

Future membrane-depth work must keep host ownership explicit:

- the host starts the membrane runtime
- the host injects provider registry and Emily dependencies
- the host owns any later retry workers or dispatch loops
- the host owns cancellation and shutdown sequencing

No milestone should introduce autonomous background work without explicit
startup, shutdown, and overlap rules.

## Public Facade Preservation Note

The membrane crate should evolve append-only where practical:

- keep precise helper APIs when their semantics are narrow
- add broader entrypoints instead of overloading narrow ones
- reuse stable local/remote execution paths rather than duplicating persistence
- keep provider and runtime concerns out of `emily`

## Research Alignment Summary

The research implies the membrane should eventually support:

- bounded membrane representations rather than raw provider prompts
- pre-dispatch gating informed by `EARL`
- local validation that shapes trust in remote outputs
- local reconstruction that preserves sovereignty and provenance
- future retry/mutation behavior aligned with `AOPO/APC`

That suggests the next depth sequence should be:

1. typed membrane IR
2. stronger local validation
3. retry / mutation control
4. richer provider policy
5. multi-target execution
6. deeper reconstruction

## Milestones

### Milestone 6: Typed Membrane IR

**Goal:** Replace "bounded prompt" as the main boundary representation with a
typed membrane IR.

**Tasks:**
- [x] Add membrane IR DTOs for:
  - task payload
  - context handles
  - boundary metadata
  - optional reconstruction handles
- [x] Keep provider transport adapters translating from IR rather than owning
  the only meaningful task representation
- [x] Preserve current simple prompt compilation as one IR rendering mode
- [x] Add serde roundtrip and runtime acceptance tests

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- acceptance coverage from task request -> IR -> execution

**Status:** Completed on 2026-03-08 via `feat(emily-membrane): add typed membrane ir`.

### Milestone 7: Stronger Local Validation

**Goal:** Add a first real local validation layer that is richer than the
current accepted/rejected wrapper.

**Tasks:**
- [ ] Add structured validation categories for:
  - coherence
  - relevance
  - confidence
  - provenance sufficiency
- [ ] Add typed findings and disposition rules
- [ ] Keep claims modest: this is a first slice toward `ECCR`, not full `ECCR`
- [ ] Ensure validation results still map cleanly into Emily validation
  outcomes

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- acceptance tests for caution, review, and rejection paths

### Milestone 8: Request-Scoped Retry And Mutation

**Goal:** Add bounded retry / mutation behavior without introducing hidden
background orchestration.

**Tasks:**
- [ ] Add typed retry policy contracts
- [ ] Add request-scoped retry loop limits
- [ ] Add explicit auditability for retries and mutation attempts
- [ ] Keep retry semantics deterministic and bounded
- [ ] Defer autonomous adaptive retry systems

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- acceptance tests for retry success, retry exhaustion, and cancellation-safe
  behavior

### Milestone 9: Richer Provider Policy

**Goal:** Move provider choice beyond static metadata matching without
introducing opaque heuristics.

**Tasks:**
- [ ] Add explicit provider selection factors such as:
  - capability fit
  - provider metadata class
  - host-declared latency/cost class
  - validation compatibility
- [ ] Add optional telemetry inputs only if lifecycle ownership is explicit
- [ ] Keep tie-breaking deterministic

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- deterministic ranking tests with explained findings

### Milestone 10: Multi-Target Execution

**Goal:** Add the first multi-target execution contracts and bounded fanout
behavior.

**Tasks:**
- [ ] Add multi-target routing and result DTOs
- [ ] Define bounded fanout and reconciliation rules
- [ ] Keep lifecycle and cancellation explicit
- [ ] Persist durable route/remote/validation/audit records through Emily
  without inventing a second durability system

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- acceptance tests for bounded multi-target execution and reconciliation

### Milestone 11: Deeper Reconstruction

**Goal:** Make reconstruction a real local sovereignty layer rather than output
pass-through.

**Tasks:**
- [ ] Add explicit reconstruction handles and provenance references
- [ ] Add local rendering rules for remote outputs and validation findings
- [ ] Keep reconstruction host-agnostic
- [ ] Add audit-relevant provenance for final rendered outputs

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- acceptance tests for provenance-aware reconstruction

## Re-Plan Triggers

- Typed membrane IR requires breaking current public contracts
- Validation semantics begin to require durable artifacts that Emily cannot
  reasonably own
- Retry logic requires background workers rather than request-scoped control
- Multi-target execution requires complex fanout orchestration beyond the
  current lifecycle model
- Gestalt integration pressures force immediate product behavior decisions that
  are not yet architecture-backed

## Recommendations

- Prioritize IR and validation before more routing sophistication.
- Keep retry bounded and request-scoped before considering longer-lived worker
  models.
- Add new facade layers instead of widening narrow helpers.
- Preserve deterministic, explainable behavior until stronger evidence-backed
  policy inputs exist.

## Completion Criteria

- The missing membrane-depth layers are sequenced into concrete milestones.
- The plan remains aligned with the Emily research and coding standards.
- The plan preserves the core membrane boundary rather than leaking concerns
  back into `emily`.
