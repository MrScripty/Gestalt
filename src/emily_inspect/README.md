# emily_inspect

## Purpose
`emily_inspect` contains deterministic inspection helpers for reading Emily
state from the Gestalt host side without introducing product runtime ownership.

## Contents
| File | Description |
| ---- | ----------- |
| `mod.rs` | Public inspection helpers and query/result models |

## Relationship
This module supports diagnostics and testability. It is not a composition root
and does not own long-lived runtime resources.
