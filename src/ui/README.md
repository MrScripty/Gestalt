# ui

## Purpose
`ui` contains Dioxus presentation components, interaction handlers, and UI-side coordination for terminals, Git, autosave, and workspace layout.

## Contents
| File/Folder | Description |
| ----------- | ----------- |
| `workspace.rs` | Main workspace layout and shell composition |
| `terminal_view.rs` | Terminal output rendering |
| `terminal_input.rs` | Terminal input and viewport measurement |
| `tab_rail.rs` | Group/session tab strip behavior |
| `commands_panel.rs` | Insert-command library UI |
| `file_browser_panel.rs` | File browser and selection stats UI |
| `git_panel.rs` | Git actions and metadata UI |
| `git_refresh.rs` | Git refresh coordination hook |
| `autosave.rs` | Background autosave worker integration |
| `git_helpers.rs` | Shared helper actions for Git UI |
| `command_palette.rs` | Palette interactions |
| `insert_command_mode.rs` | Insert mode state and controls |
| `local_agent_panel.rs` | Local agent control panel |
| `sidebar_panel_host.rs` | Sidebar container selection |

## Problem
Provide responsive desktop UI workflows while delegating domain behavior to lower layers.

## Constraints
- Must preserve keyboard-first interaction.
- Must avoid direct PTY lifecycle ownership.
- Must coexist with polling loops currently used for runtime sync.

## Decision
Keep UI responsibilities component-focused and route runtime/domain mutations through shared services and orchestrator APIs.

## Alternatives Rejected
- Single monolithic UI file: rejected due to scale.
- Direct subprocess usage in many components: rejected due to duplication.

## Invariants
- UI state is transient and presentation-oriented.
- Persistent/business state changes route through `state`, `orchestrator`, or `persistence` paths.
- Components remain keyboard reachable.

## Revisit Triggers
- Component files exceed maintainability limits.
- Polling loops can be replaced by event-driven updates.

## Dependencies
**Internal:** `state`, `terminal`, `orchestrator`, `git`, `persistence`  
**External:** `dioxus`, `tokio`

## Related ADRs
None.

## Usage Examples
```rust
// ui.rs mounts the root app component.
// Nested components consume shared signals passed from App.
```
