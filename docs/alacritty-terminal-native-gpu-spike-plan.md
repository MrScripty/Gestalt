# Plan: Alacritty Terminal Native GPU Spike

## Objective

Prove that Gestalt can adopt Alacritty-style terminal semantics and a native GPU
render path without destabilizing the current app-facing terminal and
orchestrator contracts.

## Scope

### In Scope

- Add a contained spike path rooted at `main` commit `8e9b832`.
- Preserve existing Gestalt terminal/orchestrator facades during the spike.
- Implement a standalone native GPU terminal demo backed by a real PTY and
  `alacritty_terminal` semantics.
- Add focused verification for ANSI parsing, terminal damage, resize, and basic
  input/render behavior.

### Out of Scope

- Replacing the existing Dioxus terminal panes in the main app.
- Migrating persistence, Emily, Git, or orchestration workflows onto the new
  renderer path.
- Polishing typography, theming, or production-grade terminal feature parity.
- Shipping a final cross-platform integration inside the desktop UI.

## Inputs

### Problem

Gestalt's current PTY send path is fast, but terminal rendering is dominated by
line-string snapshot rebuilding and Dioxus row rendering. The spike needs to
validate whether a cell-grid terminal core with a native GPU-presented surface
can remove that bottleneck.

### Constraints

- The current `main` branch has no native renderer seam.
- Existing app-facing terminal ownership must remain single-owner and stable.
- The spike must stay reviewable and isolated from unrelated work.
- New dependencies must be justified and scoped to the binary/leaf path.

### Assumptions

- A standalone spike binary is the lowest-risk way to validate viability before
  invasive app integration.
- A local path dependency on the checked-out Alacritty repo is acceptable for a
  spike branch and can be replaced with a registry dependency later if the
  direction is adopted.
- The first render pass can prioritize terminal correctness and damage-driven
  redraw over production-level visual polish.

### Dependencies

- `portable_pty` for PTY lifecycle.
- `alacritty_terminal` for terminal emulation semantics and damage reporting.
- A native window + GPU-backed presentation stack for the standalone demo.
- Existing Gestalt shell/session conventions for default startup behavior.

### Risks

| Risk | Impact | Mitigation |
| ---- | ------ | ---------- |
| Standalone spike diverges from eventual app integration needs | Medium | Preserve app-facing contracts and keep the spike adapter narrow |
| Native renderer lifecycle introduces ownership/race bugs | High | Keep PTY/runtime ownership in one controller and isolate rendering as a consumer |
| Cross-platform backend details exceed spike scope | Medium | Keep platform-specific code in thin modules and gate unsupported paths clearly |
| Local path dependency on Alacritty becomes sticky | Low | Document that the dependency is spike-only and revisit before broader adoption |

## Definition of Done

- A standards-aligned spike plan is checked into the repo.
- A new `terminal_native` module exists with documented responsibilities.
- A standalone binary can launch a PTY-backed shell using
  `alacritty_terminal` semantics and present terminal output through a native
  GPU-backed render path.
- Focused tests cover terminal core adapter behavior.
- The spike can be built and its targeted verification commands run from the new
  branch.

## Milestones

### Milestone 1: Boundary and Documentation

**Goal:** Define the spike boundary and document the preserved contracts.

**Tasks:**
- [ ] Add a plan artifact aligned to repo standards.
- [ ] Create a dedicated `terminal_native` module boundary with README.
- [ ] Update source/binary documentation to explain the spike role.

**Verification:**
- README/plan content matches the required documentation sections.
- New module ownership and lifecycle notes are explicit.

**Status:** In progress

### Milestone 2: Terminal Core Adapter

**Goal:** Feed PTY bytes into an Alacritty terminal core while preserving clear
runtime ownership.

**Tasks:**
- [ ] Add the spike dependency set with a written rationale in module docs.
- [ ] Implement PTY startup, reader thread, resize, and input handling for the
  standalone path.
- [ ] Implement an adapter that exposes renderable cells and damage information
  from `alacritty_terminal`.
- [ ] Add focused tests for parsing, cursor state, and damage updates.

**Verification:**
- Affected tests pass.
- The standalone binary builds successfully.

**Status:** Not started

### Milestone 3: Native GPU Demo Surface

**Goal:** Render the terminal state through a native GPU-presented surface with
damage-aware redraw.

**Tasks:**
- [ ] Implement the native window/render loop in a standalone diagnostic binary.
- [ ] Draw terminal cells, backgrounds, and cursor from the adapter output.
- [ ] Route keyboard input and resize events back into the PTY/controller path.
- [ ] Keep platform-specific handling isolated from terminal semantics.

**Verification:**
- Manual run confirms shell startup, typing, and resize behavior.
- Build/test commands covering the demo path complete successfully.

**Status:** Not started

### Milestone 4: Verification and Decision Capture

**Goal:** Leave the spike in a traceable, reviewable state.

**Tasks:**
- [ ] Run targeted verification and record the commands used.
- [ ] Update module/binary documentation with usage and constraints.
- [ ] Summarize adoption risks and next-step integration triggers.

**Verification:**
- Verification commands are recorded in the completion summary.
- Traceability docs reflect the shipped spike boundary.

**Status:** Not started

## Execution Notes

- Ownership and lifecycle note:
  - The spike runtime controller owns PTY startup, shutdown, input, resize, and
    terminal-state mutation.
  - The renderer owns only window lifecycle and frame presentation.
  - The reader thread sends bytes to the controller-owned terminal core; it does
    not mutate renderer state directly.
  - Shutdown is driven by one stop signal, and the PTY child must be terminated
    from the controller boundary.

- Public facade preservation note:
  - This spike is facade-first.
  - The main app's `terminal.rs`, `orchestrator`, and Dioxus pane contracts are
    not rewritten in this branch.
  - Integration into the app is deferred until the standalone spike proves the
    direction.

## Commit Cadence Notes

- Commit when one logical slice is complete and verified.
- Follow conventional commit rules from `COMMIT-STANDARDS.md`.
- Review `origin/main..HEAD` before each new commit for accidental regression
  plus fix pairs.

## Re-Plan Triggers

- The standalone spike cannot consume a real PTY with acceptable lifecycle
  clarity.
- The chosen render stack cannot support the minimal terminal behaviors needed
  for evaluation.
- Cross-platform isolation requires a different module boundary than planned.
- The Alacritty dependency shape forces broader architectural changes in Gestalt
  core modules.

## Recommendations

- Recommendation 1: Keep the first spike outside the Dioxus app shell.
  - Why: mainline `main` does not yet have a stable native renderer seam, so
    standalone validation is lower risk and easier to measure.
  - Impact: faster proof-of-concept, delayed pane integration work.

- Recommendation 2: Treat dependency additions as spike-scoped leaf concerns.
  - Why: heavy native rendering dependencies belong in the demo binary path
    first, not the shared core until the architecture is proven.
  - Impact: cleaner future extraction if the spike is rejected.

## Completion Summary

### Completed

- None yet.

### Deviations

- None yet.

### Follow-Ups

- None yet.

### Verification Summary

- None yet.

### Traceability Links

- Module README updated: N/A
- ADR added/updated: N/A
- PR notes completed per `templates/PULL_REQUEST_TEMPLATE.md`

## Brevity Note

This plan stays intentionally narrow: prove semantics and rendering viability
first, then reconsider app integration.
