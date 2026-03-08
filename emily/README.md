# Emily Crate

## Purpose

Reusable, host-agnostic Emily memory runtime that ingests arbitrary text objects and provides context retrieval, history paging, and vectorization maintenance against addressable databases.

Current scope is the memory and embedding-integrity core. This crate does not yet implement the broader March 2026 Emily sovereign-cognition layer such as `Semantic Membrane`, provider routing, local legend mapping, or multi-model dispatch orchestration.

## Public API

- `EmilyApi`: open/switch/close database, ingest text, query context, page history
- Episode contract surface: create episodes, read episodes, link traces, record outcomes, append audits
- Sovereign record surface: record routing decisions, remote episodes, explicit remote state transitions, validation outcomes, and structured sovereign audits
- Sovereign query surface: read routing decisions, remote episodes, validation outcomes, and sovereign audits by episode
- `EARL` control surface: evaluate episode risk and receive `OK / CAUTION / REFLEX` results
- Integrity surface: read the latest durable cognitive-integrity snapshot
- Retrieval policy surface: read/update `MemoryPolicy`
- Vectorization control surface: config updates, status, backfill/revectorize jobs, cancellation
- Sovereign-preparation model surface: remote episode, routing, validation, and audit metadata DTOs
- Generic DTOs in `model.rs` (no Gestalt-specific types)
- Optional Pantograph workflow-session embedding provider via feature `pantograph`

## Current Architectural Position

This crate currently corresponds to the Emily memory subsystem described in the research notes:

- persistent text-object store
- vector store and retrieval
- scoring-related fields on stored objects
- runtime health and vectorization operations

It is best treated as the local persistence / retrieval / embedding core that a broader Emily architecture could build on later.

Current policy fields on stored objects are provisional. Until active `EARL` and
`ECGL` runtimes exist, the crate should not imply that stored confidence,
learning, or integration values are the result of a real policy engine.

Episode, outcome, audit, `EARL` evaluation, and integrity snapshot records are now part of the reusable crate boundary.
Those persisted artifacts are additive extensions to existing Emily storage and
do not require breaking changes for databases that only contain text/vector data.

The current `EARL` runtime is a deterministic first slice. It gives hosts a
typed pre-cognitive gate and durable decision trail without yet claiming the
full learned manifold or Mahalanobis implementation described in the papers.

The current `ECGL` runtime is also a deterministic first slice. It runs
synchronously on outcome ingestion, assigns explicit memory states, updates
text-level scoring fields, and persists integrity snapshots without yet adding
background workers or the full adaptive policy stack from the papers.

The crate now includes durable sovereign-support records for remote episodes,
routing decisions, validation outcomes, and structured sovereign audit metadata.
Those contracts prepare later sovereign Emily work without committing the core
crate to one membrane or transport runtime.

Routing, remote-episode, and validation writes now generate deterministic
sovereign audit records automatically. Hosts can still append explicit
sovereign audits for additional boundary events or annotations. Automatic
generation is intentionally limited to write-side lifecycle events for now;
read/query access does not emit sovereign audits until a real boundary-crossing
host flow requires it. Future sovereign record families should inherit
write-side automatic audit generation by default unless they are explicitly
documented as exempt.

The sovereign runtime now also applies bounded lifecycle policy inside the
crate: routes marked `LocalOnly` or `Rejected` cannot be used for remote
dispatch, rejected routing decisions block the host episode, and validation
outcomes reconcile remote-episode terminal state plus episode caution/blocking
without introducing provider-specific transport logic.
Hosts can also record explicit terminal remote-episode transitions for failure,
cancel, rejection, or success when validation is not the source of closure.

## Host Responsibilities

Host applications remain responsible for:

- choosing stream IDs, source kinds, and host-specific metadata
- deciding when to open databases and when to rotate them
- mapping host events into Emily episodes, trace links, and outcomes
- deciding when EARL or ECGL results should influence host behavior
- keeping UI, transport, and provider-routing concerns outside this crate

## Boundary Decision

`Semantic Membrane` IR, leakage-accounting internals, and provider transport
contracts remain outside `emily` for now. The core crate keeps only the stable,
host-agnostic records that higher layers can persist, exchange, or audit across
hosts.

## Internal Modules

- `api`: transport-agnostic public contracts
- `model`: canonical data structures
- `store`: storage traits + Surreal-backed implementation for text, episode, outcome, and audit records
- `runtime`: default in-process API implementation, episode lifecycle writes, and background vectorization jobs
- `inference`: embedding provider contracts + Pantograph workflow-session client adapters
- `error`: typed error surface

## Revisit Triggers

- Emily expands from memory-runtime scope into sovereign-dispatch orchestration
- `EARL` / `ECGL` controls move from stored fields to active runtime policy
- Membrane-bound remote reasoning or audit surfaces are added
- Semantic Membrane IR or provider transport contracts need to become durable crate contracts
