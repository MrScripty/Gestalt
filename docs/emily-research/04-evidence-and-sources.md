# Evidence And Sources

## Repo-Local Artifacts Reviewed

- `docs/emily-research/Emily_OS-Sovereign_Cognition-v4.pdf`
- `docs/emily-research/Emily_OS-EMEB_Scientific_Paper.pdf`
- `docs/emily-research/Emily_OS-EARL_Scientific_Paper.pdf`
- `docs/emily-research/Emily_OS-ECGL_Scientific_Paper.pdf`

## Additional Local Artifacts Reviewed

- `/home/jeremy/Downloads/Screenshot2026_02_27_124701.jpg`
- `/home/jeremy/Downloads/Screenshot2026_02_27_125630.jpg`
- `/home/jeremy/Downloads/Screenshot2026_02_27_125814.jpg`
- `/home/jeremy/Downloads/Screenshot2026_02_27_130927.jpg`

## Directly Evidenced By Papers

### Sovereign Cognition (March 2026)

Direct claims supported by `Emily_OS-Sovereign_Cognition-v4.pdf`:

- Emily is framed as a sovereign cognitive architecture, not a model and not a wrapper.
- Identity is intended to persist across model, provider, hardware, and session changes.
- Identity-bearing memory is described using `L3 distilled essences` and `L4 raw sources`.
- External foundation models are dispatched as reasoning organs.
- A `Semantic Membrane` bounds what crosses the trust boundary.
- Local governance is performed through `EMEB`, `EARL`, `ECCR`, `ECGL`, and `AOPO/APC`.
- Final rendering happens locally, not inside the remote model.
- Indigenous-owned and community-governed data sovereignty is presented as an architectural foundation, not a policy add-on.

### EMEB (November 2025)

Direct claims supported by `Emily_OS-EMEB_Scientific_Paper.pdf`:

- `EMEB` is a probabilistic framework for expected union size and duplicate-mass estimation.
- The paper is explicitly about deduplication and memory coordination under stated assumptions.
- It introduces effective-universe estimation (`Meff`) and correlation-aware mitigation.
- It includes an adaptive controller that chooses between cheap and expensive deduplication modes.
- It is framed as a control signal for deciding when embeddings/vector search are worth the compute.

### EARL (November 2025)

Direct claims supported by `Emily_OS-EARL_Scientific_Paper.pdf`:

- `EARL` is a pre-cognitive gate over reasoning traces.
- It uses a compact signal vector and Mahalanobis-distance anomaly detection.
- It defines three states: `OK`, `CAUTION`, `REFLEX`.
- It is intended to prevent memory pollution from confused reasoning.
- It includes a continuity-anchor signal for drift from established patterns.

### ECGL (November 2025)

Direct claims supported by `Emily_OS-ECGL_Scientific_Paper.pdf`:

- `ECGL` is a meta-memory layer for controlled identity formation.
- Learning weight combines confidence, outcome, novelty, and stability.
- Gate controls integrate vs quarantine.
- Quarantined memories remain recallable but are blocked from semantic identity formation.
- Novelty decay and adaptive threshold calibration are part of the framework.
- `CI` is the global cognitive-integrity scalar.

## Extracted Signals From Screenshots

- Cognitive metrics panel includes:
  - `Epsilon (EMEB): Uncertainty measure`
  - `Confidence: Decision certainty`
  - `Learning Weight (EARL): Outcome-weighted learning`
  - `Stability (ECGL): Knowledge consolidation`
- Internal/starting metrics references include:
  - `emeb.epsilon_range`
  - `emeb.confidence_range`
  - `emeb.epsilon_confidence_correlation`
  - `earl.outcome_responsiveness`
  - `ecgl.novelty_rate`
  - `memory_states.integration_rate`
- Graph view legend:
  - Size = learning weight
  - Color = primary dimension
  - Distance = age/importance
  - Lines = relationships (top links)

## Working Interpretation Layers

### High Confidence

- Existence of `EMEB`, `EARL`, and `ECGL` as named formal frameworks.
- `EARL` as a pre-cognitive gating framework, not merely post-hoc scoring.
- `ECGL` as an integration / identity-formation gate with quarantine and `CI`.
- Presence of explicit `Confidence` tracking and epsilon-confidence coupling in screenshots.
- Presence of vector / graph memory visualization semantics from screenshots.
- Existence of the broader sovereign-cognition framing with membrane-bounded dispatch.

### Medium Confidence

- Exact runtime orchestration details and service decomposition in production.
- Exact implementation of `ECCR` and `AOPO/APC` beyond their high-level roles in the March 2026 paper.
- Precise relationship between paper-level `EMEB` deduplication math and screenshot-level epsilon/confidence telemetry.

### Low Confidence / Unknown

- Proprietary production thresholds and hidden heuristics.
- Exact provider routing policy in live systems.
- Unpublished anti-adversarial defenses and full remote-call validation logic.

## Public Links Checked

- `https://emily.robinsonai.com`
- `https://robsoninc.com/#/science`
- `https://emily.robsoninc.com`
- `https://robsoninc.com/#/emeb`
- `https://robsoninc.com/#/earl`
- `https://robsoninc.com/#/ecgl`
- `https://ai.robsoninc.com/marketing/`

## Ambiguities To Preserve

- `ECCR` naming drifts in the March 2026 paper:
  - one passage reads like `Epistemic Confidence-Coherence-Relevance`
  - the evidence table expands it to `Epistemic Confidence-Coherence-Curiosity-Relevance`
- `EARL` appears in two names across the local corpus:
  - `Episode-level Adaptive Risk Learning`
  - `Episode-level Adaptive Risk Assessment`
- `EMEB` is directly evidenced as a deduplication / memory-coordination framework, while later documents and screenshots also position it inside the confidence / epsilon vocabulary.
