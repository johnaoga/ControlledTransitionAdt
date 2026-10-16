//! Layer 3: Frozen backends.
//!
//! - [`IndexedFrozen`]: explicit CSR store with reverse incidence index (Phases 1–2).
//! - `BddBackend` / `BcddBackend`: symbolic stores on oxidd (Phase 3, feature = "oxidd").

pub mod indexed_frozen;

#[cfg(feature = "oxidd")]
pub mod bdd;

#[cfg(feature = "oxidd")]
pub use bdd::{BcddBackend, BddBackend, DdBackend, DdConfig, DdKind};
pub use indexed_frozen::IndexedFrozen;
