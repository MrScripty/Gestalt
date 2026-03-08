# emily/src/inference/pantograph

## Purpose

This directory contains the Pantograph-specific embedding provider implementation used by Emily when the `pantograph` feature is enabled. The split keeps workflow-session transport logic, provider runtime behavior, and tests reviewable without bloating the top-level inference module.

## Contents

| File/Folder | Description |
| ----------- | ----------- |
| `client.rs` | Workflow-session bindings and the service-backed client adapter. |
| `provider.rs` | Pantograph embedding provider runtime and vector extraction logic. |
| `tests.rs` | Feature-gated provider tests with mocked workflow-session behavior. |

## Problem

Pantograph integration is the largest optional inference implementation in the crate. Without a dedicated subdirectory, one file would mix client contracts, provider state management, and test fixtures past the repo's decomposition threshold.

## Constraints

- All Pantograph code must remain feature-gated.
- Provider logic must continue to satisfy the `EmbeddingProvider` contract.
- Tests must stay local to the integration without pulling Gestalt runtime concerns into the crate.

## Decision

Split Pantograph support into client, provider, and test files under one subdirectory while preserving the existing public re-exports.

## Alternatives Rejected

- Keep Pantograph in one file: rejected because the file exceeded the size threshold and mixed multiple responsibilities.
- Move tests outside the crate: rejected because the tests are tightly coupled to the provider contract and mocked workflow-session behavior.

## Invariants

- Public Pantograph re-exports remain stable through `emily::inference`.
- Provider logic depends on workflow-session contracts, not Gestalt-specific runtime APIs.
- Tests exercise the public provider behavior, not internal-only shortcuts.

## Revisit Triggers

- A second Pantograph-specific provider or client strategy appears.
- Pantograph contract changes require a new compatibility layer.

## Dependencies

**Internal:** `emily/src/inference.rs`, `crate::error`, `crate::model`  
**External:** `async-trait`, `tokio`, `pantograph-workflow-service`

## Related ADRs

- None identified as of 2026-03-08.
- Reason: this is an internal decomposition within an existing optional integration.
- Revisit trigger: Pantograph support changes the public feature or provider contract.

## Usage Examples

```rust
use emily::{
    PantographEmbeddingProvider,
    PantographWorkflowBinding,
    PantographWorkflowEmbeddingConfig,
};

# let _ = (
#     std::any::type_name::<PantographEmbeddingProvider>(),
#     std::any::type_name::<PantographWorkflowBinding>(),
#     std::any::type_name::<PantographWorkflowEmbeddingConfig>(),
# );
```

## API Consumer Contract

- Consumers should use the re-exported Pantograph types from `emily`, not internal module paths.
- The provider remains available only when the `pantograph` feature is enabled.
- Provider lifecycle remains async and must be shut down by the host when no longer needed.

## Structured Producer Contract

- `provider.rs` emits embedding vectors and status snapshots through existing Emily contracts.
- `tests.rs` produces no persisted artifacts.
- Compatibility for vector contents remains governed by provider configuration and Emily model types.
