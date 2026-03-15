# ui

## Purpose
`ui` contains Dioxus presentation components, interaction handlers, and UI-side coordination for terminals, Git, autosave, workspace layout, and renderer-specific platform adapters.

## Contents
| File/Folder | Description |
| ----------- | ----------- |
| `workspace.rs` | Main workspace layout and shell composition |
| `state.rs` | Root-shared transient `UiState` and terminal history paging state |
| `auxiliary_panel_host.rs` | Shared docked auxiliary tab host and panel body routing |
| `terminal_view.rs` | Terminal output rendering and mounted-element terminal wiring |
| `terminal_input.rs` | Terminal input, clipboard adapters, and viewport measurement |
| `tab_rail.rs` | Group/session tab strip behavior |
| `commands_panel.rs` | Insert-command library UI |
| `file_browser_panel.rs` | File browser and selection stats UI |
| `host_open.rs` | Cross-platform host default-app launcher for UI file interactions |
| `git_panel.rs` | Graph-first Git panel UI with commit selection, details, and repo actions |
| `git_commit_graph.rs` | Snapshot-driven commit-lane layout builder for the Git panel SVG tree |
| `git_refresh.rs` | Git refresh coordination hook |
| `git_helpers.rs` | Shared helper actions for Git UI |
| `command_palette.rs` | Palette interactions |
| `insert_command_mode.rs` | Insert mode state and controls |
| `local_agent_panel.rs` | Local agent control panel and run-start dispatch surface |
| `native_crt.rs` | Native-only WGPU CRT overlay tied to the shell toggle |
| `native_terminal/` | Native terminal pane pilot adapter and renderer-only pane body seam |
| `run_review_panel.rs` | Latest run checkpoint review UI |
| `run_sidebar_panel_host.rs` | Run-sidebar wrapper around the shared auxiliary dock host |
| `sidebar_panel_host.rs` | Right-sidebar wrapper around the shared auxiliary dock host |

## Problem
Provide responsive desktop UI workflows while delegating domain behavior to lower layers.

## Constraints
- Must preserve keyboard-first interaction.
- Must avoid direct PTY lifecycle ownership.
- Must keep first-interaction paths free of avoidable blocking work.
- Must coexist with polling loops currently used for runtime sync.
- Vectorization business behavior remains Emily-owned; UI dispatches actions and renders status.
- Renderer-specific startup, clipboard, geometry, and post-process integration must stay isolated from terminal/state/orchestrator business logic.

## Decision
Keep UI responsibilities component-focused, route runtime/domain mutations through shared services
and orchestrator APIs, and treat the active path group's visible sessions as the startup critical
path while leaving startup/session coordination ownership outside presentation modules. Keep only
root-shared transient interaction state in `UiState`; feature-local drafts remain component-local.
Decomposition review on 2026-03-08 kept `local_agent_panel.rs` and `run_review_panel.rs` in their
current files despite crossing the soft UI-component size threshold because each still owns one
user-facing workflow and no additional responsibility boundary was introduced by the sidebar refresh
contract fix. The Git panel commit history is intentionally graph-first: the SVG lane tree is the
primary history surface, while commit metadata stays attached to the same rows without changing Git
data ownership or action routing. The native renderer spike keeps the `App`/workspace/terminal
facades stable and moves renderer-specific launch, clipboard, mounted-geometry, and WGPU effect
integration behind narrow presentation adapters instead of spreading renderer conditionals through
domain-facing code.

## Alternatives Rejected
- Single monolithic UI file: rejected due to scale.
- Direct subprocess usage in many components: rejected due to duplication.
- Renderer-specific business forks inside `state` or `terminal`: rejected because renderer choice is
  a presentation/infrastructure concern.

## Invariants
- UI state is transient and presentation-oriented.
- `UiState` owns only root-shared transient state; component-local drafts do not get hoisted into it.
- Per-panel note selection is transient UI state; it must not be persisted back into `state`.
- Persistent/business state changes route through `state`, `orchestrator`, or `persistence` paths.
- UI event handlers and `use_future` lifecycle paths must not call blocking Emily or persistence APIs directly; use async/background facades and apply results back through signals.
- Host default-open actions stay short-lived UI-side helpers; they must not introduce new long-lived task ownership or shell-fragment execution paths.
- Emily vectorization settings UI is a bridge surface only; runtime authority stays in Emily APIs.
- Active path group visible sessions start before deferred sessions in other groups.
- Components remain keyboard reachable.
- Startup/session lifecycle coordination is consumed from orchestrator facades rather than duplicated across UI surfaces.
- Autosave feedback is rendered in UI, but debounce/inflight worker coordination is consumed from orchestrator.
- Local-agent send actions start attributed runs through orchestrator facades; UI does not persist run checkpoints directly.
- Local-agent Emily retrieval is assembled through host-side async helpers; the panel keeps the human-entered command separate from the dispatched prompt payload used for terminal writes.
- Local-agent Emily episode creation and host-side gate interpretation stay in host helpers; the UI only renders the resulting feedback and dispatch counts.
- Run review loads checkpoint-derived data on demand and refreshes from existing Git context signals instead of starting a separate polling loop.
- Sidebar hosts forward the shared `git_refresh_nonce` into repo-aware child panels; refresh invalidation remains owned by the action-producing child surface rather than the container.
- Auxiliary panel host membership, order, and active-tab selection come from durable `state`; UI wrappers only render the host-specific shell and route clicks.
- Git commit tree rendering stays snapshot-driven and presentation-only; lane geometry is derived from `RepoSnapshot.commits` without introducing a second Git state owner in UI.
- Renderer-specific platform shims may adapt clipboard, mounted element geometry, or post-process rendering, but they must not become a second owner for terminal runtime or workspace state.

## Revisit Triggers
- Component files exceed maintainability limits.
- `local_agent_panel.rs` or `run_review_panel.rs` gains another async workflow, background task owner, or persistence concern while already above the soft size threshold.
- Polling loops can be replaced by event-driven updates.
- Commit history density or interaction requirements exceed what the current graph-row layout can express without virtualization or a dedicated graph component boundary.
- The native renderer path requires broader business-layer changes instead of presentation-edge adapters.

## Polling Exceptions
The following loops are currently retained because upstream signal hooks are not yet available at the required boundary:

| Location | Cadence | Why It Exists | Revisit Trigger |
| -------- | ------- | ------------- | --------------- |
| `ui.rs` terminal refresh loop | 33 ms | PTY snapshot revisions are pull-based and shared across many sessions. | Terminal runtime publishes change events directly to UI state. |
| `ui.rs` terminal resize loop | 180 ms | Viewport measurement is sampled from mounted element bounds because terminal surfaces still need a renderer-neutral sizing path. | A reliable event-driven mounted resize/viewport bridge exists for both desktop and native paths. |
| `ui.rs` startup background tick | 120 ms + notify nudges | Deferred session startup and initial history backfill still need bounded background progress, but active path group startup is notify-driven. | Session startup and history restore are fully event-driven. |
| `ui.rs` autosave loop | 1200 ms | Autosave worker completion and signature checks are currently drained by polling. | Autosave worker adopts callback/event notification. |
| `file_browser_panel.rs` refresh loop | 1000 ms + nonce triggers | Uses nonce-driven event triggers with low-frequency fallback for repo/fs drift. | File-system/repo watcher events can fully replace fallback cadence. |
| `git_refresh.rs` coordinator loop | 500 ms | Event bus + debounced scheduling still requires periodic due checks. | Scheduler is converted to timer/event queue without tick loop. |

Resource polling is root-owned in `ui.rs`; child components consume the shared snapshot rather than starting duplicate samplers.

## Dependencies
**Internal:** `state`, `terminal`, `orchestrator`, `git`, `persistence`  
**External:** `dioxus`, `dioxus-native` (native spike only), `tokio`

## API Consumer Contract
- UI components may issue async requests to shared services and bridge adapters, but they must not block the UI-sensitive task path on worker responses or disk I/O.
- Startup restore, history backfill, and embedding actions must tolerate stale results caused by session/path changes.
- Background loops must have one owner, one cleanup path, and explicit overlap prevention.
- Autosave coordination may compute signatures and schedule work, but snapshot building and projection/workspace disk writes must remain background-owned.
- Poll loops should project only the state they need; they must not clone full `AppState` snapshots on a fixed cadence when an ID/path projection is sufficient.
- Compatibility note: synchronous bridge wrappers may remain for non-UI callers, but UI code must prefer async variants.
- Git panel consumers may assume commit selection stays keyed by commit SHA across refreshes, while graph layout stays a pure function of the latest repo snapshot.

## Related ADRs
None.

## Usage Examples
```rust
// ui.rs mounts the root app component.
// main.rs chooses the desktop or native renderer bootstrap.
// Nested components consume shared signals passed from App.
```
