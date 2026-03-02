# resource_monitor

## Purpose
`resource_monitor` samples system/process metrics and maps them to per-session load states for UI indicators.

## Contents
| File/Folder | Description |
| ----------- | ----------- |
| `mod.rs` | Cross-platform aggregation and load classification |
| `platform_linux.rs` | Linux process/memory sampling |
| `platform_macos.rs` | macOS process/memory sampling |
| `platform_windows.rs` | Windows process/memory sampling |

## Problem
The app needs lightweight, periodic resource visibility per terminal session.

## Constraints
- Must support Linux, macOS, and Windows.
- Sampling must degrade gracefully on command failures.

## Decision
Use platform-specific modules behind a shared interface and aggregate to a common snapshot model.

## Alternatives Rejected
- Single platform-conditional file: rejected due to readability.
- External monitoring daemon: rejected due to complexity.

## Invariants
- Public snapshot type remains platform-agnostic.
- Platform modules only expose sampling internals.

## Revisit Triggers
- Need high-frequency telemetry beyond periodic polling.
- OS APIs replace command-based sampling.

## Dependencies
**Internal:** `state`, `ui`  
**External:** `serde_json` (windows parser)

## Related ADRs
None.

## Usage Examples
```rust
let snapshot = crate::resource_monitor::sample_resource_snapshot(&session_roots);
# let _ = snapshot;
```
