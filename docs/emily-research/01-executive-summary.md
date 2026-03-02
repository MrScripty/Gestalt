# Executive Summary

## Core Finding

Emily appears to use a 4-part cognitive scoring stack for memory consolidation and action reliability:

1. `EMEB` (Epsilon): uncertainty/confidence-bound quality of memory traces
2. `EARL`: outcome-weighted learning and risk-informed performance
3. `ECGL` (Stability): epistemic confidence-gated integration into semantic identity
4. `Confidence`: decision certainty, tracked explicitly and correlated with EMEB epsilon

The first three are publicly described. The fourth (`Confidence`) is visible in screenshots and aligned with ECGL paper mechanics.

## What The System Is Doing

- Stores memory as embeddings (vector memory), plus relationship graph links.
- Retrieves by semantic similarity, then re-ranks by epistemic quality signals.
- Uses gate logic to decide whether memory becomes identity-shaping semantic knowledge or is quarantined.
- Continuously recalibrates trust via a global integrity metric.

## Why This Matters

This is not only retrieval optimization; it is a control layer that decides what information is allowed to shape long-term reasoning.

## Practical Reproduction Feasibility

A local reproduction is feasible with:

- SurrealDB as primary multi-model memory store
- Graph relations in SurrealDB (or Neo4j if graph scale demands it)
- Async workers (Tokio channels/NATS/Redis style queueing)
- Per-memory scoring pipeline implementing EMEB/EARL/ECGL + confidence correlation metrics

## Limits

- Public pages were partially client-rendered and not fully machine-readable in crawler output.
- Exact production constants and training logic remain private; formulas and default parameters in the ECGL paper are sufficient for a strong baseline implementation.
