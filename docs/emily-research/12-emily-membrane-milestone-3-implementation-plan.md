# Plan: Emily Membrane Milestone 3 Implementation

## Objective

Implement Milestone 3 of the membrane work by introducing a standards-aligned
remote-dispatch boundary for `emily-membrane` without collapsing the crate into
one provider implementation. The first slice should define provider-owned
contracts, a provider trait, and a narrow remote execution path that still
persists all durable sovereign state through Emily's public API.

## Scope

### In Scope

- Defining a membrane-owned provider adapter trait
- Defining provider-dispatch request and result contracts
- Adding a first provider-neutral remote execution path to the membrane runtime
- Adding a narrow in-crate test provider for deterministic acceptance coverage
- Planning the Pantograph-backed adapter as a later sub-slice of Milestone 3
- Updating crate documentation and milestone status records

### Out of Scope

- Full `Semantic Membrane` IR compilation
- Leakage-budget enforcement beyond narrow placeholder fields
- Multi-provider fanout or quorum logic
- Background worker pools, retry queues, or cancellation orchestration
- Gestalt host integration
- Pantograph transport implementation in the first Milestone 3 slice

## Inputs

### Problem

Milestone 1 produced a usable local-only membrane facade, but the crate still
has no explicit provider boundary. The next useful step is to create the
remote-dispatch interface that the membrane owns, prove that it composes with
Emily's sovereign record APIs, and keep the first remote path deterministic so
the architecture stays reviewable before Pantograph-specific transport work
lands.

### Constraints

- The provider boundary must live in `emily-membrane`, not in `emily`.
- Durable route, remote episode, validation, and audit writes must continue to
  flow through `emily::api::EmilyApi`.
- Public contracts must remain append-only and transport-agnostic.
- Rust files should stay at or below the repo review threshold and trigger
  decomposition review if they grow past it.
- Every new `src/` directory must include a `README.md`.
- Verification must include:
  - `cargo fmt`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test -q`
- Cross-layer acceptance checks are required because this work crosses the
  membrane-to-Emily boundary.

### Assumptions

- A provider-neutral remote boundary should exist before the Pantograph adapter.
- The first remote provider can be deterministic and test-only as long as it
  proves record flow and replay behavior.
- Emily's current sovereign records are sufficient for the first remote path:
  - routing decisions
  - remote episodes
  - validation outcomes
  - sovereign audits
- Cancellation and retry semantics should remain out of scope until the first
  real transport adapter lands.

### Dependencies

- [10-emily-membrane-crate-design-plan.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/10-emily-membrane-crate-design-plan.md)
- [11-emily-membrane-milestone-1-implementation-plan.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/11-emily-membrane-milestone-1-implementation-plan.md)
- `emily` crate public sovereign APIs
- Shared standards in `/media/jeremy/OrangeCream/Linux Software/Coding-Standards/`
- Repo rules in [GESTALT-STANDARDS.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/GESTALT-STANDARDS.md)

### Affected Structured Contracts

- New membrane provider trait and error surface
- New provider dispatch request/result DTOs
- Runtime facade additions for provider-backed remote execution
- Existing membrane contracts extended for remote execution reporting
- Emily API consumption for:
  - routing writes
  - remote episode writes
  - remote episode state updates
  - validation writes
  - sovereign audit reads for replay checks

### Affected Persisted Artifacts

- No new membrane-owned persisted artifact families
- Emily-owned persisted records written through the membrane remote path:
  - `routing_decisions`
  - `remote_episodes`
  - `validation_outcomes`
  - `audit_records`

### Concurrency And Race-Risk Review

The first Milestone 3 slice should still avoid background concurrency. The
rules for this slice are:

- one remote execution call maps to one provider call
- no detached tasks
- no retry loops
- no parallel fanout across providers
- no overlapping remote dispatch for one call inside the runtime method

If the first provider path needs queues, cancellation channels, or fanout, stop
and re-plan before implementation.

### Risks

| Risk | Impact | Mitigation |
| ---- | ------ | ---------- |
| Provider abstractions become Pantograph-shaped too early | High | Keep the first provider request/result generic and narrow |
| Remote replay safety is weaker than local replay safety | High | Use deterministic record ids and read-before-write checks through Emily |
| The test provider becomes production architecture by accident | Medium | Keep it clearly documented as deterministic test infrastructure |
| Runtime ownership starts drifting toward background orchestration | Medium | Keep the first remote path synchronous and request-scoped |
| Provider metadata grows without contract discipline | Medium | Keep metadata append-only and document omitted-field semantics |

## Definition of Done

- `emily-membrane` has a documented provider boundary owned by the crate.
- A narrow remote execution path exists without binding the crate to Pantograph.
- All durable remote-state writes go through Emily's public API only.
- Acceptance tests prove route -> remote episode -> validation -> reconstruction
  through the membrane facade.
- Replay/idempotency behavior is covered for repeated deterministic remote
  execution.

## Ownership And Lifecycle Note

Milestone 3 still keeps lifecycle request-scoped:

- the host constructs the membrane runtime
- the host injects the provider implementation
- one runtime call issues at most one provider dispatch
- no provider sessions outlive the runtime call in this slice
- no background workers or cancellation registries are introduced yet

If implementation pressure requires:

1. provider session pooling
2. in-flight cancellation tokens
3. retry workers
4. parallel fanout across providers

then that work belongs in a later milestone and must be re-planned.

## Public Facade Preservation Note

This plan assumes:

- `emily` remains append-only
- provider ownership stays in `emily-membrane`
- the membrane runtime keeps using Emily's public facade rather than store
  internals
- Pantograph-specific dependencies remain out of the first Milestone 3 slice

## Milestones

### Milestone 3A: Provider Boundary

**Goal:** Define the provider trait and provider-owned dispatch contracts.

**Tasks:**
- [ ] Add a `providers/` module with README
- [ ] Add provider dispatch request/result DTOs
- [ ] Add a membrane-owned provider trait and error type
- [ ] Keep contracts transport-agnostic and append-only
- [ ] Add unit tests for provider DTO roundtrips if serde-backed

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- README review against `DOCUMENTATION-STANDARDS.md`

**Status:** Completed on 2026-03-08 in commit `6b069f7`

### Milestone 3B: Runtime Remote Path With Deterministic Test Provider

**Goal:** Add a first provider-backed remote path without Pantograph transport.

**Tasks:**
- [ ] Extend the runtime facade to accept an optional provider dependency
- [ ] Add one deterministic test provider implementation
- [ ] Record route, remote episode, validation, and remote state closure through
  Emily's public API
- [ ] Return a remote-aware reconstruction/result envelope
- [ ] Add replay/idempotency coverage for repeated remote execution

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- acceptance test from membrane request to Emily remote sovereign records

**Status:** Completed on 2026-03-08 in commit `acbf43f`

### Milestone 3C: Pantograph Adapter Planning Gate

**Goal:** Decide whether the generic provider boundary is stable enough for the
first real transport adapter.

**Tasks:**
- [ ] Review the provider trait against current Pantograph workflow contracts
- [ ] Identify any missing provider metadata or lifecycle hooks
- [ ] Record whether Pantograph can land as an additive next slice
- [ ] Stop and re-plan if the generic boundary is materially wrong

**Verification:**
- design review against the current Pantograph crates
- update design docs if boundary changes are required

**Status:** Completed on 2026-03-08 through Pantograph boundary review

## Execution Notes

Update during implementation:
- 2026-03-08: Milestone 3 implementation plan written.
- 2026-03-08: First remote adapter slice constrained to a provider-neutral
  boundary plus deterministic test provider before Pantograph transport.
- 2026-03-08: Milestone 3A provider boundary implemented in commit `6b069f7`.
- 2026-03-08: Provider-boundary verification passed with:
  - `cargo fmt --manifest-path emily-membrane/Cargo.toml`
  - `cargo clippy --manifest-path emily-membrane/Cargo.toml --all-targets --offline -- -D warnings`
  - `cargo test --manifest-path emily-membrane/Cargo.toml -q --offline`
- 2026-03-08: Milestone 3B deterministic remote path implemented in commit
  `acbf43f`.
- 2026-03-08: Remote-path verification passed with:
  - `cargo fmt --manifest-path emily-membrane/Cargo.toml`
  - `cargo clippy --manifest-path emily-membrane/Cargo.toml --all-targets --offline -- -D warnings`
  - `cargo test --manifest-path emily-membrane/Cargo.toml -q --offline`
- 2026-03-08: Milestone 3C Pantograph planning gate reviewed against:
  - `emily/src/inference/pantograph/provider.rs`
  - `src/pantograph_host.rs`
- 2026-03-08: Review result:
  - The current `MembraneProvider` trait is sufficient for a first additive
    one-shot Pantograph adapter.
  - The next adapter slice should add explicit request metadata conventions for
    `timeout_ms`, `priority`, and output-target selection instead of changing
    the provider trait immediately.
  - Streaming, cancellation, and session-reuse hooks are not blockers for the
    first adapter, but they are future trait-evolution candidates rather than
    metadata-only concerns.
- 2026-03-08: Post-gate additive Pantograph adapter implemented in commit
  `2135888`.
- 2026-03-08: Pantograph adapter scope:
  - Added an optional `pantograph` feature to `emily-membrane`.
  - Added `PantographWorkflowProvider`, `PantographProviderConfig`, and
    `PantographWorkflowBinding` under `src/providers/pantograph.rs`.
  - Kept the existing `MembraneProvider` trait unchanged.
  - Bound the first adapter to one-shot workflow execution only.
  - Supported additive metadata conventions for `workflow_id`, `timeout_ms`,
    `priority`, and `output_targets`.
  - Rejected nonzero `priority` for the one-shot path pending a future
    session-backed adapter.
- 2026-03-08: Pantograph adapter verification passed with:
  - `cargo fmt --manifest-path emily-membrane/Cargo.toml`
  - `cargo test --manifest-path emily-membrane/Cargo.toml -q --offline --features pantograph`
  - `cargo clippy --manifest-path emily-membrane/Cargo.toml --all-targets --offline --features pantograph -- -D warnings`
- 2026-03-08: Verification caveat:
  - Feature-gated Pantograph checks emit upstream warnings from the Pantograph
    workspace, but `emily-membrane` passes under `-D warnings`.
- 2026-03-08: Provider registry lookup implemented in commit `4a6be10`.
- 2026-03-08: Registry lookup scope:
  - Added a membrane-owned `MembraneProviderRegistry` abstraction.
  - Added `InMemoryProviderRegistry` as the default host-supplied registry.
  - Kept `with_provider(...)` as an additive single-provider wrapper.
  - Added `with_provider_registry(...)` for host-owned provider lookup.
  - Changed remote execution to resolve providers by `target.provider_id`
    instead of assuming one injected provider.
  - Added runtime coverage for missing-registry and missing-provider cases.
- 2026-03-08: Provider registry verification passed with:
  - `cargo fmt --manifest-path emily-membrane/Cargo.toml`
  - `cargo test --manifest-path emily-membrane/Cargo.toml -q --offline`
  - `cargo clippy --manifest-path emily-membrane/Cargo.toml --all-targets --offline -- -D warnings`
  - `cargo test --manifest-path emily-membrane/Cargo.toml -q --offline --features pantograph`
  - `cargo clippy --manifest-path emily-membrane/Cargo.toml --all-targets --offline --features pantograph -- -D warnings`
- 2026-03-08: Registry-backed target selection implemented in commit `421e9cc`.
- 2026-03-08: Target-selection scope:
  - Added `RemoteRoutingPreference` as a host-facing routing contract.
  - Extended `MembraneProviderRegistry` with deterministic registered-target
    metadata.
  - Added `RegisteredProviderTarget` and explicit-target registry builders.
  - Added `select_remote_target(...)` for registry-backed target resolution.
  - Added `execute_remote_with_registry_and_record(...)` as a host-facing
    wrapper over target selection plus remote execution.
  - Updated acceptance coverage so the remote path now proves registry-based
    target selection instead of a prebuilt `ProviderTarget`.
- 2026-03-08: Target-selection verification passed with:
  - `cargo fmt --manifest-path emily-membrane/Cargo.toml`
  - `cargo test --manifest-path emily-membrane/Cargo.toml -q --offline`
  - `cargo clippy --manifest-path emily-membrane/Cargo.toml --all-targets --offline -- -D warnings`
  - `cargo test --manifest-path emily-membrane/Cargo.toml -q --offline --features pantograph`
  - `cargo clippy --manifest-path emily-membrane/Cargo.toml --all-targets --offline --features pantograph -- -D warnings`

## Commit Cadence Notes

- Commit the provider boundary separately from runtime remote execution.
- Keep documentation-only commits separate from feature commits.
- Keep Pantograph planning or boundary-review updates separate from code.
- Follow `COMMIT-STANDARDS.md` and include `Agent: codex`.

## Optional Subagent Assignment

| Owner/Agent | Scope | Output Contract | Handoff Checkpoint |
| ----------- | ----- | --------------- | ------------------ |
| None | None assigned | Reason: provider boundary and runtime path are still tightly coupled in the first remote slice | Revisit if Pantograph transport work is split from generic provider work |

## Re-Plan Triggers

- The provider trait requires Pantograph-specific fields to function
- Remote replay safety cannot be achieved through Emily's current public API
- The first remote path needs background orchestration or cancellation registries
- Provider contracts begin to imply a full membrane IR before the compiler layer exists

## Recommendations

- Prefer a public provider trait with a small request/result surface instead of
  leaking workflow-service types into the membrane crate.
- Prefer one deterministic test provider before a real transport adapter.
- Prefer explicit helper builders for Emily remote records instead of ad hoc
  inline JSON metadata shapes.

## Completion Summary

### Completed

- Milestone 3 implementation plan drafted

### Deviations

- No implementation has started yet.

### Follow-Ups

- Decide whether provider dispatch requests need a dedicated leakage-budget
  placeholder field before Pantograph integration.
- Decide whether the first remote reconstruction envelope should carry provider
  provenance explicitly or derive it from Emily sovereign records.
- Add documented metadata conventions for Pantograph adapter fields:
  - `timeout_ms`
  - `priority`
  - output-target selection
- Revisit the provider trait only when streaming, cancellation, or session reuse
  become real requirements.

### Verification Summary

- Standards review against:
  - `PLAN-STANDARDS.md`
  - `TESTING-STANDARDS.md`
  - `DOCUMENTATION-STANDARDS.md`
  - `CONCURRENCY-STANDARDS.md`
  - `DEPENDENCY-STANDARDS.md`
  - `COMMIT-STANDARDS.md`
  - [GESTALT-STANDARDS.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/GESTALT-STANDARDS.md)
