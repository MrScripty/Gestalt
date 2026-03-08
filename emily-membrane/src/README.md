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
| `contracts.rs` | Placeholder module for future executable membrane contracts |
| `runtime.rs` | Placeholder module for the future membrane runtime facade |

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
use emily_membrane::{contracts, runtime};

assert_eq!(contracts::MODULE_NAME, "contracts");
assert_eq!(runtime::MODULE_NAME, "runtime");
```

## API Consumer Contract

- No stable runtime API is exposed yet beyond the module boundary.
- Consumers should treat this source tree as pre-contract until Milestone 1B and
  1C land.
- Revisit trigger: the first public membrane facade or DTO lands.

## Structured Producer Contract

- None identified as of 2026-03-08.
- Reason: the skeleton modules do not yet publish structured artifacts.
- Revisit trigger: the first membrane DTO or validation envelope lands.
