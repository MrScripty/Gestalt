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
- Must keep first-interaction paths free of avoidable blocking work.
- Must coexist with polling loops currently used for runtime sync.
- Vectorization business behavior remains Emily-owned; UI dispatches actions and renders status.

## Decision
Keep UI responsibilities component-focused, route runtime/domain mutations through shared services
and orchestrator APIs, and treat the active path group's visible sessions as the startup critical
path while leaving startup/session coordination ownership outside presentation modules.

## Alternatives Rejected
- Single monolithic UI file: rejected due to scale.
- Direct subprocess usage in many components: rejected due to duplication.

## Invariants
- UI state is transient and presentation-oriented.
- Persistent/business state changes route through `state`, `orchestrator`, or `persistence` paths.
- UI event handlers and `use_future` lifecycle paths must not call blocking Emily or persistence APIs directly; use async/background facades and apply results back through signals.
- Emily vectorization settings UI is a bridge surface only; runtime authority stays in Emily APIs.
- Active path group visible sessions start before deferred sessions in other groups.
- Components remain keyboard reachable.
- Startup/session lifecycle coordination is consumed from orchestrator facades rather than duplicated across UI surfaces.
- Autosave feedback is rendered in UI, but debounce/inflight worker coordination is consumed from orchestrator.

## Revisit Triggers
- Component files exceed maintainability limits.
- Polling loops can be replaced by event-driven updates.

## Polling Exceptions
The following loops are currently retained because upstream signal hooks are not yet available at the required boundary:

| Location | Cadence | Why It Exists | Revisit Trigger |
| -------- | ------- | ------------- | --------------- |
| `ui.rs` terminal refresh loop | 33 ms | PTY snapshot revisions are pull-based and shared across many sessions. | Terminal runtime publishes change events directly to UI state. |
| `ui.rs` terminal resize loop | 180 ms | Viewport measurement is DOM-driven and currently sampled. | Reliable resize observer bridge is available in Dioxus desktop layer. |
| `ui.rs` startup background tick | 120 ms + notify nudges | Deferred session startup and initial history backfill still need bounded background progress, but active path group startup is notify-driven. | Session startup and history restore are fully event-driven. |
| `ui.rs` autosave loop | 1200 ms | Autosave worker completion and signature checks are currently drained by polling. | Autosave worker adopts callback/event notification. |
| `file_browser_panel.rs` refresh loop | 1000 ms + nonce triggers | Uses nonce-driven event triggers with low-frequency fallback for repo/fs drift. | File-system/repo watcher events can fully replace fallback cadence. |
| `git_refresh.rs` coordinator loop | 500 ms | Event bus + debounced scheduling still requires periodic due checks. | Scheduler is converted to timer/event queue without tick loop. |

Resource polling is root-owned in `ui.rs`; child components consume the shared snapshot rather than starting duplicate samplers.

## Dependencies
**Internal:** `state`, `terminal`, `orchestrator`, `git`, `persistence`  
**External:** `dioxus`, `tokio`

## API Consumer Contract
- UI components may issue async requests to shared services and bridge adapters, but they must not block the UI-sensitive task path on worker responses or disk I/O.
- Startup restore, history backfill, and embedding actions must tolerate stale results caused by session/path changes.
- Background loops must have one owner, one cleanup path, and explicit overlap prevention.
- Autosave coordination may compute signatures and schedule work, but snapshot building and projection/workspace disk writes must remain background-owned.
- Poll loops should project only the state they need; they must not clone full `AppState` snapshots on a fixed cadence when an ID/path projection is sufficient.
- Compatibility note: synchronous bridge wrappers may remain for non-UI callers, but UI code must prefer async variants.

## Related ADRs
None.

## Usage Examples
```rust
// ui.rs mounts the root app component.
// Nested components consume shared signals passed from App.
```
