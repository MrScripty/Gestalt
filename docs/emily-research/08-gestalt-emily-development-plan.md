# Gestalt Emily Development Plan

## Purpose

Define a staged implementation plan for evolving Gestalt's current Emily integration from a durable terminal-memory backend into a system that is consistent with the intended Emily design documented in the local paper set.

This document is for planning and review. It is not an implementation spec for a single sprint.

## Planning Scope

This plan covers three related layers:

1. Gestalt's current Emily integration.
2. The Emily memory and retrieval core.
3. The later policy and sovereign-dispatch layers that should be built above that core.

This plan does not assume the full March 2026 Emily architecture should be implemented in one pass.

## Current Implementation Snapshot

Today, Emily in Gestalt is primarily:

- the source of truth for terminal history
- a persistent store for snippets
- an embedding and vectorization subsystem
- a paging backend for restoring terminal history
- a provisional context-query surface that is exposed but not yet used by Gestalt's agent flow

Today, Emily is not yet:

- a real semantic retrieval engine
- an active `EARL` runtime
- an active `ECGL` runtime
- an episode or outcome model
- a sovereign-dispatch or `Semantic Membrane` runtime
- the policy layer that governs Gestalt's local-agent execution

## Planning Assumptions

Unless review changes this direction, the plan assumes:

1. The `emily` crate remains the host-agnostic memory, retrieval, and policy core.
2. Gestalt owns application-specific orchestration and user-facing integration.
3. The sovereign-dispatch layer should be built above the stabilized memory core, not fused into the current terminal-history bridge.
4. Terminal input and output paths must remain non-blocking.
5. Placeholder scores and policy fields should not be treated as real runtime policy until their data model and evaluation path exist.

## Strategic Goals

1. Make the current Emily implementation truthful and internally consistent.
2. Turn Emily from passive storage into an active context subsystem.
3. Add real episode and outcome semantics before implementing policy frameworks that depend on them.
4. Build `EARL` and `ECGL` as actual runtime behavior rather than decorative metadata.
5. Prepare a clean architectural boundary for later `Semantic Membrane`, `ECCR`, and `AOPO/APC` work.

## Non-Goals For The First Implementation Cycle

The first implementation cycle should not attempt to fully deliver:

- multi-provider remote dispatch
- membrane IR construction
- local legend mapping and secure reconstruction
- full `ECCR` behavior
- full `AOPO/APC` behavior
- a complete sovereign Emily runtime

Those remain later phases after the memory and policy core are stable.

## Workstreams

This plan is easier to execute if treated as four workstreams:

1. Memory-core correctness
2. Retrieval and consumption
3. Episode and policy runtime
4. Sovereign-dispatch preparation

The phases below sequence those workstreams into deliverable slices.

## Phase 0: Baseline Alignment

### Objective

Make the current implementation honest, measurable, and easier to extend without changing the user-facing product model yet.

### Work Packages

1. Audit the current code path from terminal events to Emily storage and document the actual runtime behavior.
2. Remove or downgrade misleading defaults where newly ingested objects are implicitly treated as fully trusted and integrated.
3. Decide which scoring fields stay as placeholders and which become active invariants in the near term.
4. Make it explicit that `query_context` is provisional until real semantic retrieval exists.
5. Fix or remove half-implemented restore cursor state such as `history_before_sequence`.
6. Make health reporting reflect real runtime behavior only.
7. Add metrics for:
   - ingested terminal lines
   - ingested snippet objects
   - vectorized objects
   - history page queries
   - context queries
   - dropped enqueue events

### Deliverables

- cleaned runtime invariants
- updated module docs where behavior was ambiguous
- truthful telemetry surface
- tests around restore and history paging behavior

### Exit Criteria

- runtime surfaces no longer imply that `EARL` or `ECGL` are active when they are not
- restore and paging state are internally consistent
- telemetry and health data correspond to real counters

## Phase 1: Retrieval Core

### Objective

Replace provisional lexical lookup with a real Emily-style retrieval foundation that Gestalt can safely consume.

### Work Packages

1. Implement vector-based retrieval using stored embeddings.
2. Add semantic edge creation so `SemanticSimilar` becomes real persisted structure.
3. Make graph expansion meaningful:
   - linear neighborhood
   - semantic neighborhood
   - configurable depth and scope
4. Introduce deterministic ranking using some combination of:
   - semantic similarity
   - recency
   - stream or scope relevance
   - future policy weights when available
5. Define fallback behavior when embeddings are unavailable:
   - lexical fallback
   - mixed ranking
   - stream-scoped history fallback
6. Ensure retrieval APIs return provenance that Gestalt can surface or audit later.

### Deliverables

- vector-aware `query_context`
- semantic edge persistence
- ranking contract with deterministic tie-breaking
- retrieval-focused tests

### Exit Criteria

- `query_context` is useful enough for one real Gestalt workflow
- semantic retrieval and fallback behavior are test-covered
- retrieval policy fields affect real ranking or are removed from the path

## Phase 2: Gestalt Context Consumption

### Objective

Use Emily context in a real Gestalt flow so Emily stops being only a history backend.

### Recommended First Target

The first consumer should be the local-agent path, because it is already the most obvious place where historical and snippet context can change behavior.

### Work Packages

1. Define a minimal context packet contract for Gestalt.
2. Decide what evidence goes into the first packet:
   - recent terminal evidence
   - relevant historical evidence
   - snippet evidence
   - provenance identifiers
3. Add one orchestrator path that queries Emily before selected local-agent actions.
4. Add diagnostics so the UI or logs can show when Emily context was used.
5. Ensure failure behavior is safe:
   - agent flow continues if retrieval fails
   - terminal hot paths are unaffected

### Deliverables

- one real context consumer in Gestalt
- observable retrieval usage
- graceful degradation tests

### Exit Criteria

- at least one production code path consumes Emily context
- Emily is no longer only a passive history store

## Phase 3: Episode And Outcome Model

### Objective

Create the data model required for real `EARL` and `ECGL` behavior.

### Work Packages

1. Introduce an explicit episode abstraction above raw text objects.
2. Define episode lifecycle data:
   - task or intent
   - input traces
   - output traces
   - produced response or action
   - outcome status
   - validation state
   - risk metadata
3. Add outcome ingestion APIs to Emily.
4. Decide how existing Gestalt events map into episodes:
   - command round
   - local-agent action
   - snippet promotion
   - future remote-dispatch episode
5. Store:
   - episode records
   - outcome records
   - episode-to-trace links
   - decision and rationale audit data

### Deliverables

- episode data model
- outcome ingestion path
- storage and API updates
- tests for episode linkage and persistence

### Exit Criteria

- Emily can reason about more than isolated text lines
- outcome-linked learning has a real data path

## Phase 4: EARL Runtime

### Objective

Implement the first real `EARL` runtime slice as a pre-cognitive risk gate over selected episodes.

### Work Packages

1. Define the minimum viable `EARL` input signals available in Gestalt today.
2. Decide which signals are ready now and which must wait.
3. Add `EARL` evaluation to a selected episode flow.
4. Implement the first practical `OK`, `CAUTION`, and `REFLEX` behaviors.
5. Define how clarification, tentative output, or abort behavior surfaces in Gestalt UX and logs.
6. Prevent blocked or reflexive episodes from contaminating long-term integrated memory.

### Deliverables

- first `EARL` evaluator
- policy-driven episode state transitions
- diagnostics for gated episodes
- tests for normal, cautious, and blocked paths

### Exit Criteria

- `EARL` exists as real runtime behavior in at least one path
- guarded episodes are distinguishable in storage and diagnostics

## Phase 5: ECGL Runtime

### Objective

Turn existing score fields and integration flags into a real memory-integration policy layer.

### Work Packages

1. Define explicit memory states:
   - pending
   - integrated
   - quarantined
   - deferred
2. Stop defaulting new objects to `integrated = true`.
3. Implement computation or approximation of:
   - confidence factor
   - outcome factor
   - novelty factor
   - stability factor
4. Implement learning weight, gate score, quarantine behavior, and adaptive threshold behavior.
5. Add `CI` computation and reporting.
6. Add re-evaluation or release jobs for quarantined items.

### Deliverables

- active `ECGL` state machine
- quarantine and reintegration behavior
- integrity reporting
- tests for integration policy decisions

### Exit Criteria

- `ECGL` is active policy, not placeholder metadata
- Emily can distinguish integrated memory from merely stored memory

## Phase 6: Snippet And Knowledge Promotion

### Objective

Move snippets from simple embedded notes into a more meaningful knowledge substrate.

### Work Packages

1. Define snippet-specific linkage and retrieval behavior.
2. Link snippets more explicitly to sessions, episodes, and source traces.
3. Decide which snippet types can be promoted into higher-value memory.
4. Expose snippet provenance in context packets.

### Deliverables

- snippet retrieval rules
- better provenance and linkage
- tests for snippet promotion and retrieval

### Exit Criteria

- snippets participate meaningfully in retrieval and policy

## Phase 7: Sovereign-Dispatch Preparation

### Objective

Prepare the memory and policy core for later sovereign Emily work without implementing the whole remote architecture yet.

### Work Packages

1. Define the boundary between:
   - memory core
   - episode and policy core
   - sovereign-dispatch layer
2. Introduce audit-friendly types for future remote episodes:
   - routing decision
   - provider
   - model
   - validation result
   - budget metadata
3. Reserve extension points for:
   - `Semantic Membrane`
   - `ECCR`
   - `AOPO/APC`
   - local rendering and legend mapping
4. Keep these concerns out of the terminal-history bridge until a separate remote-episode flow exists.

### Deliverables

- agreed layer boundaries
- extension-friendly API shapes
- future audit record schema

### Exit Criteria

- current implementation can host a higher sovereign layer without major redesign

## Phase 8: Sovereign-Dispatch Slice

### Objective

Implement the first narrow slice of the March 2026 Emily sovereign layer on top of the stabilized memory and policy core.

### Work Packages

1. Build membrane IR construction for bounded external tasks.
2. Add provider routing and model selection.
3. Add local validation and rendering of returned results.
4. Introduce the first `ECCR` and `AOPO/APC` behaviors that are supported by implementation decisions.
5. Add remote-episode auditing and budget reporting.
6. Route remote results through `EARL` and `ECGL` instead of bypassing them.

### Deliverables

- first remote-episode path
- audit and validation trail
- bounded sovereign-dispatch behavior

### Exit Criteria

- Gestalt has a real sovereign Emily slice, not only memory persistence

## Recommended Delivery Order

Recommended implementation sequence:

1. Phase 0
2. Phase 1
3. Phase 2
4. Phase 3
5. Phase 4
6. Phase 5
7. Phase 7
8. Phase 8

Phase 6 can move earlier if snippet workflows become a product priority.

## Milestone View

If we want larger review checkpoints instead of phase-by-phase shipping, the milestones should be:

1. Honest memory core
2. Real retrieval core
3. First Gestalt consumer
4. Episode-aware Emily
5. Active `EARL`
6. Active `ECGL`
7. Sovereign-ready architecture
8. First sovereign-dispatch slice

## Dependencies And Gating

The main hard dependencies are:

1. Phase 0 before any policy claims.
2. Phase 1 before Phase 2 if the first consumer is meant to rely on real semantic retrieval.
3. Phase 3 before meaningful `EARL` and `ECGL`.
4. Phase 5 before remote results should be allowed to affect long-term integrated memory.
5. Phase 7 before any serious sovereign-dispatch implementation.

## Immediate First Slice

If we want the smallest high-value slice to implement first, it should be:

1. Phase 0 baseline alignment
2. Phase 1 retrieval core
3. Phase 2 context consumption in one real Gestalt flow

That gets Gestalt from "history store with embeddings" to "active context subsystem" before the larger policy architecture begins.

## Open Decisions For Review

These decisions should be resolved before implementation starts in earnest:

1. Should the sovereign-dispatch layer live in Gestalt above the `emily` crate, or should the `emily` crate absorb part of it?
2. What is the first real workflow that should consume Emily context?
3. What is the first canonical episode type in Gestalt?
4. Which `EARL` signals can be computed from current local data?
5. Should `ECGL` make decisions per text object, per episode, or both?
6. Do we want Phase 3 immediately after Phase 2, or should we deepen retrieval first?

## Risks

The main implementation risks are:

1. Overbuilding sovereign-dispatch architecture before the memory core is reliable.
2. Building product behavior on placeholder scores or policy fields.
3. Coupling the current bridge too tightly to future remote-dispatch concerns.
4. Adding blocking work or fragile dependencies into terminal hot paths.
5. Implementing policy without enough observability to debug it.

## Validation Strategy

Every phase should include:

1. API boundary tests
2. persistence and restore tests
3. retrieval correctness tests where applicable
4. graceful-degradation tests
5. hot-path performance checks for terminal input and output

## Definition Of Ready To Implement

We should consider the plan ready for implementation when:

1. the architectural boundary between memory core and sovereign layer is accepted
2. the first consuming workflow is chosen
3. the Phase 0 exit criteria are accepted
4. the Phase 1 retrieval contract is accepted
5. we agree whether Phase 3 begins immediately after Phase 2

## Review Guidance

When reviewing this plan, focus first on:

1. whether the layer boundaries are correct
2. whether the phase order is realistic
3. whether the first slice is small enough
4. whether the open decisions are the right ones to resolve before coding
