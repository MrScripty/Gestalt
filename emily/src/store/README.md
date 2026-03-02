# emily/src/store

## Purpose
`store` defines persistence contracts and concrete storage implementations for Emily text objects and query operations.

## Contents
| File/Folder | Description |
| ----------- | ----------- |
| `mod.rs` | `EmilyStore` trait and module exports |
| `surreal.rs` | Embedded SurrealDB-backed store implementation |

## Problem
Emily runtime needs persistence operations without coupling runtime logic to one database API.

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

## Revisit Triggers
- Additional backend support is required.
- Query/index performance requires backend-specific capability flags.

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
