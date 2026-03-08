# emily-membrane/src/runtime

## Purpose

`runtime` holds membrane runtime submodules that would otherwise bloat
`runtime.rs`. This directory exists so local-only orchestration can remain in
the top-level runtime facade while newer remote-path logic stays reviewable in
focused files.

## Contents

| File/Folder | Description |
| ----------- | ----------- |
| `policy.rs` | Deterministic routing-policy evaluation over registered membrane targets |
| `remote.rs` | Remote execution helpers and provider-registry-backed runtime methods |

## Problem

The membrane runtime started as a single-file local-only facade. The first
remote execution path adds enough logic that keeping everything in one file
would push the runtime beyond the repo's review-size target.

## Constraints

- Runtime submodules must stay beneath the public `MembraneRuntime` facade.
- No module in this directory may talk to Emily store internals directly.
- Remote logic must continue to use Emily's public API only.

## Decision

Split the first remote runtime path into `remote.rs` while keeping the public
runtime type in `runtime.rs`.

## Alternatives Rejected

- Keep all remote logic in `runtime.rs`
  - Rejected because it would push the runtime beyond the review threshold.
- Create a second top-level runtime file unrelated to `runtime.rs`
  - Rejected because the membrane runtime still needs one public facade.

## Invariants

- `runtime.rs` owns the public runtime type and core local path.
- `policy.rs` owns deterministic routing-policy evaluation for the first
  sovereign routing slice.
- `remote.rs` owns provider-registry-backed remote execution helpers.
- Remote execution remains synchronous and request-scoped in this milestone.

## Revisit Triggers

- Remote runtime logic grows enough to justify `remote/` submodules.
- Cancellation, retry, or fanout logic becomes real.
- Local-only helpers also need extraction for size or clarity.

## Dependencies

**Internal:** `contracts`, `providers`, `runtime.rs`  
**External:** None beyond the crate's top-level dependencies

## Related ADRs

- None identified as of 2026-03-08.
- Reason: this is a size and decomposition split, not a new architecture layer.
- Revisit trigger: provider transport lifecycle becomes materially more complex.

## Usage Examples

```rust
// Remote runtime methods are exposed on `MembraneRuntime`; this directory only
// holds the internal decomposition.
```

## API Consumer Contract

- Consumers continue to use `emily_membrane::runtime::MembraneRuntime`.
- The public runtime can be constructed with a single provider or a host-owned
  provider registry.
- Registry-backed runtimes can now resolve `ProviderTarget` values from
  `RemoteRoutingPreference` instead of requiring the host to prebuild targets.
- Registry-backed runtimes can now evaluate typed routing-policy requests
  before provider dispatch.
- Policy evaluation now consumes Emily episode state plus the latest durable
  `EARL` evaluation before provider scoring.
- This directory does not expose a separate public facade.
- Revisit trigger: a separate runtime builder or worker owner becomes necessary.

## Structured Producer Contract

- `remote.rs` does not publish standalone structured artifacts.
- Structured artifacts remain defined in `contracts` and `providers`.
- Revisit trigger: remote runtime lifecycle snapshots need their own DTO family.
