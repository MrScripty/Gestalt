# emily-membrane/src/contracts

## Purpose

`contracts` owns the membrane crate's executable boundary DTOs. This directory
now exists because Milestone 6 adds typed membrane IR and the contract surface
is large enough to justify a focused submodule instead of continuing to grow
one file.

## Contents

| File | Description |
| ---- | ----------- |
| `ir.rs` | Typed membrane IR contracts and render-mode metadata |
| `validation.rs` | Typed membrane validation contracts and category assessments |

## Problem

The membrane runtime can already compile, route, dispatch, validate, and
reconstruct bounded tasks, but until Milestone 6 the only durable compile
representation was a rendered prompt string. The Emily research expects a
typed membrane representation that remains meaningful before transport
rendering.

## Constraints

- DTOs in this directory must remain transport-agnostic.
- IR contracts must be append-only.
- Validation contracts must stay membrane-owned and modest in their claims.
- Provider adapters must translate from membrane IR rather than inventing their
  own primary task representation.

## Decision

Add a dedicated IR submodule while keeping the public `contracts` module
surface stable.

## Invariants

- The typed IR is the primary compile representation.
- Validation assessments are a first deterministic slice toward richer local
  membrane validation, not a claim of full `ECCR`.
- Rendered prompt text remains an adapter-oriented view derived from the IR.
- No type in this directory may depend on Emily store internals or Gestalt app
  modules.

## Revisit Triggers

- Retry or reconstruction contracts need additional focused directories.

## Dependencies

**Internal:** `contracts.rs`  
**External:** `serde`
