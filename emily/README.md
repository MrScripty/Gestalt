# Emily Crate

## Purpose

Reusable, host-agnostic Emily memory runtime that ingests arbitrary text objects and provides context retrieval, history paging, and vectorization maintenance against addressable databases.

Current scope is the memory and embedding-integrity core. This crate does not yet implement the broader March 2026 Emily sovereign-cognition layer such as `Semantic Membrane`, provider routing, local legend mapping, or multi-model dispatch orchestration.

## Public API

- `EmilyApi`: open/switch/close database, ingest text, query context, page history
- Retrieval policy surface: read/update `MemoryPolicy`
- Vectorization control surface: config updates, status, backfill/revectorize jobs, cancellation
- Generic DTOs in `model.rs` (no Gestalt-specific types)
- Optional Pantograph workflow-session embedding provider via feature `pantograph`

## Current Architectural Position

This crate currently corresponds to the Emily memory subsystem described in the research notes:

- persistent text-object store
- vector store and retrieval
- scoring-related fields on stored objects
- runtime health and vectorization operations

It is best treated as the local persistence / retrieval / embedding core that a broader Emily architecture could build on later.

Current policy fields on stored objects are provisional. Until active `EARL` and
`ECGL` runtimes exist, the crate should not imply that stored confidence,
learning, or integration values are the result of a real policy engine.

## Internal Modules

- `api`: transport-agnostic public contracts
- `model`: canonical data structures
- `store`: storage traits + Surreal-backed implementation for text objects and vectors
- `runtime`: default in-process API implementation and background vectorization jobs
- `inference`: embedding provider contracts + Pantograph workflow-session client adapters
- `error`: typed error surface

## Revisit Triggers

- Emily expands from memory-runtime scope into sovereign-dispatch orchestration
- `EARL` / `ECGL` controls move from stored fields to active runtime policy
- Membrane-bound remote reasoning or audit surfaces are added
