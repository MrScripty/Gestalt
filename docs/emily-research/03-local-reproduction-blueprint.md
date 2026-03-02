# Local Reproduction Blueprint

## Objective

Implement an Emily-like epistemic memory controller in a local agent stack.

## Minimal Architecture

- API/agent runtime: your current orchestrator
- Primary store: SurrealDB (document + graph relations + vector-oriented retrieval)
- Graph layer: SurrealDB relation records (upgrade to Neo4j only if graph workloads outgrow baseline)
- Queue/workers: async scoring and consolidation jobs
- Metrics store: per-trace and aggregate epistemic telemetry

## Suggested Data Model

### `memory_traces` (SurrealDB table)

- `id` (uuid)
- `created_at` (timestamp)
- `text` (text/json)
- `embedding` (vector)
- `dimension` (enum)
- `epsilon` (float)
- `confidence` (float)
- `outcome_factor` (float)
- `novelty_factor` (float)
- `stability_factor` (float)
- `learning_weight` (float)
- `gate_score` (float)
- `integrated` (bool)
- `quarantine_score` (float)

### `memory_edges` (SurrealDB relation table)

- `from_id` (uuid)
- `to_id` (uuid)
- `strength` (float)
- `relation_type` (enum/text)
- `updated_at` (timestamp)

### `system_integrity` (SurrealDB table)

- `ts` (timestamp)
- `ci_value` (float)
- `tau` (float)
- `alerts` (json)

## Scoring Functions

Use paper defaults first:

- `w1=0.35` (confidence)
- `w2=0.35` (outcome)
- `w3=0.20` (novelty)
- `w4=0.10` (stability)
- `tau_initial=0.65`
- `CI_target=0.88`

Core equations:

- `C(m) = 1 - epsilon/epsilon_max`
- `L(m) = w1*C + w2*O + w3*N + w4*S`
- `G(m) = sigmoid(k*(L - tau))` with large `k` (e.g., 100)
- `Q(m) = N * (1 - L)` if blocked

## Pipeline

1. **Write path**
   - Create trace + embedding.
   - Compute novelty from nearest-neighbor similarity.
   - Initialize or update confidence/epsilon stats.
2. **Decision path**
   - Log which traces influenced response.
   - Record downstream outcome and risk weight.
3. **Consolidation job**
   - Recompute EMEB/EARL/ECGL components.
   - Compute `L`, `G`, `Q`.
   - Integrate or quarantine.
4. **Integrity job**
   - Compute `CI` over integrated memories.
   - Auto-adjust `tau` toward CI target.
   - Emit alerts if CI drops quickly or below floor.

## SurrealDB Modeling Notes (High-Level)

- Keep core entities in tables: `memory_traces`, `system_integrity`, `events`.
- Represent graph links with relation records in `memory_edges`.
- Store vectors directly on memory records for similarity retrieval.
- Keep integration state explicit on each memory (`integrated`, `quarantine_score`, `gate_score`).
- Add versioned policy records so threshold/weight changes are auditable.

## Confidence Layer Implementation Notes

Given screenshot evidence (`epsilon-confidence correlation`), track:

- rolling mean/range of confidence
- rolling mean/range of epsilon
- correlation(`epsilon`, `confidence`) per memory cohort

Use this for anomaly detection:

- high confidence + high epsilon => over-certainty risk
- low confidence + low epsilon => under-confidence inefficiency

## Retrieval Ranking

Practical rank score:

`rank = sim * (0.4 + 0.6*confidence) * (0.5 + 0.5*learning_weight) * recency_decay`

Then expand top hits through graph edges and re-rank.

## Validation Checklist

- Adversarial injection remains quarantined.
- CI remains above chosen floor under mixed-quality traffic.
- Recovery occurs after restoring high-quality inputs.
- Epsilon-confidence correlation remains within expected band.
