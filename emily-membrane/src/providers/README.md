# emily-membrane/src/providers

## Purpose

`providers` owns the membrane crate's remote-dispatch adapter boundary. This
directory exists so provider-specific transport integrations can grow behind a
stable membrane-owned trait without leaking workflow-service or host-specific
types into the rest of the crate.

## Contents

| File/Folder | Description |
| ----------- | ----------- |
| `mod.rs` | Provider trait, registry contracts, provider request/result DTOs, and provider error surface |
| `pantograph.rs` | Feature-gated one-shot Pantograph workflow adapter |

## Problem

The membrane crate now has a local-only runtime path, but it still lacks a
remote adapter boundary. Without a membrane-owned provider contract, the first
real transport integration would either couple the runtime to one backend or
push provider concerns down into `emily`.

## Constraints

- Provider contracts must remain membrane-owned and transport-agnostic.
- No type in this directory may depend on Gestalt UI or app modules.
- Provider request/result DTOs must stay append-only.
- This first slice must avoid transport session ownership or retry loops.

## Decision

Add a narrow public provider trait plus provider-owned dispatch DTOs before the
first real transport adapter. Keep the boundary generic enough for Pantograph
or another provider implementation later.

## Alternatives Rejected

- Put provider traits in `runtime.rs`
  - Rejected because provider concerns deserve an explicit boundary of their
    own.
- Reuse external workflow-service types directly
  - Rejected because the membrane crate should own its remote-dispatch
    contracts.

## Invariants

- `providers` defines provider-facing contracts, not Emily persistence records.
- Provider implementations must be injectable into the membrane runtime through
  a host-supplied provider or provider registry rather than globally
  discovered.
- Provider DTOs stay generic enough that Pantograph remains an adapter, not the
  model for the whole crate.

## Revisit Triggers

- The first real adapter requires submodules such as `pantograph/`.
- Provider lifecycle logic grows enough to justify splitting trait and DTOs.
- Leakage-budget or reconstruction-specific contracts need their own modules.

## Dependencies

**Internal:** `contracts`  
**External:** `async-trait`, `serde`, `serde_json`

## Related ADRs

- None identified as of 2026-03-08.
- Reason: the provider boundary is still the first additive slice.
- Revisit trigger: the first real transport adapter lands.

## Usage Examples

```rust
use async_trait::async_trait;
use emily_membrane::providers::{
    MembraneProvider, MembraneProviderError, ProviderDispatchRequest,
    ProviderDispatchResult, ProviderDispatchStatus,
};

struct ExampleProvider;

#[async_trait]
impl MembraneProvider for ExampleProvider {
    fn provider_id(&self) -> &str {
        "example"
    }

    async fn dispatch(
        &self,
        request: ProviderDispatchRequest,
    ) -> Result<ProviderDispatchResult, MembraneProviderError> {
        Ok(ProviderDispatchResult {
            provider_request_id: request.provider_request_id,
            provider_id: self.provider_id().to_string(),
            status: ProviderDispatchStatus::Completed,
            output_text: "remote result".to_string(),
            metadata: serde_json::json!({}),
        })
    }
}
```

## API Consumer Contract

- `MembraneProvider` is the membrane-owned remote adapter trait.
- `MembraneProviderRegistry` is the membrane-owned provider lookup boundary for
  host-supplied routing resolution.
- `InMemoryProviderRegistry` is the default registry for request-scoped host
  injection.
- `RegisteredProviderTarget` carries the registry metadata used for target
  selection before provider dispatch.
- `ProviderDispatchRequest` and `ProviderDispatchResult` are append-only DTOs
  for the first remote slices.
- The optional `pantograph` feature adds a one-shot workflow adapter without
  changing the provider trait.
- Revisit trigger: the first real adapter needs streaming, cancellation, or
  multi-step execution hooks.

## Structured Producer Contract

- `mod.rs` publishes serde-backed provider DTOs for remote dispatch.
- These DTOs are intentionally narrow and transport-agnostic.
- Revisit trigger: the first transport adapter needs richer envelopes or
  leakage-budget accounting.
