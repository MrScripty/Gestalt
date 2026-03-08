# Plan: Emily Membrane Routing Policy

## Objective

Define a standards-aligned plan for evolving `emily-membrane` from deterministic
registry lookup into Emily-informed sovereign routing policy that can choose,
defer, caution, or reject remote dispatch in a way that matches the intended
Emily architecture.

## Scope

### In Scope

- Defining the first routing-policy layer above the current provider registry
- Aligning routing behavior with the Emily research stack:
  - `EMEB`
  - `EARL`
  - `ECCR`
  - `ECGL`
  - `Semantic Membrane`
- Defining how routing policy should consume Emily state without pushing
  membrane orchestration into the `emily` crate
- Planning typed contracts, runtime flow, and verification for the first policy
  slice
- Recording clear boundaries between deterministic infrastructure and
  higher-order sovereign policy

### Out of Scope

- Full `Semantic Membrane` IR compilation
- Multi-provider quorum or ensemble dispatch
- Retry, backoff, or queue-worker policy
- Gestalt UI integration
- Autonomous self-modifying routing heuristics
- Final `AOPO/APC` implementation

## Inputs

### Problem

The membrane crate can now resolve registered providers and select a matching
target from registry metadata, but the current behavior is still host-assisted
and deterministic. That is below the intended Emily design. The research docs
describe Emily as a sovereign cognition layer where dispatch decisions are
shaped by epistemic control, continuity, and bounded trust, not just static
host preference plus registry sorting.

### Constraints

- `emily` remains the durable memory/policy core and source of truth for:
  - episodes
  - `EARL` evaluations
  - routing decisions
  - remote episodes
  - validation outcomes
  - sovereign audits
- `emily-membrane` remains the orchestration layer and must not move transport
  or runtime concerns back into `emily`.
- The first routing-policy slice must stay deterministic and reviewable.
- Public contracts must remain typed, append-only, and host-agnostic.
- Policy must prefer explicit factors over opaque scoring.
- Rust files should target `<= 500` lines and trigger decomposition review when
  they exceed the threshold.
- Verification must include:
  - `cargo fmt`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test -q`
- Any async or cached policy state must follow
  `CONCURRENCY-STANDARDS.md`.

### Assumptions

- Autonomous Emily routing should emerge as membrane policy informed by Emily
  state, not as hardcoded Gestalt host logic.
- The first useful routing-policy slice should be conservative and
  explainable, not "smart" in an opaque way.
- `EARL` should influence whether remote dispatch is allowed, cautioned, or
  blocked before provider selection occurs.
- Provider choice should initially use explicit factors rather than learned
  weights.
- `ECCR` and `AOPO/APC` should be represented as future hooks, not invented in
  full before the underlying validation/runtime layers exist.

### Dependencies

- [01-executive-summary.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/01-executive-summary.md)
- [02-architecture-reconstruction.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/02-architecture-reconstruction.md)
- [09-emily-crate-continuation-plan.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/09-emily-crate-continuation-plan.md)
- [10-emily-membrane-crate-design-plan.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/10-emily-membrane-crate-design-plan.md)
- [12-emily-membrane-milestone-3-implementation-plan.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/12-emily-membrane-milestone-3-implementation-plan.md)
- Shared standards in `/media/jeremy/OrangeCream/Linux Software/Coding-Standards/`
- Repo rules in [GESTALT-STANDARDS.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/GESTALT-STANDARDS.md)

### Affected Structured Contracts

- New membrane routing-policy request/result contracts
- Additive registry metadata for routing relevance
- Additive runtime APIs for:
  - policy evaluation
  - route recommendation
  - route explanation
  - execution through policy-selected targets
- Emily API read usage for:
  - episode state
  - `EARL` evaluation state
  - routing history
  - validation history
  - sovereign record state as needed for deterministic policy inputs

### Affected Persisted Artifacts

- No new persisted artifact family is required for the plan itself.
- Implementation should prefer existing Emily-owned artifacts:
  - routing decisions
  - remote episodes
  - validation outcomes
  - audits
- If durable policy snapshots become necessary later, they should be justified
  explicitly and ideally persisted through `emily`.

### Concurrency And Race-Risk Review

The first routing-policy slice should avoid background concurrency. The rules
for that slice are:

- one policy evaluation per runtime call
- no background scoring workers
- no cached mutable policy state unless ownership is explicit
- no overlapping dispatch fanout during first policy rollout
- no hidden async refresh loop for provider registry metadata

If routing policy needs:

1. background telemetry refresh
2. rolling provider health windows
3. concurrent multi-target comparison with shared mutable state
4. retry queues or delayed reconsideration

then stop and re-plan before implementation.

### Risks

| Risk | Impact | Mitigation |
| ---- | ------ | ---------- |
| Routing policy becomes opaque too early | High | Use named factors, typed findings, and deterministic ordering |
| Policy logic leaks into `emily` core | High | Keep routing evaluation in `emily-membrane` and consume Emily state through the public facade |
| The first policy slice overclaims `ECCR` or `AOPO/APC` behavior | High | Treat them as hooks and deferred contract points, not completed systems |
| Provider metadata becomes ad hoc and untrustworthy | Medium | Define explicit registration fields and default semantics |
| `EARL` and routing semantics conflict | Medium | Make `EARL` gating a first-class precondition before target scoring |
| Too much policy gets inferred from incomplete research | Medium | Start with only strongly supported choices and document what remains deferred |

## Definition of Done

- The plan defines a credible first routing-policy slice that moves beyond
  deterministic "first match" selection.
- The plan preserves the boundary where `emily` provides durable policy inputs
  and `emily-membrane` owns dispatch policy execution.
- The plan identifies the minimum explicit factors for target scoring,
  rejection, and caution.
- The plan records verification, lifecycle constraints, and re-plan triggers.
- The plan stays aligned with the Emily research docs rather than treating
  provider selection as a generic infrastructure problem.

## Ownership And Lifecycle Note

The host continues to own membrane runtime lifecycle:

- the host constructs the runtime
- the host injects `EmilyApi`
- the host injects a provider registry
- the host invokes routing-policy evaluation per request

The first routing-policy slice must not introduce:

- detached policy workers
- persistent mutable routing state
- background provider-health polling

without explicit lifecycle ownership and shutdown rules.

## Public Facade Preservation Note

This plan assumes append-only membrane evolution:

- preserve current direct-target execution APIs
- add routing-policy APIs beside them
- keep the registry and routing-policy contracts typed and explainable
- keep Emily API usage facade-first rather than reaching into Emily internals

If routing policy requires breaking changes to current membrane contracts, stop
and re-plan explicitly before implementation.

## Research Alignment Summary

The research docs imply the following routing posture:

- `EARL` is a pre-cognitive gate and should constrain or block remote dispatch
  before provider selection.
- `Semantic Membrane` means routing is not only "which provider?" but also
  "should a bounded remote dispatch happen at all?"
- `ECCR` implies local coherence/relevance/confidence checks should shape
  remote execution policy and later validation, but the first routing slice can
  only add hooks and typed findings, not full `ECCR`.
- `ECGL` implies route outcomes should not directly mutate identity-bearing
  memory without later integration control.
- `AOPO/APC` suggests future retry/mutation behavior, but that should remain
  deferred until the first explicit routing policy is stable.

The first implementation slice should therefore emphasize:

- pre-dispatch gating
- deterministic provider scoring
- explicit rationale
- durable traceability through Emily's existing sovereign records

## Recommended First Policy Model

The first routing-policy slice should be rule-based and additive.

### Required Pre-Selection Gates

- If remote is not allowed by the task contract, return `LocalOnly`.
- If the episode is blocked or reflex-gated by `EARL`, return `Rejected`.
- If the episode is cautioned by `EARL`, allow remote only with a caution flag
  and explicit findings.
- If no provider matches the requested capability profile, return `LocalOnly`
  or `Rejected` with rationale, depending on host policy input.

### Initial Explicit Scoring Factors

- provider/profile exact match
- required capability-tag coverage
- explicit model match when requested
- route compatibility with `EARL` state
- local-only override or high-sensitivity task flag

### Deferred Factors

- learned provider preference
- adaptive performance weighting
- retry mutation strategy
- leakage-budget scoring
- multi-provider ranking by empirical success history
- full `ECCR`-driven routing reshaping

## Milestones

### Milestone 1: Routing Policy Contracts

**Goal:** Define typed policy request/result contracts and explicit route
explanation outputs.

**Tasks:**
- [ ] Add routing-policy request DTOs that combine:
  - task routing preference
  - required capability profile
  - local-only sensitivity flags
  - optional preferred provider/profile hints
- [ ] Add routing-policy result DTOs that can express:
  - `LocalOnly`
  - `SingleRemote`
  - `Rejected`
  - caution state
  - selected target
  - typed findings/rationale
- [ ] Keep contracts append-only and host-agnostic
- [ ] Add serde roundtrip tests

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- README updates for affected membrane directories

**Status:** Completed on 2026-03-08 in commit `790ff02`

### Milestone 2: Policy Evaluation Core

**Goal:** Implement deterministic target scoring and route recommendation.

**Tasks:**
- [ ] Add a routing-policy evaluator module under `emily-membrane`
- [ ] Implement explicit matching and scoring rules with named constants
- [ ] Separate hard rejection gates from soft ranking
- [ ] Return typed findings instead of opaque numbers alone
- [ ] Add deterministic ordering and tie-breaking rules

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- unit tests for rejection, caution, ranking, and deterministic ties

**Status:** Completed on 2026-03-08 in commit `fbe7a89`

### Milestone 3: Emily-Informed Gating

**Goal:** Use Emily state as policy input without moving routing into the core
crate.

**Tasks:**
- [ ] Define a small membrane-owned policy input snapshot composed from
  `EmilyApi` reads
- [ ] Incorporate episode state and `EARL` state into route evaluation
- [ ] Define conservative behavior for missing Emily state
- [ ] Add acceptance coverage proving Emily state can block or caution remote
  routing before dispatch

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- acceptance tests from Emily state -> routing result -> durable sovereign
  records

**Status:** Completed on 2026-03-08 in commit `0eafbad`

### Milestone 4: Runtime Integration

**Goal:** Make policy selection the preferred host-facing membrane path.

**Tasks:**
- [ ] Add runtime helper methods that evaluate policy then execute with the
  selected target
- [ ] Preserve direct-target execution APIs for compatibility
- [ ] Ensure policy-selected remote execution still flows through existing
  routing, remote-episode, validation, and audit writes
- [ ] Update crate docs and milestone notes

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- acceptance coverage through the new policy-selected execution path

## Re-Plan Triggers

- The first routing-policy slice requires background provider-health tracking
- `EARL` data needed for routing is not available through the current Emily
  facade
- A real `ECCR` implementation changes routing-result shape materially
- Provider selection needs multi-provider fanout instead of single-target
  recommendation
- Host requirements force policy semantics that contradict the Emily research
  posture

## Execution Notes

Update during implementation:

- 2026-03-08: Routing-policy plan written and indexed in the Emily research
  docs.
- 2026-03-08: Milestone 1 completed in commit `790ff02`.
- 2026-03-08: Milestone 1 scope:
  - Added typed routing-policy request/result DTOs.
  - Added typed routing sensitivity, outcome, and finding enums.
  - Reused `ProviderTarget` as the selected-target contract to preserve
    append-only compatibility with the existing membrane boundary.
  - Added serde roundtrip tests for the new contracts.
  - Updated membrane crate READMEs to reflect the new public contract surface.
- 2026-03-08: Milestone 1 verification passed with:
  - `cargo fmt --manifest-path emily-membrane/Cargo.toml`
  - `cargo test --manifest-path emily-membrane/Cargo.toml -q --offline`
  - `cargo clippy --manifest-path emily-membrane/Cargo.toml --all-targets --offline -- -D warnings`
  - `cargo test --manifest-path emily-membrane/Cargo.toml -q --offline --features pantograph`
  - `cargo clippy --manifest-path emily-membrane/Cargo.toml --all-targets --offline --features pantograph -- -D warnings`
- 2026-03-08: Verification caveat:
  - Feature-gated Pantograph checks still emit upstream warnings from the
    Pantograph workspace, but `emily-membrane` itself passes under
    `-D warnings`.
- 2026-03-08: Milestone 2 completed in commit `fbe7a89`.
- 2026-03-08: Milestone 2 scope:
  - Added a deterministic routing-policy evaluator under
    `emily-membrane/src/runtime/policy.rs`.
  - Added explicit pre-dispatch gates for `allow_remote = false` and
    `RoutingSensitivity::Critical`.
  - Added named scoring constants for provider/profile hints, required
    capability coverage, additional capability coverage, and model presence.
  - Added typed findings and stable route rationale generation.
  - Added deterministic tie-breaking over matching registered targets.
  - Exposed policy evaluation through `MembraneRuntime::evaluate_routing_policy`.
- 2026-03-08: Milestone 2 verification passed with:
  - `cargo fmt --manifest-path emily-membrane/Cargo.toml`
  - `cargo test --manifest-path emily-membrane/Cargo.toml -q --offline`
  - `cargo clippy --manifest-path emily-membrane/Cargo.toml --all-targets --offline -- -D warnings`
  - `cargo test --manifest-path emily-membrane/Cargo.toml -q --offline --features pantograph`
  - `cargo clippy --manifest-path emily-membrane/Cargo.toml --all-targets --offline --features pantograph -- -D warnings`
- 2026-03-08: Milestone 3 completed in commit `0eafbad`.
- 2026-03-08: Milestone 3 scope:
  - Added a membrane-owned Emily policy snapshot composed from episode reads
    plus the latest durable `EARL` evaluation.
  - Added conservative gating for missing episodes, closed episodes, blocked
    episodes, and `EARL REFLEX` states before provider scoring.
  - Added caution propagation for `EARL CAUTION` and already-cautioned episode
    states without moving routing logic into the `emily` crate.
  - Added one additive Emily API read for the latest durable `EARL`
    evaluation per episode.
  - Added acceptance coverage proving Emily `EARL` state can caution or block
    remote routing before provider dispatch and before sovereign writes.
- 2026-03-08: Milestone 3 verification passed with:
  - `cargo fmt --manifest-path emily/Cargo.toml`
  - `cargo fmt --manifest-path emily-membrane/Cargo.toml`
  - `cargo test --manifest-path emily/Cargo.toml -q`
  - `cargo clippy --manifest-path emily/Cargo.toml --all-targets -- -D warnings`
  - `cargo test --manifest-path emily-membrane/Cargo.toml -q --offline`
  - `cargo clippy --manifest-path emily-membrane/Cargo.toml --all-targets --offline -- -D warnings`
  - `cargo test --manifest-path emily-membrane/Cargo.toml -q --offline --features pantograph`
  - `cargo clippy --manifest-path emily-membrane/Cargo.toml --all-targets --offline --features pantograph -- -D warnings`

## Recommendations

- Prefer deterministic, explainable policy before adaptive or learned routing.
- Prefer typed findings over free-form JSON for route rationale.
- Treat `EARL` as the first real sovereign routing input because the research
  supports it strongly.
- Keep `ECCR` and `AOPO/APC` as explicit future hooks rather than guessing
  their implementation details now.

## Completion Criteria

- The plan is aligned with Emily research and shared standards.
- The next routing-policy implementation slice is clearly sequenced.
- The plan keeps the `emily` / `emily-membrane` boundary intact.
- The plan provides enough specificity to implement routing policy in atomic,
  reviewable commits.
