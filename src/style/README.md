# style

## Purpose
`style` stores CSS split by UI concern so visual changes can be made without mixing with Rust component logic.

## Contents
| File/Folder | Description |
| ----------- | ----------- |
| `base.css` | Global variables, reset, and shared tokens |
| `workspace.css` | Workspace shell layout rules |
| `git_panel.css` | Git panel component styles |
| `commands_panel.css` | Command library panel styles |
| `file_browser_panel.css` | File browser panel styles |
| `run_review_panel.css` | Run checkpoint review panel styles |

## Problem
The desktop UI needs cohesive styling with clear ownership per panel, including read-only review surfaces for orchestrated runs.

## Constraints
- Must work in Dioxus desktop rendering context.
- Needs maintainable selectors and theme variables.

## Decision
Partition CSS by feature panel plus a base stylesheet for shared primitives so new sidebar surfaces can be added without growing one catch-all stylesheet.

## Alternatives Rejected
- Inline style strings in components: rejected due to readability and reuse limits.
- Single large stylesheet: rejected due to maintainability.

## Invariants
- Shared variables live in `base.css`.
- Feature files scope panel-specific classes.
- Run review styling stays in its own file instead of expanding `workspace.css` further.

## Revisit Triggers
- Styling duplication grows across files.
- Theme system requires runtime switching.

## Dependencies
**Internal:** `ui.rs` stylesheet concat  
**External:** None

## Related ADRs
None.

## Usage Examples
```rust
const STYLE: &str = concat!(
    include_str!("style/base.css"),
    include_str!("style/workspace.css"),
    include_str!("style/run_review_panel.css")
);
```
