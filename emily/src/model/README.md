# emily/src/model

## Purpose

`model/` holds focused model submodules that extend the root `model.rs` contracts without turning one file into the entire Emily domain surface.

## Contents

| File | Description |
| ---- | ----------- |
| `earl.rs` | EARL signal and evaluation contracts for pre-cognitive episode gating |
| `ecgl.rs` | ECGL memory-state and integrity-snapshot contracts |
| `episode.rs` | Episode, trace-link, outcome, and audit contracts for host-agnostic policy inputs |
| `sovereign.rs` | Additive sovereign-preparation contracts for routing, remote episodes, explicit remote state transitions, validation, and audit metadata |

## Problem

Milestone 3 adds new public contracts for episode-oriented Emily behavior. Keeping those types in a dedicated submodule preserves the existing `emily::model` facade while keeping the domain surface reviewable.

## Constraints

- Public model types must stay host-agnostic.
- Record shapes must serialize cleanly for store backends.
- Append-oriented contracts must support idempotent replay.

## Decision

Add focused episode/outcome/audit/EARL/ECGL/sovereign contracts under `model/` and re-export them from `model.rs`.

## Invariants

- Episode and outcome records are additive extensions to the existing text-memory model.
- EARL evaluations are additive extensions to the existing text-memory model.
- ECGL memory states and integrity snapshots are additive extensions to the existing text-memory model.
- Sovereign-preparation contracts are additive extensions to the existing text-memory model.
- Audit records remain immutable event records.
- Request/record types avoid Gestalt-specific UI or transport assumptions.

## Revisit Triggers

- Model contracts expand into separate policy-runtime namespaces.
- Remote-episode contracts become large enough for additional submodules.
- Semantic Membrane IR or transport contracts need a dedicated sibling crate boundary.
