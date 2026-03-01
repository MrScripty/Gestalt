# Memory Module Plan

## Purpose

Build a reusable, Emily-style memory subsystem for Gestalt that:

1. Stores terminal text history permanently in a database.
2. Adds an embedding vector to each text object.
3. Maintains both linear edges (chronological chain) and semantic edges (vector similarity links).
4. Returns context packets for local agent orchestration.

The design must be modular enough to extract later without breaking Gestalt, and safe enough that changing memory internals does not break terminal behavior.

## Scope

In scope:

- `src/memory/` module architecture and contracts
- event ingestion from terminal input/output
- persistent text object and edge storage
- embedding generation pipeline
- retrieval + re-ranking for agent context
- phased path to Emily-style confidence/gating metrics

Out of scope for first implementation:

- out-of-process network service
- external API transport (HTTP/IPC server)
- full autonomous policy engine

## Standards Alignment

This plan follows:

- `GESTALT-STANDARDS.md` architecture layers (`ui -> orchestrator -> terminal/state`)
- typed errors for stabilized modules
- no blocking work in PTY hot paths
- source/module documentation requirements for `src/`
- file-size target guidance (keep files focused and split by responsibility)

## High-Level Design

Memory remains a separate service layer inside Gestalt:

1. `terminal` emits memory events only.
2. `memory` runtime consumes events asynchronously.
3. `memory` persists text objects and edges.
4. `orchestrator` queries `memory` for context packets.
5. `ui` stays unaware of memory internals.

This preserves current ownership boundaries and keeps terminal emulation behavior unchanged.

## Planned Module Layout

`src/memory/mod.rs`
- public facade exports
- module wiring

`src/memory/core.rs`
- domain types only (no DB/provider coupling)
- `TextObject`, `Edge`, query/result DTOs
- traits: `MemoryStore`, `Embedder`, `MemoryEngine`

`src/memory/error.rs`
- typed error enums (`MemoryError`, `StoreError`, `EmbedError`)

`src/memory/store.rs`
- concrete persistent storage implementation
- SurrealDB client integration and schema/migration hooks

`src/memory/embed.rs`
- embedding provider abstraction
- initial provider: local Ollama embeddings

`src/memory/engine.rs`
- ingest orchestration
- linear edge creation
- semantic edge creation
- retrieval + ranking

`src/memory/runtime.rs`
- background worker loop + queues
- lifecycle control (start/stop/drain)
- non-blocking enqueue API for terminal/orchestrator callers

## Core Domain Model

### TextObject

Required fields:

- `id: Uuid`
- `session_id: SessionId`
- `group_id: GroupId`
- `seq_in_session: u64`
- `ts_utc: i64` (unix ms or ns; pick one constant globally)
- `kind: TextKind` (`InputLine`, `OutputLine`, `RoundSummary`, `AgentNote`)
- `text: String`
- `embedding: Vec<f32>`
- `cwd: Option<String>`
- `metadata: serde_json::Value`

### Edge

Required fields:

- `id: Uuid`
- `from_id: Uuid`
- `to_id: Uuid`
- `edge_type: EdgeType` (`LinearNext`, `SemanticSimilar`, `CommandToOutput`, `SameRound`, `SameCwd`)
- `weight: f32`
- `ts_utc: i64`

## Storage Strategy

Primary choice: SurrealDB single-store approach.

Tables:

- `text_objects`
- `text_edges`
- `memory_integrity` (future scoring/integrity metrics)
- `memory_policy` (future weight/threshold versions)

Key indexes:

- `text_objects(session_id, seq_in_session)` for linear chain queries
- timestamp index for recency filtering
- vector index for ANN semantic retrieval
- edge index on `from_id`, `to_id`, `edge_type`

## Ingestion Pipeline

### Input events

Capture before PTY write path:

- source today: `src/terminal.rs` in `send_input`/`send_line`
- event contains: session/group/cwd, text, timestamp, monotonic session seq

### Output events

Capture at finalized line boundaries:

- source today: `src/terminal.rs` line finalization (`finish_line`)
- event contains: session/group/cwd, output text, timestamp, monotonic session seq

### Non-blocking requirement

- terminal path only enqueues events
- DB writes and embeddings run in background worker
- bounded queue with drop accounting + metrics (no hidden data loss)

## Edge Construction Rules

### Linear edges

For each new object in a session:

- create `LinearNext(previous -> current)` if previous exists
- this forms exact chronological chain

### Semantic edges

After embedding is available:

1. ANN search top `K` candidates
2. filter by similarity threshold
3. create `SemanticSimilar(current -> candidate)` edges with similarity weight

Suggested initial constants:

- `SEMANTIC_TOP_K = 12`
- `SEMANTIC_MIN_SIM = 0.78`

Tune later via policy records.

## Retrieval Contract for Agents

`query_context(request)` should:

1. embed request/query text
2. fetch vector nearest neighbors
3. expand graph neighborhood (linear +/- `N`, semantic neighbors)
4. re-rank with deterministic formula
5. return compact context packet with provenance

Initial rank formula:

`rank = sim * (0.4 + 0.6 * confidence) * (0.5 + 0.5 * learning_weight) * recency_decay`

For MVP, `confidence` and `learning_weight` can default to `1.0` until scoring is introduced.

## Embedding Strategy

Initial provider:

- Ollama embeddings with `nomic-embed-text`

Provider abstraction requirements:

- configurable model name
- explicit embedding dimension validation
- timeout and retry policy with typed errors

Future providers:

- local ONNX/Candle provider
- remote embedding endpoint

## Emily-Style Scoring Roadmap

Phase-in scoring after retrieval is stable:

1. EMEB proxy (`epsilon`, uncertainty estimate)
2. EARL proxy (`outcome_factor`, risk-weighted feedback)
3. ECGL gate (`learning_weight`, integrate vs quarantine)
4. CI metric and adaptive threshold policy

Do not block MVP on full scoring stack; stage it behind optional policy mode.

## Gestalt Integration Plan

### Orchestrator-facing API

Expose memory access through orchestrator-facing service functions:

- `enqueue_input_event(...)`
- `enqueue_output_event(...)`
- `query_context(...)`

`ui` should not call memory internals directly.

### Runtime ownership

App owns one `MemoryRuntime` instance.

- clear startup initialization
- clear shutdown/drain behavior
- testable `NoopMemoryRuntime` fallback for disabled mode

### Failure behavior

- terminal/orchestrator continue operating if memory backend is unavailable
- surface error counters/health in logs and optional diagnostics

## Testing Plan

Unit tests:

- linear edge creation correctness
- semantic edge threshold filtering
- ranking determinism and tie handling
- typed error mapping

Integration tests:

- event enqueue -> persist -> query roundtrip
- queue backpressure behavior
- no deadlock/blocking in terminal input path

Manual verification:

- long-running terminal sessions still render correctly
- copy/paste and key behavior unchanged
- agent receives context packet with provenance IDs

## Migration and Rollout

1. Add `memory` module scaffolding and typed interfaces.
2. Add `NoopMemoryRuntime` and wire in app startup/shutdown.
3. Add event emission from terminal input/output boundaries.
4. Add persistent store implementation.
5. Add embeddings + semantic edges.
6. Add orchestrator query API.
7. Add scoring fields and optional gating mode.

Each step should be independently shippable behind a feature flag or runtime toggle.

## Definition of Done for Memory MVP

MVP is complete when:

1. every terminal input/output line creates a timestamped text object in DB,
2. each object has an embedding vector,
3. linear and semantic edges are present,
4. orchestrator can request context packet for a group/session query,
5. failures degrade gracefully without breaking terminal workflows,
6. tests and docs are updated per Gestalt standards.
