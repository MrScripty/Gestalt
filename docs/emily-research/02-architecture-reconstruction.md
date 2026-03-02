# Architecture Reconstruction

## System View

Reconstructed Emily stack combines:

- Cognitive processing loop (interpret/analyze/plan/memory/replay/synthesis)
- Multi-tier memory model (working, episodic/session, semantic, identity archive)
- Epistemic control stack (`EMEB`, `EARL`, `ECGL`, `Confidence`)
- Vector + graph representation for memory structure and retrieval context

## Memory Representation

Each memory trace likely includes:

- `embedding`: semantic vector
- `metadata`: timestamp, source, context/session
- `dimension`: category (self, thinking, doing, feeling, world, values, time, other)
- `learning_weight`: dynamic importance
- `epsilon`: uncertainty/error-bound value
- `confidence`: decision certainty value
- `outcome_signals`: post-decision success/failure history
- `stability`: decay consistency / corruption-resistance
- `integration_state`: quarantined vs integrated

Graph observations from screenshots:

- Node size corresponds to learning weight
- Node color corresponds to primary dimension
- Distance encodes age/importance
- Edges show top relationships

## The 4-Part Metric Stack

### 1) EMEB (Epsilon / Uncertainty Measure)

Purpose: quantify memory confidence bounds over time.

- Lower epsilon => tighter confidence => higher trust.
- Feeds directly into ECGL confidence factor.
- Screenshot evidence includes `emeb.epsilon_range`.

### 2) EARL (Outcome-Weighted Learning)

Purpose: measure how decisions informed by memory perform under risk.

- High-stakes successful outcomes should contribute more.
- Screenshot evidence includes `earl.outcome_responsiveness`.

### 3) ECGL (Stability / Knowledge Consolidation)

Purpose: gate identity-forming integration of memory.

ECGL paper defines learning weight:

`L(m) = w1*C(m) + w2*O(m) + w3*N(m) + w4*S(m)`

Where:

- `C(m)`: confidence factor (from EMEB epsilon)
- `O(m)`: outcome factor (from EARL-style outcomes/risk)
- `N(m)`: novelty factor
- `S(m)`: stability factor (expected vs observed epsilon decay)

Gate:

- Hard form: `G(m) = H(L(m) - tau)`
- Production form: sigmoid gate for soft transition around threshold

Integration behavior:

- If gated in: update semantic model proportionally to `L(m)`
- If gated out: quarantine memory, preserve for recall, re-evaluate later

### 4) Confidence (Decision Certainty)

Purpose: explicit certainty score for decision reliability.

Evidence:

- Shown as distinct metric next to Epsilon/EARL/ECGL in screenshot.
- Internal fields reference:
  - `emeb.confidence_range`
  - `emeb.epsilon_confidence_correlation`

Interpretation:

- Confidence is not only a UI number; it is monitored statistically against epsilon behavior.
- This likely guards against pathological states (high confidence with high uncertainty, or incoherent confidence drift).

## End-to-End Cycle (Reconstructed)

1. Ingest interaction/event, embed memory trace.
2. Retrieve candidate memory set by vector similarity.
3. Expand context via graph neighbors/relations.
4. Compute/update `epsilon`, `confidence`, `outcome`, `novelty`, `stability`.
5. Compute `L(m)` and gate integration.
6. Integrate approved traces into semantic layer with learning-rate modulation.
7. Quarantine blocked traces; decay novelty and re-evaluate periodically.
8. Update global integrity scalar and adjust threshold conservatism.

## Integrity Control

The ECGL paper introduces `CI(t)` (Cognitive Integrity) as average integrated learning weight.

Operationally:

- High CI => normal operation
- Mid CI => caution in novel domains
- Low CI => restrict high-stakes decisions and audit semantic memory

This provides a system-level self-trust measure beyond per-memory confidence.

