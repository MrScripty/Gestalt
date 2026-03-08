# emily/src/model

## Purpose

`model/` holds focused model submodules that extend the root `model.rs` contracts without turning one file into the entire Emily domain surface.

## Contents

| File | Description |
| ---- | ----------- |
| `episode.rs` | Episode, trace-link, outcome, and audit contracts for host-agnostic policy inputs |

## Problem

Milestone 3 adds new public contracts for episode-oriented Emily behavior. Keeping those types in a dedicated submodule preserves the existing `emily::model` facade while keeping the domain surface reviewable.

## Constraints

- Public model types must stay host-agnostic.
- Record shapes must serialize cleanly for store backends.
- Append-oriented contracts must support idempotent replay.

## Decision

Add focused episode/outcome/audit contracts under `model/` and re-export them from `model.rs`.

## Invariants

- Episode and outcome records are additive extensions to the existing text-memory model.
- Audit records remain immutable event records.
- Request/record types avoid Gestalt-specific UI or transport assumptions.

## Revisit Triggers

- Model contracts expand into separate policy-runtime namespaces.
- Remote-episode and membrane contracts become large enough for additional submodules.
