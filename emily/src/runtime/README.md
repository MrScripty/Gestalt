# emily/src/runtime

## Purpose

This directory contains the runtime-specific implementation slices for the Emily in-process runtime. The split exists to keep runtime API entrypoints, vectorization job orchestration, and runtime tests discoverable without letting one file become the de facto home for every async concern in the crate.

## Contents

| File/Folder | Description |
| ----------- | ----------- |
| `retrieval.rs` | Semantic retrieval, ranking, provenance, and semantic-edge linking logic. |
| `vectorization.rs` | Background job orchestration for backfill and revectorize runs. |
| `tests.rs` | Runtime-focused integration-style tests using in-memory test doubles. |

## Problem

`emily/src/runtime.rs` owns host-facing runtime behavior while runtime subdomains own retrieval and maintenance workflows. Without a dedicated split, semantic retrieval, vectorization jobs, and runtime tests would continue to expand a single file past the repo's decomposition threshold.

## Constraints

- Runtime behavior must remain behind the `EmilyApi` facade.
- Background work must keep explicit ownership and shutdown behavior.
- Tests in this area need realistic async behavior without depending on Gestalt.

## Decision

Keep `runtime.rs` as the public runtime entrypoint and move focused subdomains under `runtime/` for retrieval, job orchestration, and tests.

## Alternatives Rejected

- Leave all runtime code in one file: rejected because `runtime.rs` already exceeded the file-size threshold and mixed public API logic with worker implementation details.
- Split runtime into a new public top-level crate module tree: rejected for now because the current facade remains stable and only internal decomposition is needed.

## Invariants

- `EmilyRuntime` remains the public runtime type.
- Retrieval logic remains internal to the runtime implementation rather than a host-facing helper API.
- Vectorization job logic remains an internal implementation detail.
- Runtime tests continue to exercise the public facade rather than private helpers only.

## Revisit Triggers

- Another runtime concern grows large enough to deserve its own submodule.
- Runtime task ownership becomes complex enough to justify a dedicated runtime state-machine module.

## Dependencies

**Internal:** `emily/src/runtime.rs`, `crate::model`, `crate::store`  
**External:** `tokio`, `async-trait`

## Related ADRs

- None identified as of 2026-03-08.
- Reason: this is an internal decomposition change within an existing crate boundary.
- Revisit trigger: runtime decomposition changes the public facade or package boundary.

## Usage Examples

```rust
use emily::runtime::EmilyRuntime;
use emily::store::surreal::SurrealEmilyStore;
use std::sync::Arc;

let runtime = EmilyRuntime::new(Arc::new(SurrealEmilyStore::new()));
# let _ = runtime;
```

## API Consumer Contract

- Consumers continue to use `EmilyRuntime` through `EmilyApi`.
- Submodules in this directory are internal and should not be imported by host code.
- Runtime lifecycle remains owned by the caller that constructs and shuts down `EmilyRuntime`.
- Background vectorization work must remain cancellable and observable through the public facade.

## Structured Producer Contract

- `tests.rs` produces no persisted artifacts.
- `retrieval.rs` produces semantic edges and context packets through existing Emily contracts.
- `vectorization.rs` updates runtime job snapshots and vector records through existing crate contracts.
- Compatibility expectations for those records remain defined by `emily/src/model.rs` and store implementations.
