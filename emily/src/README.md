# emily/src

## Purpose

`emily/src` implements the in-process Emily memory subsystem API, runtime orchestration, data model, and storage abstractions used by Gestalt.

This source tree currently covers the memory-side runtime, not the full Emily sovereign-cognition architecture described in the local March 2026 paper.

## Contents

| File/Folder | Description |
| ----------- | ----------- |
| `lib.rs` | Library exports |
| `api.rs` | Public Emily API trait |
| `runtime.rs` | Default runtime facade and API implementation |
| `runtime/` | Runtime submodules for vectorization jobs and runtime tests |
| `model.rs` | Shared request/response and domain models |
| `error.rs` | Typed Emily error types |
| `inference.rs` | Embedding provider facade and feature gating |
| `inference/` | Provider-specific embedding integrations |
| `store/` | Storage interfaces and concrete backends |

## Problem

Provide a modular memory layer that can ingest terminal text, persist text/vector state, and answer context/history queries without coupling Gestalt to one database API or one embedding provider.

## Constraints

- Must support typed async APIs.
- Storage backend should be swappable behind trait boundaries.
- Pantograph integration must use workflow-session contracts, not direct inference APIs.
- The current boundary should stay narrow enough that broader Emily layers can be added later without destabilizing Gestalt.

## Decision

Use trait-based API/store abstractions with a default runtime wiring focused on persistence, retrieval, vectorization, and runtime diagnostics.

## What This Layer Does Not Yet Cover

The local paper set describes broader Emily components that are not implemented here today:

- `Semantic Membrane`
- provider routing / multi-model dispatch
- local legend mapping and reconstruction
- `ECCR`
- `AOPO/APC`

Those belong above this crate boundary unless the crate is intentionally expanded later.

## Invariants

- `EmilyApi` remains the primary integration contract for the current memory runtime.
- Runtime validates inputs at API boundaries.
- Text vectors are persisted separately from text object records.
- Vectorization configuration and job state are Emily-owned runtime data.
- Pantograph session lifecycle is managed by embedding providers, not by store modules.
- Newly ingested text objects are not treated as integrated memory by default.
- Health diagnostics describe real in-flight work, not implied queue state.

## Revisit Triggers

- Multi-tenant storage requirements expand.
- Streaming query APIs are introduced.
- Emily grows from a memory runtime into a fuller sovereign-dispatch runtime.

## Dependencies

**Internal:** `store`  
**External:** `tokio`, `async-trait`, `serde`, `pantograph-workflow-service` (feature-gated)

## Related ADRs

None.

## Usage Example

```rust
use emily::runtime::EmilyRuntime;
use emily::store::surreal::SurrealEmilyStore;
use std::sync::Arc;

let runtime = EmilyRuntime::new(Arc::new(SurrealEmilyStore::new()));
# let _ = runtime;
```
