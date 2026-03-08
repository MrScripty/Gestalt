//! Emily membrane crate.
//!
//! This crate is the sibling sovereign-dispatch layer above the `emily` core
//! crate. It will eventually own bounded task compilation, routing, dispatch,
//! validation orchestration, and local reconstruction without taking ownership
//! of Emily's durable memory and policy state.

pub mod contracts;
pub mod runtime;
