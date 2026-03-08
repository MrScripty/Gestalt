# emily_seed

## Purpose
Host-side helpers for building and seeding deterministic Emily corpora used by
Gestalt diagnostics and acceptance tests. This directory keeps seeded Emily
fixtures reusable across tests and local tooling without coupling them to UI
components or private Emily store internals.

## Contents
| File/Folder | Description |
| ----------- | ----------- |
| `mod.rs` | Public seed corpus contracts, open/seed helpers, and reports |
| `datasets.rs` | Built-in deterministic corpora for terminal, agent, risk, and semantic retrieval flows |

## Problem
Gestalt needs repeatable Emily data for host-side verification, inspection, and
adoption work. Ad hoc test data would make retrieval and policy behavior hard
to compare across runs.

## Constraints
- Seed data must flow only through Emily public APIs.
- Corpora must stay deterministic so acceptance tests remain stable.
- Dataset labels must remain simple enough for local tooling and CLI use.

## Decision
Keep built-in corpora in one small host module with typed fixture structs and
reports. Tests and diagnostics choose a corpus by stable label and seed it
through Emily's public facade.

## Alternatives Rejected
- Inline fixture data inside each integration test: rejected because corpus
  drift would make host-side adoption checks inconsistent.
- Private store writes for faster seeding: rejected because it would bypass the
  same Emily contracts Gestalt needs to trust in production.

## Invariants
- Built-in corpora must remain deterministic across runs.
- Seeding must use Emily public APIs only.
- Corpus labels are stable identifiers for diagnostics and tests.

## Revisit Triggers
- A second host needs the same corpora from outside Gestalt.
- Corpus size or variety grows enough to justify file-per-dataset splitting.
- Seeding requires non-deterministic data or external fixture imports.

## Dependencies
**Internal:** `emily` public API and model types  
**External:** `serde`, `serde_json`, `thiserror`

## Related ADRs
- None identified as of 2026-03-08.
- Reason: This directory is a host test-fixture helper, not a cross-cutting
  architectural boundary yet.
- Revisit trigger: Another host or external tool consumes these corpora.

## Usage Examples
```rust
let report = gestalt::emily_seed::seed_builtin_corpus(
    &emily_runtime,
    gestalt::emily_seed::SYNTHETIC_SEMANTIC_CONTEXT_DATASET,
)
.await?;
assert_eq!(report.text_objects_seeded, 3);
```

## API Consumer Contract
- Callers provide an already-open `EmilyApi` or a `DatabaseLocator` plus corpus
  label.
- Seeding is append-oriented and deterministic for one corpus label.
- Unknown corpus labels return `EmilySeedError::UnknownCorpus`.
- Reset helpers only clear the requested Emily storage path before opening.

## Structured Producer Contract
- `EmilySeedCorpus` is the stable in-memory corpus shape for built-in datasets.
- `EmilySeedReport` counts what was seeded and uses stable label/stream/episode
  fields for diagnostics.
- Built-in dataset labels are stable strings exported from `mod.rs`.
- If a built-in corpus changes materially, update affected acceptance tests and
  diagnostics in the same change.
