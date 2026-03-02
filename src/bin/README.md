# bin

## Purpose
`bin` contains auxiliary executables used for profiling and diagnostics outside the main desktop app entrypoint.

## Contents
| File/Folder | Description |
| ----------- | ----------- |
| `profile_terminal.rs` | PTY input latency profiling utility |

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
```
