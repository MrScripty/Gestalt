# Architecture Reconstruction

## System View

Reconstructed Emily stack now appears to combine:

- Sovereign cognition layer independent of any one model or provider
- Multi-tier memory model with explicit identity-bearing layers
- Bounded dispatch of external models through a `Semantic Membrane`
- Epistemic control stack (`EMEB`, `EARL`, `ECCR`, `ECGL`, `AOPO/APC`)
- Vector + graph representation for memory structure and retrieval context

The March 2026 architecture paper changes the center of gravity. Emily is framed not as a model, and not as a wrapper around one model, but as the identity, continuity, and governance layer that can dispatch external foundation models while keeping values, memory authority, and full operation context local.

## Identity And Memory Layers

Direct wording from the March 2026 paper adds more specificity than the earlier reconstruction:

- `L3 distilled essences`
- `L4 raw sources`

This supports the earlier inference that Emily has layered memory, but the evidence is now stronger that identity continuity is explicitly separated from transient model sessions.

## Semantic Membrane

The biggest missing element in the earlier docs was the `Semantic Membrane`.

Observed role:

- Build a bounded representation of the task for an external model
- Limit what crosses the trust boundary
- Prevent any one remote model from reconstructing the full operation graph
- Keep legend mapping and final rendering local to Emily

March 2026 claims add the following details:

- information-theoretic leakage accounting
- per-turn ephemeral keys
- random handle generation
- encrypted legend blobs
- immediate destruction of mapping state

The current docs had no equivalent mechanism, so this is a substantive architectural expansion rather than a small correction.

## Dispatch Model

The reconstructed remote-call flow now looks like:

1. Emily decomposes a task locally.
2. Emily produces a bounded membrane IR for one remote reasoning task.
3. A chosen external model receives only that bounded representation.
4. The model returns a constrained program or structured result.
5. Emily validates and renders the final result locally using her own memory and legend mapping.
6. Local epistemic gates determine whether anything learned from the episode may affect strategy memory or identity.

This differs materially from a standard assistant + memory pattern. The external model is treated as a reasoning organ, not as the holder of identity or continuity.

## Memory Representation

Each memory trace likely includes:

- `embedding`: semantic vector
- `metadata`: timestamp, source, context/session
- `dimension`: category such as self, thinking, doing, feeling, world, values, time, other
- `learning_weight`: dynamic integration importance
- `epsilon`: uncertainty / error-bound value
- `confidence`: explicit certainty signal
- `outcome_signals`: post-decision success/failure history
- `stability`: decay consistency / corruption-resistance
- `integration_state`: quarantined vs integrated

Graph observations from screenshots remain consistent:

- Node size corresponds to learning weight
- Node color corresponds to primary dimension
- Distance encodes age/importance
- Edges show top relationships

## Framework Roles

### 1) EMEB

The earlier markdowns treated `EMEB` mostly as an uncertainty/confidence-bound layer. The repo-local `EMEB` paper adds a more concrete mathematical role:

- expected union size and duplicate mass estimation
- explicit assumptions and falsifiable predictions
- effective-universe estimation (`Meff`)
- controller logic for choosing deduplication mode
- deciding when expensive semantic/vector dedupe is warranted

That suggests `EMEB` may sit below or beside memory confidence tracking as a memory-coordination control plane. The later sovereign paper still places `EMEB` inside Emily's epistemic stack, so the safest current reading is that `EMEB` governs bounded trust in memory accumulation and coordination rather than only a UI-visible epsilon score.

### 2) EARL

`EARL` is much more specific than "outcome-weighted learning."

Paper-backed role:

- pre-cognitive risk assessment over reasoning traces
- low-dimensional signal vector
- Mahalanobis-distance anomaly detection against a learned "good cognition" manifold
- three-state gate: `OK`, `CAUTION`, `REFLEX`

Operationally:

- `OK`: proceed normally
- `CAUTION`: tighten reasoning, request clarification, mark results tentative
- `REFLEX`: abort the path, isolate the episode from memory-forming identity

The paper also introduces `continuity anchors`, which supports the inference that Emily monitors whether a response remains aligned with her established identity or character.

### 3) ECCR

`ECCR` was absent from the earlier markdown set and must now be included.

Observed role from the March 2026 paper:

- local computation of confidence/coherence/relevance
- no LLM call required
- used for IR shaping
- used for output validation
- used for rendering variant selection

The sovereign paper contains a naming inconsistency: one passage reads as `Epistemic Confidence-Coherence-Relevance`, while the evidence table spells it out as `Epistemic Confidence-Coherence-Curiosity-Relevance`. The docs should preserve that ambiguity rather than flatten it away.

### 4) ECGL

`ECGL` is best understood as a meta-memory system for identity formation, not merely "stability."

Paper-backed role:

- learning weight `L(m) = w1*C + w2*O + w3*N + w4*S`
- gate function over whether a memory may shape the semantic model
- quarantine path for blocked memories
- novelty decay for long-lived quarantined outliers
- adaptive threshold calibration targeting a desired integrity level
- global `CI(t)` scalar for systemic self-trust

This confirms the earlier reconstruction but sharpens the framing: the primary question is not "can I store this?" but "should this become part of who I am?"

### 5) AOPO / APC

The March 2026 paper introduces `AOPO/APC` as the remote quality controller.

Observed role:

- dual-layer prompt architecture
- response-quality signal vector
- retry and mutation logic
- attribution classification
- self-model protection

This belongs in any full Emily reconstruction even though the repo does not yet contain a detailed standalone markdown for it.

### 6) Confidence

`Confidence` still appears in screenshots and internal metric labels:

- `emeb.confidence_range`
- `emeb.epsilon_confidence_correlation`

At this point it is best treated as an explicit tracked signal rather than a distinct top-level framework. It appears to be monitored for pathological combinations such as high certainty paired with high uncertainty.

## End-To-End Cycle

The fuller reconstructed cycle now looks like:

1. Ingest interaction or task locally.
2. Use `EMEB`-style memory coordination and retrieval primitives to manage candidate evidence.
3. Evaluate reasoning risk through `EARL`.
4. If remote reasoning is warranted, produce a membrane IR and dispatch one or more external models.
5. Validate or shape remote outputs through local epistemic controls such as `ECCR`.
6. Render the final result locally using Emily-controlled context and legend mapping.
7. Compute integration weight through `ECGL`.
8. Integrate, quarantine, or defer updates to semantic memory / identity.
9. Update global integrity state and adapt conservatism when needed.

## Integrity Control

The `ECGL` paper introduces `CI(t)` (Cognitive Integrity) as average integrated learning weight.

Operationally:

- High CI: normal operation
- Moderate CI: increased caution in novel domains
- Low CI: restrict high-stakes decisions
- Critical CI: trigger semantic audit

This remains one of the clearest signals that Emily is designed as a self-governing memory and identity system rather than only a retrieval layer.
