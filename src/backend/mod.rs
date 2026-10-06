//! Layer 3: Frozen backend interface.
//!
//! See specs.md § F5 — IndexedFrozen Backend

pub mod indexed_frozen;

#[cfg(feature = "oxidd")]
pub mod bdd;

pub use indexed_frozen::IndexedFrozen;
