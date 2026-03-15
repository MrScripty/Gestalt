# bin

## Purpose
`bin` contains auxiliary executables used for profiling and diagnostics outside the main desktop app entrypoint.

## Contents
| File/Folder | Description |
| ----------- | ----------- |
| `emily_inspect.rs` | Deterministic Emily inspection runner for seeded or live local DBs |
| `emily_membrane_dev.rs` | Dev-only membrane execution runner for controlled local Gestalt flows |
| `emily_pantograph_embedding_probe.rs` | Real Pantograph embedding validator that updates the `Embedding` workflow model binding, proves session-based vector return, and measures warm-session reuse |
| `emily_pantograph_reasoning_probe.rs` | Live Pantograph reasoning validator that repairs a selected workflow into a puma-backed membrane path and reports the resulting Emily routing, reflex, remote, validation, audit, or compatibility blocker state |
| `emily_seed.rs` | Deterministic Emily seed corpus runner for local diagnostics and host acceptance prep |
| `profile_terminal.rs` | PTY latency profiler plus replay-only legacy-vs-native terminal path benchmark |
| `terminal_native_spike.rs` | Feature-gated native GPU terminal spike using the Alacritty-backed `terminal_native` path |

## Problem
Developers need targeted runtime diagnostics without altering production app flow.

## Constraints
- Must stay compatible with core terminal subsystem contracts.
- Should remain optional for normal app execution.

## Decision
Keep diagnostics in a dedicated binary target under `src/bin`.

## Alternatives Rejected
- Embedding diagnostics in `main.rs`: rejected due to coupling.

## Invariants
- Diagnostic binary does not change app runtime behavior.

## Revisit Triggers
- Additional tooling binaries are introduced.

## Dependencies
**Internal:** `terminal`, `state`  
**External:** standard Rust toolchain

## Related ADRs
None.

## Usage Examples
```bash
cargo run --bin profile_terminal
cargo run --bin profile_terminal --features terminal-native-spike -- --replay-only --json
cargo run --bin emily_seed -- --reset
cargo run --bin emily_inspect -- --dataset synthetic-agent-round --reseed --reset --query "provider registry"
cargo run --bin emily_pantograph_embedding_probe
GESTALT_PANTOGRAPH_REASONING_WORKFLOW_ID='Coding Agent' cargo run --bin emily_pantograph_reasoning_probe
GESTALT_ENABLE_MEMBRANE_DEV=1 cargo run --bin emily_membrane_dev -- --reseed --reset
cargo run --features terminal-native-spike --bin terminal_native_spike
```
