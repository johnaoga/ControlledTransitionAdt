//! BDD backend stub (feature = "oxidd", Phase 3).
//!
//! Not wired in Phase 1. The oxidd dependency is in Cargo.toml behind a feature flag.
//! See specs.md — Out of Scope (Phase 3).

// Compiled only with `--features oxidd` (gated in backend/mod.rs).

/// Stub BDD-backed transition store using the oxidd crate.
///
/// TODO: implement in Phase 3 — see specs.md
pub struct BddBackend {
    _placeholder: (),
}
