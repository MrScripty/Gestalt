# Plan: Gestalt Emily Adoption And Test Data

## Objective

Define a standards-aligned plan for beginning real Emily usage in Gestalt,
including how to seed data into Emily for testing, how to observe it, and how
to gradually move host behavior onto Emily-backed flows.

## Scope

### In Scope

- Planning how Gestalt should begin using Emily beyond passive crate tests
- Planning how to seed deterministic test data into Emily
- Planning how to validate Emily-backed flows in the Gestalt host
- Defining phased adoption from test data to real host usage
- Recording what infrastructure is needed for inspection, replay, and reset

### Out of Scope

- Final product UX for every Emily-backed feature
- Full sovereign-cognition deployment in Gestalt in one pass
- Immediate replacement of all existing persistence paths
- Production data migration strategy
- External data collection policy beyond local repo/testing concerns

## Inputs

### Problem

Emily and `emily-membrane` now have enough core capability that purely
crate-level testing is no longer sufficient. Gestalt needs a practical adoption
plan for:

- putting real data into Emily
- inspecting what Emily stores and returns
- using Emily-backed flows in controlled host slices
- testing those flows with deterministic seeded data before depending on live
  user traffic alone

Today Gestalt already writes terminal history into Emily, but that is not yet a
complete adoption strategy. We still need a plan for test corpus seeding,
episode creation, context retrieval use, policy-selected execution use, and
host-side inspection/debug loops.

### Constraints

- Emily remains the durable source of truth for memory, episode, validation,
  and sovereign record state.
- Gestalt should consume Emily only through public crate facades.
- Test-data seeding must be deterministic and replayable.
- Adoption slices should be reversible until behavior is trusted.
- Verification must include both crate-level and host-level acceptance checks.
- Any generated test corpus should avoid hidden product-policy assumptions and
  should be clearly labeled as synthetic or captured.

### Assumptions

- The first useful adoption work is not a full UI feature; it is a controlled
  test and inspection loop.
- Seeded data is necessary because relying only on live terminal history is too
  slow and too noisy for early development.
- Gestalt should support both:
  - synthetic deterministic test fixtures
  - optional captured local replay fixtures
- Early adoption should emphasize observability over automation.

### Dependencies

- [08-gestalt-emily-development-plan.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/08-gestalt-emily-development-plan.md)
- [09-emily-crate-continuation-plan.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/09-emily-crate-continuation-plan.md)
- [13-emily-membrane-routing-policy-plan.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/13-emily-membrane-routing-policy-plan.md)
- [14-emily-membrane-execution-depth-plan.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/docs/emily-research/14-emily-membrane-execution-depth-plan.md)
- Current Gestalt Emily bridge and terminal integration
- Shared standards in `/media/jeremy/OrangeCream/Linux Software/Coding-Standards/`
- Repo rules in [GESTALT-STANDARDS.md](/media/jeremy/OrangeCream/Linux%20Software/Gestalt/GESTALT-STANDARDS.md)

### Affected Structured Contracts

- Gestalt-side test fixture definitions for:
  - text objects
  - episodes
  - trace links
  - outcomes
  - `EARL` evaluations
  - routing / validation records as needed
- Possible host-side debug DTOs or views for inspection
- Membrane execution test inputs for policy-selected local/remote flows

### Affected Persisted Artifacts

- Emily databases used for:
  - synthetic seed corpus testing
  - replay fixtures
  - live host development
- Optional fixture files checked into the repo for deterministic seed scenarios
- Optional local replay exports stored outside committed test fixtures

### Concurrency And Race-Risk Review

Host adoption must account for:

- test database isolation between runs
- deterministic reset/cleanup behavior
- separation between synthetic seeded datasets and live user data
- avoidance of mixing fixture seeding with active live sessions in the same DB
- clear lifecycle for opening, reusing, and closing Emily databases in tests

If adoption work requires concurrent live ingestion and test seeding into the
same target DB, stop and re-plan that slice explicitly.

### Risks

| Risk | Impact | Mitigation |
| ---- | ------ | ---------- |
| Gestalt starts using Emily without enough observability | High | Add inspection and replay tooling before wider adoption |
| Seed data becomes unrealistic or misleading | High | Use both synthetic deterministic fixtures and optional captured replay data |
| Test and live data mix in the same DB | High | Use isolated database locators and explicit dataset labeling |
| Adoption outruns membrane capability | Medium | Sequence host integration after stable seed/inspection loops |
| Host behavior diverges from crate acceptance coverage | Medium | Add cross-layer Gestalt acceptance tests using seeded Emily DBs |

## Definition of Done

- The plan defines how Gestalt should seed, inspect, and adopt Emily.
- The plan includes a deterministic data-seeding strategy for testing.
- The plan sequences adoption from test corpus to real host behavior.
- The plan defines host-level validation and reset/replay expectations.

## Ownership And Lifecycle Note

Gestalt remains responsible for:

- choosing test database locations
- seeding and resetting test datasets
- deciding when to use synthetic vs captured fixtures
- starting and stopping host-facing Emily and membrane runtimes

No adoption milestone should blur fixture DBs and live DBs.

## Public Facade Preservation Note

Gestalt adoption should use:

- `emily::EmilyApi`
- `emily_membrane::runtime::MembraneRuntime`

It should not reach into Emily or membrane internals for seeding or inspection
unless a new explicit debug/test facade is added.

## Recommended Adoption Strategy

Use four layers of increasing realism:

1. synthetic deterministic seed corpus
2. host-side inspection and replay
3. narrow Emily-backed host features
4. membrane-backed execution in controlled flows

This keeps the early loop testable while still moving toward real usage.

## Milestones

### Milestone A: Seed Corpus Infrastructure

**Goal:** Make it easy to put known data into Emily for testing.

**Tasks:**
- [x] Add a deterministic seed runner for Emily test DBs
- [x] Define fixture format for:
  - text objects
  - episodes
  - trace links
  - outcomes
  - optional `EARL` evaluations
- [x] Add dataset labels such as:
  - `synthetic-terminal`
  - `synthetic-agent-round`
  - `synthetic-risk-gated`
- [x] Ensure seeded runs are replay-safe and resettable

**Execution Notes:**
- Implemented in `gestalt::emily_seed` with a reusable host-side seed corpus
  module, a diagnostic `emily_seed` binary, and host-level acceptance coverage.
- Built-in datasets now cover terminal history, a succeeded agent round, and
  cautioned / blocked `EARL` scenarios.

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- host-level acceptance test that seeds a DB and reads back the expected Emily
  artifacts

### Milestone B: Inspection And Debug Loop

**Goal:** Let developers see what Emily contains and what it returns.

**Tasks:**
- [ ] Add a narrow inspection path for:
  - episodes
  - latest `EARL`
  - context query results
  - routing / validation records
- [ ] Add deterministic debug output for seeded DBs
- [ ] Add reset / recreate workflow for local test databases

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- manual developer loop documented and repeatable

### Milestone C: Retrieval Adoption In Gestalt

**Goal:** Start using Emily context in one narrow host flow.

**Tasks:**
- [ ] Pick the first real consumer:
  - local agent prompt assembly
  - command follow-up assistance
  - snippet recall
- [ ] Pull context from Emily through the public facade
- [ ] Add host-level acceptance coverage using seeded DBs
- [ ] Keep fallback behavior explicit if Emily is unavailable

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- Gestalt acceptance coverage using seeded Emily data

### Milestone D: Episode And Policy Adoption

**Goal:** Begin mapping real Gestalt actions into Emily episode flows beyond
terminal-history persistence.

**Tasks:**
- [ ] Define first real episode-producing host flow
- [ ] Link relevant text/context to those episodes
- [ ] Add optional seeded `EARL` scenarios for caution and reject cases
- [ ] Verify host behavior against episode state and latest `EARL`

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- seeded host acceptance tests for open, cautioned, and blocked episodes

### Milestone E: Membrane Adoption In Controlled Host Flow

**Goal:** Use the membrane facade in one controlled Gestalt flow.

**Tasks:**
- [ ] Pick a non-destructive host action for first membrane execution
- [ ] Use policy-selected execution with seeded or isolated local data
- [ ] Add inspection of resulting routing / remote / validation artifacts
- [ ] Keep the feature behind an explicit development toggle until trusted

**Verification:**
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`
- seeded or isolated host acceptance coverage through the membrane facade

## Test Data Strategy

Use three test-data classes:

### Synthetic Deterministic Fixtures

Use these for:

- unit and acceptance testing
- reliable seeded scenarios
- known routing / `EARL` / validation outcomes

Recommended fixture families:

- terminal command history with summaries
- retrieval-heavy snippet sets
- cautioned and blocked episodes
- remote-eligible and remote-rejected membrane tasks

### Captured Local Replay Fixtures

Use these for:

- debugging realistic host behavior
- validating event-to-episode mapping
- checking retrieval relevance against real sequences

These should be opt-in local artifacts, not default committed fixtures, unless
they are sanitized and intentionally curated.

### Live Development Data

Use this only after:

- seed and inspection loops are stable
- DB isolation is clear
- reset and backup paths exist

## Re-Plan Triggers

- Gestalt needs Emily writes or reads that are not available through public
  facades
- Seed fixtures are not rich enough to test the first real host consumer
- Retrieval adoption exposes major weaknesses in Emily context ranking
- Host integration requires a debug or inspection facade not yet planned
- Membrane adoption begins requiring broader product-policy choices too early

## Recommendations

- Start with deterministic seed infrastructure before wider host usage.
- Make one inspection loop before one host feature.
- Adopt Emily context in one narrow Gestalt flow before broader membrane use.
- Keep synthetic and live data isolated at the database level.
- Treat seeded `EARL` scenarios as required test data, not optional extras.

## Completion Criteria

- There is a clear plan for putting deterministic test data into Emily.
- There is a clear plan for inspecting and validating Emily-backed host flows.
- Gestalt adoption is sequenced from low-risk testing into controlled real use.
