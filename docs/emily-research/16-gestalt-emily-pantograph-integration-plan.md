# Plan: Gestalt Emily Pantograph Integration

## Objective

Define the next implementation phase for using Emily and `emily-membrane` in
Gestalt with Pantograph-backed inference, while preserving clean package
boundaries so Emily remains reusable in other hosts.

## Scope

### In Scope

- Planning Gestalt composition of:
  - `emily`
  - `emily-membrane`
  - Pantograph-backed embedding and remote membrane providers
- Planning use of:
  - `Qwen3.5-35B-A3B-GGUF` for membrane remote reasoning
  - `Qwen3-Embedding-4B-GGUF` for Emily embeddings
- Planning the next combined path:
  - real Gestalt integration
  - continued architecture hygiene
- Defining host/runtime ownership, provider wiring, verification, and
  documentation updates

### Out of Scope

- Immediate code changes outside the milestones below
- Final UX for all Emily-backed features
- Full autonomous policy design beyond current deterministic membrane/runtime
  behavior
- Moving Pantograph or Pumas concerns into `emily`
- Adding a reranker contract in this phase
- Broad production rollout across all Gestalt flows

## Inputs

### Problem

The repo now has:

- a reusable `emily` core for memory, vectors, episodes, EARL, ECGL,
  validation, and audits
- a reusable `emily-membrane` crate for routing, dispatch, validation,
  reconstruction, and Pantograph-backed remote execution
- early Gestalt adoption for seeded data, inspection, retrieval, episodes, and
  a dev-only membrane flow

The next step is to begin real host use with Pantograph-backed models without
collapsing crate boundaries or making Emily Gestalt-specific.

### Constraints

- `emily` remains host-agnostic and model-agnostic
- `emily-membrane` remains provider-agnostic at its public boundary
- Gestalt owns Pantograph host wiring, workflow selection, and runtime
  composition
- Persisted artifacts remain Emily-owned:
  - text objects
  - vectors
  - episodes
  - EARL evaluations
  - routing decisions
  - remote episodes
  - validation outcomes
  - sovereign audits
- No new background loops without explicit lifecycle ownership
- New dependencies must be justified and preferably avoided in reusable crates
- Cross-layer adoption requires at least one acceptance path per milestone

### Assumptions

- `Qwen3.5-35B-A3B-GGUF` is available through a Pantograph workflow suitable for
  one-shot membrane execution
- `Qwen3-Embedding-4B-GGUF` is available through a Pantograph workflow suitable
  for Emily's embedding provider
- workflow identifiers and node/port bindings are host configuration, not
  reusable crate contracts
- reranking is optional for this phase and should not block integration

### Dependencies

- [02-architecture-reconstruction.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/02-architecture-reconstruction.md)
- [10-emily-membrane-crate-design-plan.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/10-emily-membrane-crate-design-plan.md)
- [14-emily-membrane-execution-depth-plan.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/14-emily-membrane-execution-depth-plan.md)
- [15-gestalt-emily-adoption-and-test-data-plan.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/15-gestalt-emily-adoption-and-test-data-plan.md)
- [emily/src/inference/pantograph/provider.rs](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/emily/src/inference/pantograph/provider.rs)
- [emily-membrane/src/providers/pantograph.rs](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/emily-membrane/src/providers/pantograph.rs)
- [src/pantograph_host.rs](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/src/pantograph_host.rs)
- Shared standards in `/media/jeremy/OrangeCream/Linux Software/Coding-Standards/`
- Repo rules in [GESTALT-STANDARDS.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/GESTALT-STANDARDS.md)

### Affected Structured Contracts

- Gestalt host config for:
  - embedding workflow selection
  - membrane remote workflow selection
  - provider registry composition
  - timeout defaults
  - provider/profile metadata
- Existing public facades only:
  - `emily::EmilyApi`
  - `emily::EmbeddingProvider`
  - `emily_membrane::runtime::MembraneRuntime`
  - `emily_membrane::providers::{MembraneProvider, MembraneProviderRegistry}`

### Affected Persisted Artifacts

- Emily vector records seeded or produced via `Qwen3-Embedding-4B-GGUF`
- Emily episode / routing / validation / audit records produced by
  membrane-backed Gestalt flows
- No new non-Emily persistence should be introduced in this phase

### Concurrency And Race-Risk Review

This phase must explicitly control:

- Emily DB lifecycle per host/test run
- Pantograph embedding-session ownership
- membrane runtime ownership per host flow
- duplicate remote dispatch prevention by episode/task identity
- stale provider results after cancellation or host shutdown
- no overlap between fixture seeding and live DB use
- no hidden retries outside membrane request-scoped control
- no long-lived lock held across `.await`
- message passing preferred over shared mutable state between host coordination
  points

### Risks

| Risk | Impact | Mitigation |
| ---- | ------ | ---------- |
| Workflow/model ids leak into Emily core APIs | High | Keep all workflow selection in Gestalt host config/adapters |
| Gestalt couples directly to Pantograph internals instead of membrane/emily facades | High | Require host composition through existing public traits only |
| Retrieval quality is too weak for real adoption | High | Harden vector-backed retrieval before wide host dependence |
| Remote path reaches broader flows too early | High | Gate first remote use behind explicit dev-only or narrow opt-in path |
| Session-backed embedding and one-shot remote execution drift semantically | Medium | Keep each on its existing contract surface and document the split |
| Reranker pressure appears before retrieval is stable | Medium | Defer reranker integration until ranking quality is a proven bottleneck |
| Reusable crates gain unnecessary heavy dependencies | Medium | Keep Pantograph composition in Gestalt when possible and justify any new reusable-crate dependency in writing |

## Definition of Done

- A host-owned Pantograph configuration approach exists for both Qwen workflows
- Gestalt integration sequencing is defined from local-only adoption to gated
  remote adoption
- Boundary ownership between Gestalt, `emily`, and `emily-membrane` is
  explicit
- The plan preserves Emily and membrane as reusable modules for other hosts
- Each milestone has defined verification, including cross-layer acceptance
  where required
- Documentation and boundary-audit obligations are explicit, not implied

## Ownership And Lifecycle Note

Gestalt owns:

- building the Pantograph embedding provider
- building the Pantograph membrane provider or provider registry
- injecting those into `EmilyBridge` and `MembraneRuntime`
- starting and stopping host-owned runtimes
- feature flags for local-only vs remote-enabled behavior
- cancellation and shutdown sequencing for host-triggered flows

Emily owns:

- memory and vector persistence
- episode and policy persistence
- EARL / ECGL durable state

Membrane owns:

- bounded compile / route / dispatch / validation / reconstruction behavior
- request-scoped retry / multi-target logic
- no detached lifecycle outside host ownership

## Public Facade Preservation Note

Facade-first preservation is required:

- `emily` stays reusable and host-agnostic
- `emily-membrane` stays reusable and provider-agnostic at its public API
- Gestalt owns:
  - Pantograph host construction
  - workflow/model selection
  - provider registry assembly
  - environment/config lookup
- if a needed capability requires leaking Pantograph details into `emily`
  public contracts, stop and re-plan

## Recommendations

- Keep `Qwen3-Embedding-4B-GGUF` on Emily's existing embedding-provider path.
- Keep `Qwen3.5-35B-A3B-GGUF` on the membrane's existing remote-provider path.
- Defer reranker work until retrieval quality becomes a demonstrated bottleneck.

## Milestones

### Milestone 1: Host Configuration And Provider Mapping

**Goal:** Define host-owned Pantograph configuration for the two Qwen workflows
without leaking model/workflow knowledge into Emily core crates.

**Tasks:**
- [x] Define Gestalt host config shape for:
  - reasoning workflow using `Qwen3.5-35B-A3B-GGUF`
  - embedding workflow using `Qwen3-Embedding-4B-GGUF`
- [x] Record which existing adapter each workflow uses:
  - session-backed embedding provider for Emily
  - one-shot membrane provider for remote reasoning
- [x] Define required Pantograph bindings and timeout defaults
- [x] Ensure workflow IDs and node/port bindings remain Gestalt-owned
  configuration only
- [x] Define fallback behavior when one or both workflows are unavailable

**Verification:**
- `cargo fmt`
- targeted tests for config parsing and provider mapping
- one acceptance check that host config can construct the intended provider
  surfaces without modifying crate contracts
- documentation traceability review for any changed module/directory boundaries

**Status:** Complete

### Milestone 2: Retrieval Hardening For Real Emily Use

**Goal:** Make Emily retrieval good enough to justify broader Gestalt
dependence.

**Tasks:**
- [x] Prioritize vector-backed context retrieval using the embedding workflow
- [x] Keep lexical fallback explicit when vectors are absent
- [x] Define deterministic seeded datasets that prove useful context comes back
  from Emily
- [x] Confirm Gestalt continues to consume retrieval only through public APIs
- [x] Explicitly defer reranker integration unless ranking quality becomes the
  demonstrated bottleneck

**Verification:**
- `cargo fmt`
- targeted Emily retrieval tests
- cross-layer acceptance check from seeded data -> embedding/vector persistence
  -> host context consumption
- replay/idempotency check for seeded or repeated ingestion

**Status:** Complete

### Pre-Milestone 3 Validation Gate: Real Pantograph Embedding Roundtrip

**Goal:** Prove the actual Pantograph embedding path before moving any real
Gestalt workflow onto broader Emily dependence.

**Tasks:**
- [x] Update the saved `Embedding` workflow graph so the `puma-lib` node uses
  `Qwen3-Embedding-4B-GGUF`
- [x] Open a real session-backed Emily embedding provider through Gestalt host
  composition
- [x] Pass real text through the embedding workflow and receive vectors back
- [x] Record the measured embedding dimensionality for the selected model
- [x] Keep Pantograph-specific graph maintenance in Gestalt host code rather
  than leaking it into `emily`

**Verification:**
- `cargo fmt`
- `cargo test -q --test pantograph_host_reasoning`
- `cargo check -q --bin emily_pantograph_embedding_probe`
- `cargo run --quiet --bin emily_pantograph_embedding_probe`

**Measured Result:**
- `Qwen3-Embedding-4B-GGUF` returned `2560` dimensions
- Gestalt updated the saved workflow at
  `/media/jeremy/OrangeCream/Linux Software/Pantograph/.pantograph/workflows/Embedding.json`
- Session-based validation returned a live Pantograph session id and embedding
  vector preview
- Warm-session reuse was validated on one provider instance:
  - validate session id matched all subsequent embedding calls
  - first run: `2525 ms`
  - second run: `1649 ms`
  - third run: `26 ms`

**Status:** Complete

### Milestone 3: Real Gestalt Local-Only Membrane Adoption

**Goal:** Move one real Gestalt workflow onto the membrane path without remote
dispatch yet.

**Tasks:**
- [x] Pick one production-adjacent host flow
  - recommended: local-agent execution path
- [x] Route it through:
  - Emily context retrieval
  - episode creation/linking
  - membrane compile/validate/reconstruct
- [x] Surface reconstruction provenance and review status in host-visible
  diagnostics
- [x] Keep a feature flag and explicit fallback path
- [x] Keep all Pantograph model/workflow composition outside reusable crates

**Verification:**
- `cargo fmt`
- host acceptance test using seeded Emily DBs
- manual dev loop proving provenance-aware reconstruction is visible
- recovery check proving fallback path remains intact when membrane is disabled

**Execution Notes:**
- Adopted the real local-agent send flow first, but kept the membrane slice
  local-only and post-dispatch so it records bounded compile/validate/
  reconstruct artifacts without changing the terminal command payload.
- Added a host-only `local_agent_membrane` helper so Dioxus components do not
  own membrane runtime construction or persistence identifiers.
- Reused the existing `EmilyBridge` worker by extending it with the membrane's
  local-only sovereign record reads/writes instead of opening a second Emily
  runtime against the same database.
- Gated the path behind `GESTALT_ENABLE_LOCAL_AGENT_MEMBRANE=1` so the existing
  fallback remains the default host behavior.

**Status:** Complete

### Milestone 4: Gated Single-Remote Membrane Adoption

**Goal:** Add the first real Pantograph-backed remote reasoning path in Gestalt
behind an explicit development gate.

**Tasks:**
- [ ] Compose a Gestalt-owned membrane provider registry with the
  `Qwen3.5-35B-A3B-GGUF` workflow
- [ ] Use existing membrane routing and EARL-aware policy selection
- [ ] Restrict first remote adoption to one narrow host flow
- [ ] Record route, remote episode, validation, and audit artifacts through
  Emily only
- [ ] Keep local-only fallback explicit
- [ ] Define timeout, failure, and review-required UX/diagnostic handling
  without changing reusable crate boundaries

**Verification:**
- `cargo fmt`
- host acceptance with remote-enabled seeded flow
- developer-run diagnostic proving route/remote/validation/audit records appear
  in Emily
- failure-path acceptance for unavailable provider, timeout, and
  review-required validation
- duplicate-request/idempotency check for repeated host-triggered dispatch
  attempts

**Status:** Not started

### Milestone 5: Integration Hygiene And Boundary Audit

**Goal:** Confirm the combined system still preserves clean reusable
boundaries.

**Tasks:**
- [ ] Audit that:
  - `emily` contains no Gestalt-specific or Pantograph-host-specific logic
  - `emily-membrane` contains no Gestalt UI/app logic
  - Gestalt owns provider/workflow composition only
- [ ] Review whether any new host helpers should live in a small Gestalt
  adapter module
- [ ] Update affected `README.md` files for changed `src/` directories
- [ ] Record any follow-up boundary corrections before widening adoption

**Verification:**
- code review against package boundaries
- documentation review for every touched `src/` directory
- dependency review confirming no unjustified additions entered reusable crates

**Status:** Not started

## Execution Notes

Update during implementation:
- 2026-03-08: Plan created after reviewing the current Pantograph embedding
  bootstrap path, membrane Pantograph provider path, and existing Emily
  adoption work.
- 2026-03-08: Milestone 1 completed. Gestalt now owns a separate reasoning
  workflow config path, builds a Pantograph-backed membrane provider registry
  from host env/config, and verifies the mapping through focused integration
  coverage.
- 2026-03-08: Milestone 2 completed. Gestalt now auto-applies host-owned
  embedding vectorization defaults at Emily startup, seeds a semantic-context
  corpus for retrieval checks, and verifies both semantic retrieval and lexical
  fallback through bridge-level public APIs.
- 2026-03-08: Milestone 3 completed. The real local-agent host flow now has an
  optional local-only membrane pass behind `GESTALT_ENABLE_LOCAL_AGENT_MEMBRANE=1`,
  records routing/validation/audit artifacts through the existing Emily bridge,
  and verifies the path with bridge-backed acceptance coverage.

## Commit Cadence Notes

- Commit when a logical slice is complete and verified.
- Follow commit format/history cleanup rules from `COMMIT-STANDARDS.md`.

## Optional Subagent Assignment

| Owner/Agent | Scope | Output Contract | Handoff Checkpoint |
| ----------- | ----- | --------------- | ------------------ |
| None | None assigned | Provider composition and host integration are still tightly coupled in the first slice | Revisit if workflow config and host runtime composition split cleanly |

## Re-Plan Triggers

- Pantograph workflow bindings require new provider-contract fields in `emily`
  or `emily-membrane`
- The chosen Qwen workflows cannot be represented cleanly by current embedding
  or membrane adapters
- Retrieval quality remains insufficient after vector-backed work
- The first remote Gestalt slice requires session-backed membrane execution
  rather than one-shot dispatch
- Host integration pressures force Pantograph-specific logic into reusable
  crates
- A new dependency is proposed for a reusable crate without clear justification

## Completion Summary

### Completed

- Milestone 1: Host configuration and provider mapping
- Milestone 2: Retrieval hardening for real Emily use
- Milestone 3: Real Gestalt local-only membrane adoption

### Deviations

- `cargo clippy --test pantograph_host_reasoning -- -D warnings` remains blocked
  by pre-existing unrelated warnings/errors in app modules such as
  `src/persistence/paths.rs`, `src/emily_bridge.rs`, `src/orchestrator/`,
  `src/run_checkpoints/mod.rs`, and `src/ui/`.

### Follow-Ups

- Start Milestone 4: gated single-remote membrane adoption.

### Verification Summary

- `cargo fmt`
- `cargo test -q --test pantograph_host_reasoning`
- `cargo test -q --test pantograph_host_reasoning --test emily_seed_corpus --test emily_semantic_retrieval`
- `cargo test -q --test emily_local_agent_context --test emily_local_agent_episode --test emily_local_agent_membrane`
- `cargo check -q`
- `cargo clippy --test pantograph_host_reasoning -- -D warnings`
  - blocked by pre-existing unrelated warnings/errors outside this slice
- `cargo clippy --test emily_semantic_retrieval --test emily_seed_corpus --test pantograph_host_reasoning -- -D warnings`
  - blocked by pre-existing unrelated warnings/errors outside this slice

### Traceability Links

- Module README updated:
  - [src/README.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/src/README.md)
  - [src/emily_seed/README.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/src/emily_seed/README.md)
  - [tests/README.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/tests/README.md)
- ADR added/updated: N/A
- PR notes completed per `templates/PULL_REQUEST_TEMPLATE.md`: not applicable in
  local branch workflow
