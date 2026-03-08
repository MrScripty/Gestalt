# emily-membrane/src

## Purpose

`emily-membrane/src` holds the membrane crate's future boundary contracts and
runtime orchestration modules. This source tree exists to keep sovereign
dispatch responsibilities separate from the `emily` core crate while still
allowing the membrane to depend on Emily's public APIs.

## Contents

| File/Folder | Description |
| ----------- | ----------- |
| `lib.rs` | Crate exports and top-level membrane boundary |
| `contracts.rs` | Executable membrane DTOs for task, compile, route, dispatch, validation, and reconstruction |
| `runtime.rs` | Minimal local-only membrane runtime facade with an internal deterministic adapter |

## Problem

The Emily architecture needs a layer that can shape bounded work, choose local
or remote paths, and orchestrate validation/reconstruction. That work should
not accumulate in the `emily` core crate because it changes faster and carries
different runtime concerns.

## Constraints

- The crate must depend on `emily` without creating a reverse dependency.
- All future `src/` subdirectories must include `README.md`.
- Public contracts must remain transport-agnostic and host-agnostic.
- Runtime ownership rules must stay explicit before background work is added.

## Decision

Start with a narrow source tree containing only `contracts` and `runtime`
modules. Defer subdirectories such as `compiler/`, `router/`, or `providers/`
until those responsibilities become real in code.

## Alternatives Rejected

- Create a large module tree immediately
  - Rejected because it would add speculative structure before real contracts
    exist.
- Keep everything in `lib.rs`
  - Rejected because the crate boundary needs named modules from the beginning
    for discoverability and future growth.

## Invariants

- `contracts.rs` owns boundary DTOs, not provider implementations.
- `runtime.rs` owns membrane orchestration entrypoints, not Emily persistence.
- No module in this tree may depend on Gestalt UI or application modules.

## Revisit Triggers

- The first contract set becomes large enough for a `contracts/` directory.
- Runtime orchestration gains enough behavior to justify `runtime/` submodules.
- Provider adapters become real and need a dedicated module tree.

## Dependencies

**Internal:** `emily`  
**External:** None directly in this skeleton slice

## Related ADRs

- None identified as of 2026-03-08.
- Reason: the source tree is only a skeleton in this slice.
- Revisit trigger: the first provider adapter or runtime lifecycle design lands.

## Usage Examples

```rust
use std::sync::Arc;

use emily::EmilyApi;
use emily_membrane::contracts::MembraneTaskRequest;
use emily_membrane::runtime::MembraneRuntime;

async fn run_local(api: Arc<dyn EmilyApi>) {
    let runtime = MembraneRuntime::new(api);
    let compiled = runtime
        .compile(MembraneTaskRequest {
            task_id: "task-1".into(),
            episode_id: "episode-1".into(),
            task_text: "Local-only task".into(),
            context_fragments: Vec::new(),
            allow_remote: false,
        })
        .await
        .expect("compile");
    let route = runtime.route(&compiled).await.expect("route");
    let dispatch = runtime.dispatch_local(&compiled, &route).await.expect("dispatch");
    let validation = runtime.validate(&dispatch).await.expect("validate");
    let result = runtime.reconstruct(&validation).await.expect("reconstruct");
    assert!(result.output_text.starts_with("LOCAL: "));
}
```

## API Consumer Contract

- `contracts.rs` now exposes the first stable DTO families for Milestone 1.
- `runtime.rs` now exposes the first local-only membrane facade above `EmilyApi`.
- Revisit trigger: the first provider-backed runtime path lands.

## Structured Producer Contract

- `contracts.rs` publishes the first structured membrane artifacts through
  serde-backed DTOs.
- Those artifacts are intentionally narrow and local-first in this milestone.
- Revisit trigger: the first provider-facing or leakage-budget contract lands.
