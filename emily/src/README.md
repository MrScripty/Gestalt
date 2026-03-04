# emily/src

## Purpose
`emily/src` implements the in-process memory subsystem API, runtime orchestration, data model, and storage abstractions used by Gestalt.

## Contents
| File/Folder | Description |
| ----------- | ----------- |
| `lib.rs` | Library exports |
| `api.rs` | Public Emily API trait |
| `runtime.rs` | Default runtime implementation |
| `model.rs` | Shared request/response and domain models |
| `error.rs` | Typed Emily error types |
| `inference.rs` | Embedding provider abstraction |
| `store/` | Storage interfaces and concrete backends |

## Problem
Provide a modular memory layer that can ingest terminal text and answer context/history queries.

## Constraints
- Must support typed async APIs.
- Storage backend should be swappable behind trait boundaries.
- Pantograph integration must use workflow-session contracts, not direct inference APIs.

## Decision
Use trait-based API/store abstractions with a default runtime wiring.

## Alternatives Rejected
- Hard-coding one backend in runtime: rejected due to extensibility.

## Invariants
- `EmilyApi` remains the primary integration contract.
- Runtime validates inputs at API boundaries.
- Text vectors are persisted separately from text object records.
- Vectorization configuration and job state are Emily-owned runtime data.
- Pantograph session lifecycle is managed by embedding providers, not by store modules.

## Revisit Triggers
- Multi-tenant storage requirements expand.
- Streaming query APIs are introduced.

## Dependencies
**Internal:** `store`  
**External:** `tokio`, `async-trait`, `serde`, `pantograph-workflow-service` (feature-gated)

## Related ADRs
None.

## Usage Examples
```rust
use emily::runtime::EmilyRuntime;
use emily::store::surreal::SurrealEmilyStore;
use std::sync::Arc;

let runtime = EmilyRuntime::new(Arc::new(SurrealEmilyStore::new()));
# let _ = runtime;
```
