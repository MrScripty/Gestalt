# emily/src/inference

## Purpose

This directory contains Emily's embedding-provider abstractions and provider-specific adapters. The directory boundary exists so host-independent embedding contracts can stay small while heavier provider integrations are isolated behind feature gates.

## Contents

| File/Folder | Description |
| ----------- | ----------- |
| `pantograph.rs` | Pantograph feature entrypoint and public re-exports. |
| `pantograph/` | Pantograph client, provider, and tests. |

## Problem

Embedding support is optional but central to Emily retrieval. The crate needs a narrow provider contract for the common path and a separate location for feature-gated integrations that would otherwise dominate the main inference module.

## Constraints

- Provider contracts must remain host-agnostic.
- Optional integrations must compile away cleanly when the feature is disabled.
- Provider implementations must not force heavy dependencies onto non-feature builds.

## Decision

Keep the `EmbeddingProvider` trait and no-op provider at the top level while moving Pantograph-specific implementation into `inference/`.

## Alternatives Rejected

- Keep all provider code in one file: rejected because the Pantograph integration was large enough to obscure the provider facade.
- Move Pantograph support into Gestalt: rejected because the embedding provider contract is reusable across hosts.

## Invariants

- `EmbeddingProvider` remains the crate-wide provider contract.
- The Pantograph provider stays feature-gated.
- Hosts can disable Pantograph support without affecting the no-op or trait path.

## Revisit Triggers

- A second provider implementation is added.
- Pantograph integration grows enough to require a nested submodule split.

## Dependencies

**Internal:** `emily/src/inference.rs`, `crate::error`, `crate::model`  
**External:** `async-trait`, `tokio`, `pantograph-workflow-service` (feature-gated)

## Related ADRs

- None identified as of 2026-03-08.
- Reason: current changes preserve the existing public provider facade.
- Revisit trigger: provider architecture changes public contracts or package boundaries.

## Usage Examples

```rust
use emily::EmbeddingProvider;
use emily::NoopEmbeddingProvider;

let provider = NoopEmbeddingProvider;
# let _ = provider;
```

## API Consumer Contract

- Hosts should depend on `EmbeddingProvider`, not provider-internal helpers.
- Pantograph types are available only when the `pantograph` feature is enabled.
- Provider shutdown remains best-effort and async.

## Structured Producer Contract

- Pantograph providers produce `EmbeddingProviderStatus` snapshots through the existing runtime status contract.
- No standalone persisted artifacts are emitted from this directory.
