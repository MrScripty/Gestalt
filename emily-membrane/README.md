# Emily Membrane Crate

## Purpose

`emily-membrane` is the planned sovereign-dispatch layer that sits above the
`emily` core crate. Its job is to own bounded task compilation, routing,
dispatch orchestration, validation orchestration, and local reconstruction
without pushing transport or provider concerns down into Emily's durable memory
and policy core.

This crate exists because the membrane layer has a different dependency profile
and a faster rate of change than `emily`. The separation is architectural, not
just organizational.

## Contents

| File/Folder | Description |
| ----------- | ----------- |
| `Cargo.toml` | Independent package manifest for the sibling membrane crate |
| `src/` | Crate source tree and module-level documentation |

## Problem

Emily's durable core now handles memory, retrieval, policy, sovereign records,
and audit state. The March 2026 sovereign-cognition design still needs a layer
that can compile bounded work for local or remote reasoning paths, but that
layer should not live inside `emily/src` because it would pull orchestration
and transport responsibilities into the core crate.

## Constraints

- Must remain a sibling crate, not a module inside `emily`
- Must depend on `emily`; `emily` must not depend on it
- Must stay transport-agnostic at the crate boundary
- Must keep dependency growth deliberate
- Must add `src/README.md` and keep it current

## Decision

Create `emily-membrane` as a separate crate with a small initial module tree
and defer real provider adapters until after the local-only boundary is proven.

## Alternatives Rejected

- Fold the membrane into `emily/src`
  - Rejected because it would mix durable core responsibilities with more
    volatile orchestration/runtime concerns.
- Add the membrane directly to the Gestalt app tree
  - Rejected because the membrane should be reusable above Emily, not buried in
    one host's application modules.

## Invariants

- `emily-membrane` depends on `emily`; the reverse dependency is forbidden.
- Membrane-owned code must use Emily's public APIs rather than Emily store
  internals.
- Provider-specific runtime logic is deferred until after the crate boundary
  and local-only path are stable.

## Revisit Triggers

- The membrane layer proves too small to justify a separate crate
- Provider integration requires a smaller contracts-only subcrate
- Emily needs breaking core changes just to support membrane composition

## Dependencies

**Internal:** `emily`  
**External:** None beyond transitive dependencies from `emily` as of 2026-03-08

## Related ADRs

- None identified as of 2026-03-08.
- Reason: the repo does not yet maintain a membrane-specific ADR set.
- Revisit trigger: the first provider adapter or security-boundary design lands.

## Usage Examples

```rust
use emily_membrane::contracts::{
    MembraneTaskRequest,
    MembraneValidationDisposition,
    ValidationEnvelope,
};

let request = MembraneTaskRequest {
    task_id: "task-1".into(),
    episode_id: "episode-1".into(),
    task_text: "Summarize the local context.".into(),
    context_fragments: Vec::new(),
    allow_remote: false,
};

let validation = ValidationEnvelope {
    task_id: request.task_id.clone(),
    disposition: MembraneValidationDisposition::Accepted,
    findings: Vec::new(),
    validated_text: Some("Summarized response".into()),
};

assert_eq!(validation.task_id, request.task_id);
```

## API Consumer Contract

- The public surface is intentionally narrow in this slice.
- `contracts` now exposes typed DTOs for task input, compile results, routing,
  dispatch, validation, and reconstruction.
- `runtime` remains a placeholder until Milestone 1C lands.
- Compatibility policy for this crate will be append-only while the initial
  membrane boundary is stabilized.
- Revisit trigger: the first real runtime facade lands.

## Structured Producer Contract

- The public DTO families in `contracts` are the first structured membrane
  artifacts.
- They are transport-agnostic and local-first in this milestone.
- Revisit trigger: the first provider-facing membrane envelope or leakage-budget
  contract lands.
