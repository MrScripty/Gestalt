# AppState Domain Split Investigation

## Scope

Investigate whether `AppState` should be decomposed into smaller domains, with
minimum targets:

- `WorkspaceState`
- `KnowledgeState`
- `CommandState`
- transient `UiState`

This review is based on the current Gestalt codebase and the shared/project
standards in:

- `/media/jeremy/OrangeCream/Linux Software/Coding-Standards/CODING-STANDARDS.md`
- `/media/jeremy/OrangeCream/Linux Software/Coding-Standards/ARCHITECTURE-PATTERNS.md`
- `GESTALT-STANDARDS.md`

## Standards Fit

The current shape exceeds the decomposition triggers in the shared standards:

- `src/state.rs` is 1425 lines.
- It owns more than 3 responsibilities.
- It exposes far more than roughly 7 public functions.

Relevant standards:

- Files over 500 lines require decomposition review.
- Modules/services over roughly 7 public functions or 3 distinct
  responsibilities require decomposition review.
- Domain/state code should remain framework-independent and side-effect free.
- UI should own transient presentation state, not persistent business state.

## Current Responsibility Map

`AppState` currently mixes four durable concerns plus one cross-cutting mutation
counter:

### Workspace concern

- groups
- sessions
- session order and group membership
- selected session
- group layout
- persisted UI scale
- session/group ID allocators
- workspace restore repair

Representative methods:

- `create_group_with_defaults`
- `remove_group`
- `add_session_with_title_and_role`
- `move_session_before`
- `swap_session_with_visible_agent_slot`
- `active_group_id`
- `group_layout`

### Knowledge concern

- notes
- snippets
- snippet embedding metadata
- selected note
- note/snippet ID allocators
- knowledge restore repair

Representative methods:

- `notes_for_group`
- `select_note`
- `create_note_for_group`
- `update_note_markdown`
- `create_snippet`
- `set_snippet_embedding_ready`

### Command concern

- insert command library
- command restore repair

Representative methods:

- `commands`
- `command_by_id`
- `create_insert_command`
- `update_insert_command`
- `delete_insert_command`

### UI-ish concern currently persisted in AppState

- selected session
- selected note
- UI scale
- group layout

These are not equal in domain weight:

- `selected_session` affects startup priority and active group behavior, so it is
  not safe to treat as purely transient UI state.
- `selected_note_id` is used only by the notes UI flow and is a good candidate
  to become transient UI state.
- `ui_scale` and `group_layout` are persisted view preferences. They are
  presentation-oriented, but because they intentionally survive restart, they
  should remain in a durable domain model unless a separate persisted
  preferences model is introduced.

## Coupling Observations

### Persistence currently serializes the whole monolith

`persistence::build_workspace_snapshot` clones the entire `AppState` and then
derives terminal snapshots from `app_state.sessions`.

Implication:

- A split is feasible, but persistence needs a typed aggregate snapshot rather
  than assuming one flat mutable object.

### Orchestrator mostly depends on workspace data

`orchestrator/runtime.rs`, `orchestrator/workspace.rs`, and
`orchestrator/startup.rs` consume:

- sessions
- groups
- selected session
- group paths/layout

They do not materially depend on notes, snippets, or commands.

Implication:

- `WorkspaceState` is a natural first-class dependency for orchestrator APIs.

### Notes and snippets are already a coherent subdomain

`ui/notes_panel.rs` consumes note selection, note editing, snippet promotion,
and snippet insertion from the same area of `AppState`.

Implication:

- `KnowledgeState` is a clean extractable unit with a clear UI consumer.

### Commands are already mostly extracted at the model level

`commands/model.rs` already owns `CommandLibrary` behavior.

Implication:

- `CommandState` is the lowest-risk extraction. In practice it can initially be
  a thin wrapper around `CommandLibrary`.

### Raw field access will block a clean boundary unless reduced

Some modules still reach into `AppState` internals directly:

- `orchestrator/runtime.rs`
- `orchestrator/startup.rs`
- `orchestrator/autosave.rs`
- `ui/tab_rail.rs`
- tests that mutate `state.groups` and `state.sessions` directly

Implication:

- The first migration step should introduce narrower read/write APIs before or
  during the structural split.

## Recommended Target Shape

```rust
pub struct AppState {
    pub workspace: WorkspaceState,
    pub knowledge: KnowledgeState,
    pub commands: CommandState,
    #[serde(skip, default)]
    revision: u64,
}
```

Recommended contents:

### WorkspaceState

- `sessions`
- `groups`
- `selected_session`
- `next_session_id`
- `next_group_id`
- `ui_scale`

And workspace-only behavior:

- group/session creation and removal
- reorder and slot swap rules
- active group derivation
- layout mutation and normalization
- workspace restore repair

### KnowledgeState

- `notes`
- `snippets`
- `next_note_id`
- `next_snippet_id`

And knowledge-only behavior:

- note CRUD
- snippet CRUD
- snippet embedding status updates
- restore repair for notes/snippets

`selected_note_id` should move out of this durable domain.

### CommandState

- `command_library`

And command-only behavior:

- command CRUD
- restore repair

### Transient UiState

Do not create a second giant monolith. Split by sharing boundary:

- root-shared transient state stays in `ui` root or small `ui/state` structs
- feature-local state stays local to the component

Reasonable root-shared transient state:

- `dragging_tab`
- `focused_terminal`
- `round_anchor`
- `sidebar_panel`
- `sidebar_open`
- `git_context`
- `git_context_loading`
- `insert_mode_state`
- `terminal_history_state`
- rename/drag interaction state shared across workspace surfaces

Keep local to feature components:

- command editor drafts in `CommandsPanel`
- notes view mode/search/delete-hold state in `NotesPanel`
- one-off feedback strings that are not shared across screens

## Selection and Preference Decisions

### Keep durable

- `selected_session`
  - It affects active group, startup ordering, and orchestrator projections.
- `ui_scale`
  - It is intentionally persisted across restart.
- `group_layout`
  - It is intentionally persisted and group-scoped.

### Move to transient UI state

- `selected_note_id`
  - It is not used by orchestrator or persistence logic other than legacy
    restore cleanup.
  - It behaves like editor navigation state.

## Migration Order

### Phase 1: Extract state modules without changing runtime wiring

- Convert `src/state.rs` into `src/state/mod.rs` plus:
  - `src/state/workspace.rs`
  - `src/state/knowledge.rs`
  - `src/state/commands.rs`
- Keep `AppState` as a thin aggregate/facade.
- Delegate existing methods to the new domain types where practical.

Goal:

- Reduce file size and make responsibilities explicit before changing signal
  topology.

### Phase 2: Remove raw field access outside state

- Replace direct reads of `.sessions`, `.groups`, and `.selected_session` with
  workspace-domain accessors.
- Replace direct note/snippet access in UI with knowledge-domain accessors.
- Update tests to stop mutating internals directly unless the test is explicitly
  about serialization compatibility.

Goal:

- Enforce boundaries in call sites before changing the aggregate structure more
  aggressively.

### Phase 3: Split reactive signals in UI

- Replace `Signal<AppState>` in domain-specific surfaces with:
  - `Signal<WorkspaceState>`
  - `Signal<KnowledgeState>`
  - `Signal<CommandState>`
- Leave persistence/autosave with a composed snapshot input.

Good first targets:

- `CommandsPanel` only needs `CommandState`.
- `NotesPanel` mostly needs `KnowledgeState` plus active workspace group.
- `TabRail`, workspace projections, startup, and autosave mostly need
  `WorkspaceState`.

Goal:

- Stop rerendering or cloning unrelated durable domains for feature-local work.

### Phase 4: Move note selection into transient UI state

- Replace `selected_note_id` persistence with a UI-owned signal keyed by active
  group when needed.
- On startup, derive a sensible default from `KnowledgeState` rather than
  restoring editor navigation.

Goal:

- Keep persistent models focused on data and behavior, not editor cursor state.

## Recommended First Implementation Slice

If this is started now, the best first slice is:

1. Extract `WorkspaceState`, `KnowledgeState`, and `CommandState` into separate
   files while preserving the existing JSON shape with `serde(flatten)` or a
   compatibility migration.
2. Move command methods off the monolith first.
3. Update `CommandsPanel` to depend on `CommandState` instead of full
   `AppState`.
4. Introduce workspace read accessors used by orchestrator and autosave so raw
   field access stops spreading.

Why this order:

- `CommandState` is already mostly isolated.
- It delivers a real boundary quickly.
- It avoids forcing the workspace and knowledge split to land in one large
  refactor.

## Bottom Line

Yes, the codebase is ready for this split, and the current `AppState` shape is
already beyond the standards thresholds.

The safe domain split is:

- durable `WorkspaceState`
- durable `KnowledgeState`
- durable `CommandState`
- transient `UiState`

But `UiState` should only own truly transient interaction state. `selected_note`
should move there; `selected_session`, `ui_scale`, and `group_layout` should not.
