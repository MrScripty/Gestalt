# Emily Crate

## Purpose
Reusable, host-agnostic memory runtime that ingests arbitrary text objects and
provides context retrieval and history paging against addressable databases.

## Public API
- `EmilyApi`: open/switch/close database, ingest text, query context, page history
- Generic DTOs in `model.rs` (no Gestalt-specific types)
- Optional Pantograph workflow-session embedding provider via feature `pantograph`

## Internal Modules
- `api`: transport-agnostic public contracts
- `model`: canonical data structures
- `store`: storage traits + Surreal-backed implementation for text objects and vectors
- `runtime`: default in-process API implementation
- `inference`: embedding provider contracts + Pantograph workflow-session client adapters
- `error`: typed error surface
