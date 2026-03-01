# Emily Crate

## Purpose
Reusable, host-agnostic memory runtime that ingests arbitrary text objects and
provides context retrieval and history paging against addressable databases.

## Public API
- `EmilyApi`: open/switch/close database, ingest text, query context, page history
- Generic DTOs in `model.rs` (no Gestalt-specific types)

## Internal Modules
- `api`: transport-agnostic public contracts
- `model`: canonical data structures
- `store`: storage traits + Surreal-backed implementation
- `runtime`: default in-process API implementation
- `error`: typed error surface
