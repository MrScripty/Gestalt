# Rust Memory System: High-Level Design Draft

## Goal

Define a production-viable Rust design for the Emily memory subsystem while making the subsystem boundary explicit. This draft does not, by itself, describe the full sovereign-dispatch architecture from the March 2026 paper.

## Scope (This Draft)

- Core memory architecture boundaries
- Recommended Rust libraries
- High-level input/output contracts
- Major design decisions for the memory and epistemic-integrity layers

## Non-Goals (For Now)

- Exact database schema
- Exact scoring formulas and thresholds
- Full `Semantic Membrane` implementation
- Full provider-routing / prompt-control layer
- Final model/provider selection
- Performance tuning details

## System Purpose

The memory subsystem should:

1. Ingest user and system events as memory traces.
2. Retrieve relevant context for current decisions.
3. Estimate or track uncertainty, confidence, outcomes, novelty, and stability.
4. Gate long-term integration through an `ECGL`-style policy.
5. Expose integrity and audit signals for observability and control.
6. Optionally provide hooks for `EARL` pre-cognitive gating and sovereign-dispatch extension layers.

## Recommended Rust Stack (Baseline)

### Core Runtime And API

- `tokio`
- `axum`
- `serde`, `serde_json`
- `uuid`
- `time` or `chrono`

### Storage And Retrieval

- `surrealdb`
- optional `petgraph` for advanced in-memory traversal or ranking logic

### Integration And Orchestration

- `reqwest` for external provider calls where needed
- `tokio::mpsc` for background jobs
- optional later: `async-nats` or `redis`

### Reliability And Operations

- `tracing`, `tracing-subscriber`
- `metrics`, `prometheus`
- `thiserror` + `anyhow`
- `config` or `figment`

### Optional Future Enhancements

- `qdrant-client` if vector search needs move out of SurrealDB
- `candle` / `ort` + `tokenizers` for local embeddings or local reasoning paths

## High-Level Components

### Memory / Integrity Core

1. `Ingest Service`
2. `Embedding Service`
3. `Memory Store`
4. `Retrieval Service`
5. `EMEB Coordination Layer`
6. `EARL Gate`
7. `ECGL Gate Engine`
8. `Consolidation Worker`
9. `Integrity Monitor`
10. `Audit Service`

### Sovereign Dispatch Extension

1. `Membrane Builder`
2. `Provider Router`
3. `Remote Quality Controller`
4. `Local Renderer / Reconstructor`
5. `Leakage Budget Auditor`

## Input Contracts (High-Level)

### User Input Event

- `event_id`, `user_id`, `session_id`
- `content`
- `timestamp`
- optional metadata: channel, locale, risk tier

### Outcome Event

- `decision_id` or `response_id`
- `outcome_label`
- `risk_weight`
- optional feedback metrics

### System Tick Event

- periodic trigger for consolidation, decay, and re-evaluation

### Policy Update Event

- threshold and weighting updates
- safety mode updates

### Remote Episode Event (Extension)

- `episode_id`
- `risk_state`
- `provider`
- `model`
- `membrane_budget`
- `validation_outcome`

## Output Contracts (High-Level)

### Retrieved Context

- ranked memory candidates
- relevance score
- epistemic annotations

### Decision Support Packet

- approved memory set for planning
- suppressed memory set with reasons

### Consolidation Decision

- `decision`: integrate / quarantine / defer
- gate score and feature contributions

### Integrity Snapshot

- global integrity score
- degradation alerts and trend metrics

### Audit Record

- immutable decision trace

### Sovereign Dispatch Audit (Extension)

- routing decision
- membrane budget report
- validation result
- integration authorization result

## Suggested Processing Flow

### Memory / Integrity Core

1. Receive user input.
2. Create trace candidate and embedding.
3. Use `EMEB`-style signals to estimate duplicate/coordination pressure where relevant.
4. Retrieve nearest memories and graph neighbors.
5. Score memory candidates.
6. Apply `EARL` if a reasoning episode needs gating.
7. Produce local decision support.
8. Ingest outcome feedback.
9. Apply `ECGL` and update integrity metrics asynchronously.

### Sovereign Dispatch Extension

1. Run `EARL` before remote reasoning.
2. If remote reasoning is justified, build membrane IR.
3. Route to one or more providers.
4. Validate returned structure locally.
5. Render final response locally.
6. Pass resulting artifacts through `ECGL` before identity-forming integration.

## Module Boundaries

For a single crate or workspace split, preserve boundaries like:

- `memory-api`
- `memory-core`
- `memory-store`
- `memory-retrieval`
- `memory-emeb`
- `memory-earl`
- `memory-ecgl`
- `memory-worker`
- `memory-observability`

Optional extension modules:

- `memory-membrane`
- `memory-routing`
- `memory-remote-quality`
- `memory-render`

## Key Decisions To Make Next

1. Is this doc staying scoped to the memory subsystem, or expanding toward full Emily architecture?
2. Should `EMEB` be implemented first as dedupe planning, confidence telemetry, or both?
3. How much of `EARL` should be local-only before remote dispatch exists?
4. Should `ECGL` run synchronously on write, or asynchronously with staged memory state?
5. When does the sovereign-dispatch layer become worth building relative to the memory MVP?

## Initial Recommendation

Start with:

- SurrealDB as single primary store
- external embeddings behind provider abstraction
- in-process async workers
- `EARL` and `ECGL` as local control layers
- clear extension points for later membrane and routing work

This keeps the current implementation path realistic while staying aligned with the broader Emily architecture described in the papers.
