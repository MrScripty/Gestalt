# Rust Memory System: High-Level Design Draft

## Goal

Define a production-viable memory system for an Emily-style agent in Rust, without committing to low-level implementation details yet.

## Scope (This Draft)

- Core architecture boundaries
- Recommended Rust libraries
- High-level input/output contracts
- Major design decisions to finalize early

## Non-Goals (For Now)

- Exact database schema
- Exact scoring formulas and thresholds
- Final model/provider selection
- Performance tuning details

## System Purpose

The memory system should:

1. Ingest user and system events as memory traces.
2. Retrieve relevant context for current decisions.
3. Score memory quality (uncertainty, confidence, outcomes, stability).
4. Gate long-term integration (integrate, quarantine, or defer).
5. Expose integrity and audit signals for observability and control.

## Recommended Rust Stack (Baseline)

### Core Runtime and API

- `tokio`: async runtime
- `axum`: HTTP API layer
- `serde`, `serde_json`: typed message payloads
- `uuid`: IDs for traces/sessions/decisions
- `time` (or `chrono`): timestamps

### Storage and Retrieval

- `surrealdb`: async client for SurrealDB
- SurrealDB as the primary multi-model store (document + graph relations + vector-oriented retrieval)
- `petgraph` (optional): in-memory graph traversal/scoring utilities for advanced ranking logic

### Integration and Orchestration

- `reqwest`: embedding/model provider calls
- `tokio::mpsc` (MVP): background job channels
- Optional later: `async-nats` or `redis` for distributed queueing
- Optional later: SurrealDB live query subscriptions for reactive update flows

### Reliability and Operations

- `tracing`, `tracing-subscriber`: structured logs
- `metrics`, `prometheus`: runtime metrics
- `thiserror` + `anyhow`: error handling
- `config` (or `figment`): environment-driven configuration

### Optional Future Enhancements

- `qdrant-client` if you later prefer dedicated external vector search
- `candle` / `ort` + `tokenizers` for local inference and embeddings

## High-Level Components

1. `Ingest Service`
   - Accepts events, normalizes payloads, creates trace candidates.
2. `Embedding Service`
   - Converts trace content to vectors (local or external provider).
3. `Memory Store`
   - Persists traces, metadata, vectors, and integration states.
4. `Retrieval Service`
   - Returns ranked memory candidates (vector + graph context).
5. `Scoring Engine`
   - Computes uncertainty/confidence/outcome/stability signals.
6. `Gate Engine`
   - Produces integration decisions: integrate, quarantine, defer.
7. `Consolidation Worker`
   - Runs async consolidation and re-evaluation cycles.
8. `Integrity Monitor`
   - Produces global health/integrity metrics and alerts.
9. `Audit Service`
   - Stores decision rationale and trace-level explainability records.

## Input Contracts (High-Level)

### 1) User Input Event

- `event_id`, `user_id`, `session_id`
- `content` (text now; extensible for multimodal)
- `timestamp`
- optional metadata: channel, locale, risk tier

### 2) Outcome Event

- `decision_id` or `response_id` reference
- `outcome_label` (success/failure/partial)
- `risk_weight` (impact magnitude)
- optional feedback metrics

### 3) System Tick Event

- periodic trigger for consolidation, decay, and re-evaluation

### 4) Policy Update Event

- threshold and weighting updates
- safety mode updates (strict vs balanced vs exploratory)

## Output Contracts (High-Level)

### 1) Retrieved Context

- ranked memory candidates
- relevance score + epistemic annotations

### 2) Decision Support Packet

- approved memory set for response planning
- suppressed memory set with reasons

### 3) Consolidation Decision

- `decision`: integrate/quarantine/defer
- gate score and feature contributions

### 4) Integrity Snapshot

- global integrity score
- degradation alerts and trend metrics

### 5) Audit Record

- immutable decision trace for observability and debugging

## Suggested Processing Flow

1. Receive user input.
2. Build trace candidate and embedding.
3. Retrieve nearest memories and graph neighbors.
4. Score memory candidates (uncertainty/confidence/outcome/stability).
5. Produce response context and decision support.
6. Emit response.
7. Ingest outcome feedback.
8. Async consolidation applies gate decisions and updates integrity metrics.

## Module Boundaries (Rust Crate Layout Suggestion)

- `memory-api`: axum routes + request/response DTOs
- `memory-core`: domain types + traits
- `memory-store`: SurrealDB repositories and query adapters
- `memory-retrieval`: vector + graph ranking
- `memory-scoring`: scoring orchestration
- `memory-gate`: integration policy engine
- `memory-worker`: async consolidation pipelines
- `memory-observability`: metrics + logging setup

If a single crate is preferred initially, mirror these as modules to preserve clean boundaries.

## Key Decisions To Make Next

1. **Storage strategy**
   - SurrealDB-only vs SurrealDB + dedicated vector DB hybrid.
2. **Embedding strategy**
   - external provider first, local embeddings later, or local-first.
3. **Worker strategy**
   - in-process async workers vs separate worker service.
4. **Consistency model**
   - synchronous gating on write vs async gating with staging.
5. **Policy strictness defaults**
   - conservative default thresholds vs adaptive early exploration.

## Initial Recommendation

Start with:

- SurrealDB as single primary store
- external embeddings via provider abstraction
- in-process async workers
- async consolidation with staged memory state

This gives fastest path to a working system with low operational overhead while preserving upgrade paths.
