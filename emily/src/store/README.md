# emily/src/store

## Purpose
`store` defines persistence contracts and concrete storage implementations for Emily text objects, vector records, episode artifacts, and query operations.

## Contents
| File/Folder | Description |
| ----------- | ----------- |
| `mod.rs` | `EmilyStore` trait and module exports |
| `surreal/` | Embedded SurrealDB-backed store implementation and backend tests |

## Problem
Emily runtime needs persistence operations without coupling runtime logic to one database API.

This is the storage boundary for the current memory runtime. It does not attempt to model broader sovereign-dispatch concerns such as membrane budgets, provider routing, or local reconstruction state.

## Constraints
- Async-safe trait methods.
- Deterministic paging and ranking behavior.

## Decision
Define `EmilyStore` as the storage boundary and keep backend details inside implementation modules.

## Alternatives Rejected
- Direct database calls from runtime: rejected due to layering violations.

## Invariants
- `EmilyStore` trait remains backend-agnostic.
- Backend modules map storage failures to `EmilyError`.
- Vector writes use dedicated records (`text_vectors`) instead of embedding fields on text objects.
- Episode, trace-link, outcome, and audit writes use dedicated record families rather than being embedded into text rows.
- Runtime vectorization config is persisted in dedicated runtime config records.

## Compatibility Notes

- The current Milestone 3 schema additions are additive:
  - `episodes`
  - `episode_trace_links`
  - `outcomes`
  - `audit_records`
- Existing databases containing only text/vector/runtime-config records remain valid.
- Replay safety for durable write paths is enforced in the runtime facade through idempotent record IDs and conflict checks.

## Revisit Triggers
- Additional backend support is required.
- Query/index performance requires backend-specific capability flags.
- Emily storage needs to represent broader audit objects such as remote-episode records or membrane-boundary telemetry.

## Dependencies
**Internal:** `model`, `error`  
**External:** `surrealdb`

## Related ADRs
None.

## Usage Examples
```rust
use emily::store::EmilyStore;
use emily::store::surreal::SurrealEmilyStore;

let store = SurrealEmilyStore::new();
# let _ = store;
```
