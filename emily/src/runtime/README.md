# emily/src/runtime

## Purpose

This directory contains the runtime-specific implementation slices for the Emily in-process runtime. The split exists to keep runtime API entrypoints, vectorization job orchestration, and runtime tests discoverable without letting one file become the de facto home for every async concern in the crate.

## Contents

| File/Folder | Description |
| ----------- | ----------- |
| `ecgl.rs` | Synchronous ECGL evaluation, explicit memory-state transitions, and integrity snapshots. |
| `ecgl_tests.rs` | ECGL state-transition and integrity-snapshot tests through the public runtime facade. |
| `earl.rs` | Deterministic EARL evaluator, projection updates, and durable audit writes. |
| `earl_tests.rs` | EARL unit and acceptance tests through the public runtime facade. |
| `episodes.rs` | Episode, outcome, and audit write-path validation and idempotency logic. |
| `lifecycle.rs` | Runtime construction, database lifecycle, ingest shaping, and vectorization state helpers. |
| `retrieval.rs` | Semantic retrieval, ranking, provenance, and semantic-edge linking logic. |
| `sovereign.rs` | Sovereign-record write paths for routing, remote episodes, validation, and structured audits. |
| `sovereign_tests.rs` | Runtime acceptance tests for sovereign-record persistence and validation. |
| `test_support.rs` | Shared async test doubles and fixtures for runtime tests. |
| `episode_tests.rs` | Runtime acceptance tests for persisted episode flows. |
| `vectorization.rs` | Background job orchestration for backfill and revectorize runs. |
| `tests.rs` | Runtime-focused integration-style tests using in-memory test doubles. |

## Problem

`emily/src/runtime.rs` owns host-facing runtime behavior while runtime subdomains own retrieval and maintenance workflows. Without a dedicated split, semantic retrieval, vectorization jobs, and runtime tests would continue to expand a single file past the repo's decomposition threshold.

## Constraints

- Runtime behavior must remain behind the `EmilyApi` facade.
- Background work must keep explicit ownership and shutdown behavior.
- Tests in this area need realistic async behavior without depending on Gestalt.

## Decision

Keep `runtime.rs` as the public runtime entrypoint and move focused subdomains under `runtime/` for retrieval, EARL gating, ECGL evaluation, episode/outcome persistence rules, job orchestration, and tests.

## Alternatives Rejected

- Leave all runtime code in one file: rejected because `runtime.rs` already exceeded the file-size threshold and mixed public API logic with worker implementation details.
- Split runtime into a new public top-level crate module tree: rejected for now because the current facade remains stable and only internal decomposition is needed.

## Invariants

- `EmilyRuntime` remains the public runtime type.
- EARL evaluation logic remains internal to the runtime implementation rather than a host-facing helper API.
- ECGL evaluation logic remains internal to the runtime implementation rather than a host-facing helper API.
- Retrieval logic remains internal to the runtime implementation rather than a host-facing helper API.
- Episode, outcome, and audit writes remain host-agnostic runtime behavior behind `EmilyApi`.
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

- `ecgl_tests.rs` validates integration, quarantine, snapshot persistence, and recovery-sensitive ECGL behavior through the public runtime facade.
- `earl_tests.rs` validates `OK / CAUTION / REFLEX` behavior and blocked-episode enforcement through the public runtime facade.
- `tests.rs` and `test_support.rs` produce no persisted artifacts.
- `episode_tests.rs` validates episode, outcome, and audit persistence through the public runtime facade.
- `sovereign_tests.rs` validates route, remote-episode, validation-outcome, and sovereign-audit flows through the public runtime facade.
- `retrieval.rs` produces semantic edges and context packets through existing Emily contracts.
- `vectorization.rs` updates runtime job snapshots and vector records through existing crate contracts.
- `episodes.rs` produces episode records, trace links, outcome records, and audit records through existing crate contracts.
- `earl.rs` produces EARL evaluations, guarded episode states, and durable EARL audit records through existing crate contracts.
- `ecgl.rs` produces explicit memory states on text objects and durable integrity snapshots through existing crate contracts.
- `sovereign.rs` produces routing decisions, remote episodes, validation outcomes, and deterministic sovereign audit records through existing crate contracts.
- Compatibility expectations for those records remain defined by `emily/src/model.rs` and store implementations.
