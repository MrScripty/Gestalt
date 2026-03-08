# Persistence

## Purpose
Persists and restores workspace state so groups, sessions, and terminal projection metadata survive app restarts.

## Contents
| File | Description |
| ---- | ----------- |
| `mod.rs` | Public persistence API used by UI/startup |
| `schema.rs` | Versioned workspace schema contract |
| `migrate.rs` | Schema-version migration dispatch |
| `paths.rs` | Cross-platform workspace state file location |
| `store.rs` | Atomic load/save and corrupt-file quarantine |
| `error.rs` | Typed persistence error enums |

## Design Decisions
- Workspace persistence is best-effort infrastructure: failures degrade gracefully to cold start.
- Schema version is explicit and checked at load.
- Save path is OS-specific but hidden behind `paths.rs`.
- Terminal history lines are not persisted here; Emily is the history source of truth.

## Invariants
- `load_workspace` rejects unsupported schema versions before restored state is used.
- Migration to the current schema strips any terminal history lines from legacy in-memory payloads before returning workspace state.

## Dependencies
**Internal:** `state`, `terminal`  
**External:** `serde`, `serde_json`, `thiserror`
