# commands

## Purpose
`commands` contains insert-command domain models, validation, and ranking logic used by the UI and persisted workspace state.

## Contents
| File/Folder | Description |
| ----------- | ----------- |
| `mod.rs` | Public module exports |
| `model.rs` | Command entities and command-library mutations |
| `validate.rs` | Boundary validation and tag parsing helpers |
| `matcher.rs` | Query scoring and ranking algorithm |

## Problem
Provide a stable command library abstraction so command creation, update, and lookup rules are centralized.

## Constraints
- Must serialize cleanly with workspace persistence.
- Matching must stay fast for large command lists.
- Validation failures must be UI-friendly.

## Decision
Keep command logic as a small domain module with pure helpers and unit tests.

## Alternatives Rejected
- Moving command logic into UI components: rejected due to coupling.
- Storing commands as untyped maps: rejected due to safety and migration risk.

## Invariants
- `CommandId` values remain unique.
- `next_command_id` always advances beyond max existing id.
- Empty command name and prompt are invalid.

## Revisit Triggers
- Command metadata grows beyond current schema.
- Matching latency exceeds UX target.

## Dependencies
**Internal:** `state`, `persistence`  
**External:** `serde`

## Related ADRs
None.

## Usage Examples
```rust
use crate::commands::CommandLibrary;

let mut library = CommandLibrary::default();
let id = library.create("Build".to_string(), "cargo build".to_string(), String::new(), vec![]);
assert!(library.command(id).is_some());
```
