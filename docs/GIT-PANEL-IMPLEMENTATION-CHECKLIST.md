# Git Path-Group Panel Implementation Checklist

This checklist defines implementation steps for a contextual Git side panel in Gestalt.

## Product Scope

- Panel is contextual to the active path-group.
- Group path is treated as repo candidate root (`cwd = group.path`).
- If the path is not a Git repo, render a graceful "No repository" state.
- Commit tree is vertical with newest commits at the top.
- Supported actions:
- Display commits, branches, changed files, tags.
- Stage/unstage files.
- Write commit title and message body and commit.
- Tag selected commit for release.
- Create workspace via Git worktree.
- Checkout branches and commits.

## Standards Constraints

- Respect architecture boundary: `ui -> orchestrator -> state/git service` (no direct git command execution in UI).
- Keep module files under the 500-line target by splitting before adding major features.
- Prefer typed errors for stable modules; avoid `Result<_, String>` in new Git domain code.
- All clickable controls keyboard reachable; keep visible focus states.
- Run quality gates before merge:
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`

## Module Plan (Concrete Files)

- Add new Git domain module:
- `src/git/mod.rs`
- `src/git/model.rs`
- `src/git/error.rs`
- `src/git/command.rs`
- `src/git/parse.rs`
- Export in `src/lib.rs` as `pub mod git;`

- Refactor orchestrator into submodules before adding Git orchestration APIs:
- `src/orchestrator/mod.rs`
- `src/orchestrator/runtime.rs` (existing terminal/group snapshot logic)
- `src/orchestrator/git.rs` (new Git panel orchestration surface)

- Add Git panel UI module:
- `src/ui/git_panel.rs`
- Register with `mod git_panel;` in `src/ui.rs`
- Integrate panel composition in `src/ui/workspace.rs`

- Update styles:
- `src/style/workspace.css` (panel layout + state styles)
- `src/style/base.css` (shared control tokens only if needed)

- Update docs:
- `src/README.md` (new module ownership/data flow)
- `README.md` (feature list)

## Data Contracts (Implement First)

- `src/git/model.rs`:
- `enum RepoContext { Available(RepoSnapshot), NotRepo { inspected_path: String } }`
- `struct RepoSnapshot { root, head, branches, commits, changes, tags }`
- `struct BranchInfo { name, is_current, is_remote }`
- `struct CommitInfo { sha, short_sha, author, subject, body_preview, authored_at, decorations, graph_prefix }`
- `struct FileChange { path, status, staged }`
- `struct TagInfo { name, target_sha, annotated }`
- `struct CommitDraft { title, message }`
- `enum CheckoutTarget { Branch(String), Commit(String) }`

- `src/git/error.rs`:
- `enum GitError { NotRepo, CommandFailed { command, code, stderr }, ParseError { command, details }, InvalidInput(String), Io(std::io::Error) }`
- Implement `Display` and `From<std::io::Error>`.

## Command Surface (No Shell Parsing)

- Use `std::process::Command` with explicit args only.
- Commands to implement in `src/git/command.rs`:
- Repo detect: `git rev-parse --show-toplevel`
- Current branch/head: `git branch --show-current` and `git rev-parse HEAD`
- Branches: `git for-each-ref --format=<format> refs/heads refs/remotes`
- Commits: `git log --graph --decorate --date=iso-strict --pretty=format:<format> -n <N>`
- File status: `git status --porcelain=v2 --branch`
- Tags: `git tag --list --sort=-creatordate --format=<format>`
- Stage: `git add -- <path>`
- Unstage: `git restore --staged -- <path>`
- Commit: `git commit -m <title> [-m <message>]`
- Tag release: `git tag -a <tag> -m <message> <sha>`
- Checkout branch: `git switch <branch>`
- Checkout commit: `git switch --detach <sha>`
- Worktree create: `git worktree add <new_path> <target>`

## Orchestrator API Tasks

- In `src/orchestrator/git.rs`, add:
- `fn load_repo_context(group_path: &str) -> Result<RepoContext, GitError>`
- `fn stage_files(group_path: &str, paths: &[String]) -> Vec<FileOpResult>`
- `fn unstage_files(group_path: &str, paths: &[String]) -> Vec<FileOpResult>`
- `fn create_commit(group_path: &str, draft: CommitDraft) -> Result<String, GitError>`
- `fn create_tag(group_path: &str, name: &str, message: &str, sha: &str) -> Result<(), GitError>`
- `fn checkout_target(group_path: &str, target: CheckoutTarget) -> Result<(), GitError>`
- `fn create_worktree(group_path: &str, new_path: &str, target: &str) -> Result<(), GitError>`

- Add per-file/per-item result type:
- `struct FileOpResult { path: String, error: Option<GitError> }`

## UI Tasks (`src/ui/git_panel.rs`)

- Create `GitPanel` component props:
- `app_state: Signal<AppState>`
- `active_group_id: Option<GroupId>`
- `on_create_group_from_path: EventHandler<String>` (or equivalent callback)

- UI sections:
- Header: repo root + current branch/head.
- Branch list.
- Commit tree list (`graph_prefix + subject`), newest at top.
- Changed files list with staged/unstaged indicator and action buttons.
- Commit editor (`title` input, `message` textarea, commit button).
- Tag action (tag name/message + selected commit).
- Workspace action (new worktree path + target branch/commit).
- Checkout action for selected branch/commit.

- Non-repo state:
- Show inspected path and concise message.
- Disable/hide mutating controls.
- Keep panel mounted to avoid layout shift.

- Accessibility:
- Keyboard tab order across all controls.
- `aria-label` for icon-only controls.
- Focus-visible styling in CSS.

## State + Layout Integration Tasks

- In `src/ui/workspace.rs`:
- Add right-side splitter/layout slot for Git panel.
- Wire active path-group change to repo-context refresh.
- Add in-flight guard per operation to prevent concurrent conflicting actions.
- Add transient feedback string for operation outcomes.

- In `src/ui.rs`:
- Add signals for Git panel UI state (selection, drafts, feedback, loading).
- Add polling/refresh constants for repo refresh cadence after mutations.

## Graceful Non-Repo Behavior (Required Acceptance)

- For non-repo path-groups:
- `load_repo_context` returns `RepoContext::NotRepo`.
- Panel renders informative empty state and never throws.
- No stage/commit/tag/checkout/worktree command executes.
- Switching from repo -> non-repo -> repo updates reliably.

## Test Plan

- Unit tests (`src/git/parse.rs`):
- Parse branch refs.
- Parse `git log --graph` lines into `CommitInfo`.
- Parse porcelain v2 file status into staged/unstaged models.
- Parse tag format lines.
- Not-repo stderr mapping to `GitError::NotRepo`.

- Integration tests (`tests/git_panel_ops.rs`):
- Create temp repo and verify repo detection.
- Verify non-repo path returns `RepoContext::NotRepo`.
- Stage/unstage flow updates status.
- Commit with title/message succeeds and is visible in log.
- Annotated tag creation on selected commit.
- Checkout branch and detached commit.
- Worktree creation creates directory and valid HEAD.

- UI-adjacent behavior tests (`tests/git_panel_context_switch.rs`):
- Context switches with active group changes.
- Non-repo state does not trigger mutating operations.

## PR Breakdown

- PR 1: module refactor prep
- Split `orchestrator` into submodules.
- Split oversized UI/state files as needed to stay within target size before adding feature bulk.

- PR 2: Git domain + orchestrator
- Add `src/git/*`, typed models/errors, command runner, parsers, orchestration APIs, unit tests.

- PR 3: Read-only Git panel
- Add panel layout and non-repo graceful state.
- Show branches/commits/changes/tags.

- PR 4: Mutating actions
- Stage/unstage, commit editor, checkout, tag, feedback/in-flight guards.

- PR 5: Worktree workspace flow
- Add worktree creation and optional auto-create Gestalt path-group.
- Add integration tests + docs updates.

## Definition of Done for This Feature

- Panel is contextual per active path-group.
- Non-repo groups show graceful state with no hard errors.
- Vertical commit tree shows newest-first ordering.
- Stage/commit/tag/checkout/worktree features function with typed error handling.
- Tests and quality gates pass.
- `README.md` and `src/README.md` updated for architecture and user workflow.
