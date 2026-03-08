# Local Reproduction Blueprint

## Objective

Implement an Emily-like local system in two layers:

1. a practical memory and epistemic-integrity controller
2. an optional sovereign-dispatch layer that uses external models without surrendering identity, continuity, or governance

This document remains implementation-oriented. It is not a claim that the full March 2026 architecture is already represented in the repo.

## Scope Clarification

The fastest credible reproduction path is to separate:

- `Memory controller MVP`
  - persistent traces
  - embeddings
  - retrieval
  - `EARL` gating
  - `ECGL` integration / quarantine
  - integrity telemetry
- `Sovereign dispatch extension`
  - membrane IR construction
  - provider routing
  - local rendering / legend mapping
  - output validation
  - prompt-control / retry logic

## Minimal Architecture

### Memory Controller MVP

- API/agent runtime: current orchestrator
- Primary store: SurrealDB
- Graph layer: SurrealDB relation records
- Queue/workers: async retrieval, scoring, consolidation jobs
- Metrics store: per-trace and aggregate epistemic telemetry

### Sovereign Dispatch Extension

- `EARL` gate before remote reasoning
- Membrane compiler for bounded task representations
- Provider router for choosing remote or local reasoning path
- Local validator / renderer for final output assembly
- Audit trail for leakage budgets, routing decisions, and integration outcomes

## Suggested Data Model

### `memory_traces`

- `id`
- `created_at`
- `text`
- `embedding`
- `dimension`
- `epsilon`
- `confidence`
- `outcome_factor`
- `novelty_factor`
- `stability_factor`
- `learning_weight`
- `gate_score`
- `integrated`
- `quarantine_score`

### `memory_edges`

- `from_id`
- `to_id`
- `strength`
- `relation_type`
- `updated_at`

### `system_integrity`

- `ts`
- `ci_value`
- `tau`
- `alerts`

### `remote_episodes` (extension)

- `episode_id`
- `risk_state` (`OK`, `CAUTION`, `REFLEX`)
- `provider`
- `model`
- `routing_reason`
- `membrane_budget`
- `validation_result`
- `integration_result`

### `legend_lifecycle` (extension)

- `episode_id`
- `handle_set_id`
- `key_id`
- `created_at`
- `destroyed_at`

## Framework Roles In The Reproduction

### EMEB

Use `EMEB` in two practical ways:

- as a confidence / bounded-trust vocabulary for stored traces
- as a dedupe and memory-coordination controller for deciding when semantic dedupe is worth the compute

### EARL

Use `EARL` as a pre-cognitive gate over reasoning episodes:

- `OK`: proceed normally
- `CAUTION`: request clarification, tighten reasoning, keep outputs tentative
- `REFLEX`: abort path, do not allow memory-forming integration

### ECGL

Use `ECGL` as the meta-memory layer deciding whether an episode may shape semantic identity.

## Scoring Functions

Use paper defaults first:

- `w1 = 0.35`
- `w2 = 0.35`
- `w3 = 0.20`
- `w4 = 0.10`
- `tau_initial = 0.65`
- `CI_target = 0.88`

Core equations:

- `C(m) = 1 - epsilon / epsilon_max`
- `L(m) = w1*C + w2*O + w3*N + w4*S`
- `G(m) = sigmoid(k*(L - tau))`
- `Q(m) = N * (1 - L)` if blocked

## Processing Flow

### Memory Controller MVP

1. Create trace and embedding.
2. Apply cheap dedupe plus `EMEB`-style duplicate-mass estimation.
3. Retrieve nearest neighbors and graph context.
4. Compute or update `epsilon`, `confidence`, `outcome`, `novelty`, and `stability`.
5. Compute `L`, `G`, and `Q`.
6. Integrate or quarantine.
7. Recompute `CI` and adapt `tau`.

### Sovereign Dispatch Extension

1. Run `EARL` on the episode before remote reasoning.
2. If `OK` or justified `CAUTION`, compile a bounded membrane IR.
3. Dispatch one or more remote models.
4. Validate returned structure locally.
5. Render final answer locally with Emily-controlled context.
6. Pass resulting artifacts through `ECGL` before allowing long-term integration.

## Retrieval And Ranking

Practical rank score:

`rank = sim * (0.4 + 0.6*confidence) * (0.5 + 0.5*learning_weight) * recency_decay`

Then expand top hits through graph edges and re-rank.

For MVP, `confidence` and `learning_weight` can default to `1.0` until the full scoring stack is active.

## Validation Checklist

- Adversarial or confused episodes remain quarantined.
- `CI` stays above the chosen floor under mixed-quality traffic.
- `EARL` catches clearly bad episodes before memory pollution.
- Recovery occurs after restoring high-quality inputs.
- Epsilon-confidence correlation remains within expected band.
- Remote dispatch can be audited without exposing full operation context.
