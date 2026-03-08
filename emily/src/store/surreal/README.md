# emily/src/store/surreal

## Purpose

This directory contains the embedded SurrealDB backend for Emily persistence. The directory boundary exists so the backend implementation can evolve beyond a single file while keeping the `EmilyStore` facade and backend-specific tests grouped together.

## Contents

| File/Folder | Description |
| ----------- | ----------- |
| `mod.rs` | Surreal-backed `EmilyStore` facade and trait implementation delegation. |
| `ecgl.rs` | Integrity snapshot persistence helpers. |
| `earl.rs` | EARL evaluation persistence helpers. |
| `text.rs` | Text, vector, edge, history, and lexical fallback persistence helpers. |
| `episodes.rs` | Episode, trace-link, outcome, and audit persistence helpers. |
| `sovereign.rs` | Routing-decision, remote-episode, and validation-outcome persistence helpers. |
| `tests.rs` | Backend integration tests for history, vectors, edges, episode artifacts, and sovereign artifacts. |

## Problem

The Surreal backend now owns text objects, vectors, edges, paging, legacy lexical fallback queries, episode-oriented persisted artifacts, sovereign persisted artifacts, EARL evaluation records, and ECGL integrity snapshots. Splitting it into a subdirectory keeps the implementation within the repo's file-size and documentation thresholds.

## Constraints

- The backend must stay behind the `EmilyStore` trait.
- Errors must be normalized to `EmilyError`.
- Tests must use real backend behavior rather than only mocks.

## Decision

Use a `store/surreal/` directory with `mod.rs` for implementation and a separate test module for backend coverage.

## Alternatives Rejected

- Keep the backend in one file: rejected because it exceeded the file-size threshold and mixed implementation with integration tests.
- Move tests to the top-level `tests/` tree: rejected because these tests are backend-specific and easier to maintain adjacent to the implementation.

## Invariants

- `SurrealEmilyStore` remains the embedded backend exported at `emily::store::surreal`.
- Text objects, vectors, and edges remain stored as distinct record types.
- Episode records, trace links, outcomes, and audits remain stored as distinct record types.
- EARL evaluations remain stored as distinct record types.
- Integrity snapshots remain stored as distinct record types.
- Routing decisions, remote episodes, and validation outcomes remain stored as distinct record types.
- Backend tests validate real persistence behavior, not only helper functions.

## Revisit Triggers

- Query logic becomes large enough to deserve a dedicated backend query submodule.
- Backend-specific migrations or indexing logic are added.

## Dependencies

**Internal:** `crate::store`, `crate::model`, `crate::error`  
**External:** `surrealdb`, `tokio`

## Related ADRs

- None identified as of 2026-03-08.
- Reason: current work preserves the existing backend boundary and public path.
- Revisit trigger: backend restructuring changes public crate paths or introduces backend capability flags.

## Usage Examples

```rust
use emily::store::surreal::SurrealEmilyStore;

let store = SurrealEmilyStore::new();
# let _ = store;
```

## API Consumer Contract

- Consumers should treat this backend as one implementation of `EmilyStore`.
- Callers must open a database before using persistence methods.
- Backend-specific details stay internal; hosts should prefer the trait or Emily runtime facade.

## Structured Producer Contract

- This backend writes `text_objects`, `text_vectors`, `text_edges`, `episodes`, `episode_trace_links`, `outcomes`, `routing_decisions`, `remote_episodes`, `validation_outcomes`, `earl_evaluations`, `integrity_snapshots`, `audit_records`, and runtime config records.
- Stable record-field semantics are defined by Emily model types.
- If persisted record shapes change, compatibility or migration behavior must be documented before merge.
