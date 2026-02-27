# Persistence

## Purpose
Persists and restores workspace state so groups, sessions, and terminal snapshots survive app restarts.

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

## Dependencies
**Internal:** `state`, `terminal`  
**External:** `serde`, `serde_json`, `thiserror`

