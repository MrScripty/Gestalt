# orchestration_log

## Purpose
`orchestration_log` persists durable command lifecycles for orchestrated actions
so Gestalt can replay exact execution order after restart and inspect partial
failures.

## Contents
| File | Description |
| ---- | ----------- |
| `mod.rs` | Public exports for the orchestration log domain |
| `model.rs` | Command/event/receipt contracts and timeline DTOs |
| `store.rs` | SQLite-backed append/query repository with timeline sequencing |
| `error.rs` | Typed persistence errors |

## Design Decisions
- SQLite is the source of truth for orchestration durability.
- `sequence_in_timeline` is the replay authority; timestamps are stored alongside it.
- `command_id` is unique and prevents duplicate command insertion.
- Commands, events, and receipts are stored separately, while timeline sequence is allocated centrally.

## Dependencies
**Internal:** `state`  
**External:** `rusqlite`, `serde`, `serde_json`, `thiserror`
