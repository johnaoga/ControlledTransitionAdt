//! Layer 2: Transition builder trait and implementations.
//!
//! See specs.md § F4 — HashBuilder Backend

pub mod hash_builder;
pub mod trace_io;

pub use hash_builder::HashBuilder;
pub use trace_io::{parse_trace, triple_checksum, write_trace, TraceError};

use crate::semantic::types::{InputId, StateId};

/// Trait for mutable, insertion-oriented transition stores.
pub trait TransitionBuilder {
    /// Record the transition x --u--> y.
    fn add_transition(&mut self, x: StateId, u: InputId, y: StateId);

    /// Total number of (x, u, y) triples inserted (before deduplication).
    fn n_transitions_raw(&self) -> usize;
}
