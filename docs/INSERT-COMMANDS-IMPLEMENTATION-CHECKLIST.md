# Insert-Command Mode + Command Panel Implementation Checklist

This checklist defines implementation steps for Insert-key command mode and a reusable side-panel switcher that can host Local Agent, Commands, and the upcoming Git panel.

Aligned references:

- `docs/GIT-PANEL-IMPLEMENTATION-CHECKLIST.md`
- `GESTALT-STANDARDS.md`

## Product Scope

- Trigger command mode with the `Insert` key from a focused terminal pane.
- While command mode is active, keyboard input is captured by Gestalt and used to autocomplete commands.
- While command mode is active, key events are not forwarded to the PTY.
- `Enter` in command mode inserts the selected command prompt text into the focused terminal input buffer.
- `Esc` or `Insert` exits command mode without inserting text.
- Provide a Commands panel for browse/create/edit/delete of command entries.
- Add a reusable side-panel switcher mechanism so Commands and Git can coexist with Local Agent.

## Explicit Behavior Contract

- Default terminal mode:
- All keystrokes behave exactly as they do now.
- Slash (`/`) continues to pass through to CLI agents unchanged.

- Command mode:
- Opens only on `Insert` with no modifier keys.
- Query accepts normal character entry plus `Backspace` and `Delete`.
- `ArrowUp`/`ArrowDown` move highlighted autocomplete selection.
- `Enter` inserts the selected command prompt bytes via `send_input` (no automatic trailing `\r`).
- `Esc` and `Insert` close the mode and clear transient query state.
- All handled keys call `prevent_default` and `stop_propagation`.

- Focus/target rules:
- Command insertion always targets the terminal that opened command mode.
- If focus/session changes before submit, command mode closes automatically.

## Standards Constraints

- Preserve architecture direction: `ui -> orchestrator -> state/terminal` where practical.
- Keep keyboard accessibility for all new controls, including panel switcher tabs/buttons.
- Keep module file sizes under target by refactoring `workspace.rs` before adding large new UI blocks.
- Maintain current terminal copy/paste and selection behaviors outside command mode.
- Run quality gates before merge:
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`

## Module Plan (Concrete Files)

- Add command domain module:
- `src/commands/mod.rs`
- `src/commands/model.rs`
- `src/commands/matcher.rs`
- `src/commands/validate.rs`
- Export in `src/lib.rs` as `pub mod commands;`

- Update state model for persisted command library:
- `src/state.rs` (new command structs or imported domain types + CRUD methods)

- Refactor UI before feature bulk:
- `src/ui/workspace.rs` (extract sidebar panel area)
- New `src/ui/panels/mod.rs`
- New `src/ui/panels/local_agent_panel.rs` (move existing orchestrator card)
- New `src/ui/panels/panel_switcher.rs`
- New `src/ui/panels/commands_panel.rs`
- New `src/ui/command_palette.rs` (floating autocomplete UI bound to terminal mode)

- Keyboard/input integration:
- `src/ui/terminal_view.rs` (Insert interception and command-mode key routing)
- `src/ui/terminal_input.rs` (shared key helpers for command mode where useful)
- `src/ui.rs` (new signals for panel selection, command mode state, and drafts)

- Styles:
- `src/style/workspace.css` (panel switcher + commands panel + palette styles)
- `src/style/base.css` (shared control tokens only if needed)

- Persistence docs updates:
- `src/README.md`
- `README.md`

## Data Contracts (Implement First)

- Command entities:
- `type CommandId = u32`
- `struct InsertCommand { id, name, prompt, description, tags, updated_at_unix }`
- `struct CommandLibrary { commands: Vec<InsertCommand>, next_command_id: CommandId }`

- UI transient state:
- `enum SidebarPanelKind { LocalAgent, Git, Commands }`
- `struct InsertModeState { session_id, query, highlighted_index, is_open }`
- `struct CommandEditorDraft { name, prompt, description, tags_csv }`

- Matching result model:
- `struct CommandMatch { command_id, score, name_indices, prompt_indices }`

## Matching Rules

- Ranking order:
- exact name match
- name prefix
- token prefix
- substring in name
- substring in description/tags

- Tie-breakers:
- most recently updated command first
- stable lexical order by name

- Performance target:
- matching stays responsive for at least 1,000 stored commands.

## Panel Switcher Foundation (Shared with Git Panel)

- Introduce a single sidebar-panel host in `workspace` with:
- one active panel enum state
- one switcher control bar (keyboard focusable)
- one render match for panel content

- Initial registered panels:
- `LocalAgent` (existing behavior)
- `Commands` (new CRUD panel)
- `Git` (placeholder/stub until git panel implementation lands)

- Contract for future Git integration:
- Git panel plugs into existing switcher without layout rewrites.
- Panel container keeps consistent sizing/splitter behavior across all panels.

## Insert Command Mode Tasks

- In `terminal_view` keydown flow:
- detect `Insert` and open mode before `key_event_to_bytes`
- short-circuit PTY forwarding while mode is open
- route mode-specific keys to query/selection actions
- on submit, call `TerminalManager::send_input` with command prompt bytes
- close mode and restore normal key pipeline

- Overlay/palette rendering:
- show near focused terminal shell
- show query + top matches + highlighted row
- show empty state when no match
- hide immediately on blur/session change/panel unmount

## Commands Panel Tasks

- Browse:
- scrollable command list with search filter
- visible selected row tied to editor form

- Create:
- new command button with default draft
- validate name/prompt non-empty before save

- Edit:
- inline form for name, prompt, description, tags
- dirty-state indicator before save

- Delete:
- explicit delete action with confirmation step
- selection fallback to nearest remaining item

- Feedback:
- inline success/error messages
- preserve keyboard navigation and visible focus states

## Persistence and Migration Tasks

- Persist command library inside `AppState` with `#[serde(default)]` fields for backward compatibility.
- Ensure `AppState::repair_after_restore` initializes command library invariants (`next_command_id`, unique ids).
- Keep schema version at `1` unless a breaking envelope change is introduced.
- Add migration note: older workspaces load with empty command library by default.

## Test Plan

- Unit tests:
- `src/commands/matcher.rs` scoring and tie-break ordering
- `src/commands/validate.rs` name/prompt validation and tag parsing
- `src/state.rs` command CRUD, id allocation, and restore repair logic

- UI/input behavior tests:
- Insert opens mode and is not forwarded as `\x1b[2~`
- while mode active, typed characters do not reach PTY
- Enter inserts selected command prompt bytes (without newline)
- Esc closes mode and restores standard input handling

- Integration tests (`tests/insert_commands.rs`):
- create/edit/delete persists across save/load
- mode session target integrity when focus changes
- sidebar switcher toggles Local Agent/Commands and preserves layout

- Manual verification:
- Confirm slash commands still work in terminal-native tools.
- Confirm clipboard and Ctrl+C behavior remain unchanged when not in command mode.
- Confirm keyboard-only navigation for switcher and Commands panel.

## PR Breakdown

- PR 1: sidebar panel host refactor
- extract local agent panel
- add reusable panel switcher + Git placeholder entry

- PR 2: commands domain + state persistence
- add models, matcher, validation, CRUD, and tests

- PR 3: insert-mode input capture + autocomplete overlay
- key handling interception and terminal insertion behavior

- PR 4: commands CRUD panel
- list/search/editor/delete UX and feedback

- PR 5: polish + docs + regression checks
- README/src docs updates, manual QA notes, quality gates

## Definition of Done for This Feature

- `Insert` opens command mode and does not forward Insert escape sequence to terminal.
- While mode is active, keystrokes are captured by Gestalt and never leak to PTY.
- `Enter` inserts selected command prompt text into the target terminal without auto-running it.
- Commands panel supports browse/create/edit/delete with persistence across restarts.
- Sidebar panel switcher supports Local Agent and Commands now, and Git via the same host when merged.
- Slash-driven CLI workflows remain unaffected.
- Tests and quality gates pass.
