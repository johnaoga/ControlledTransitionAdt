//! Layer 1: Representation-independent ADT contract.
//!
//! See specs.md § F1 — `ControlledTransitionAdt` Trait
//! Transcribed from blueprint Listing 1.

pub mod reference;
pub mod types;

pub use types::{InputId, InputSet, PairId, PairSet, ReachResult, SafetyResult, StateId, StateSet};

/// Representation-independent public interface for a finite controlled transition system
/// S = (X, U, F) where F: X × U → 2^X.
///
/// All backends implement this trait; solvers are generic over it.
///
/// # Object safety
/// Methods returning iterators use `Box<dyn Iterator<...>>` to preserve object safety
/// for dynamic dispatch in the benchmark harness.
pub trait ControlledTransitionAdt {
    /// The set of all states X.
    fn states(&self) -> &StateSet;

    /// The set of all inputs U.
    fn inputs(&self) -> &InputSet;

    /// Returns true iff y ∈ F(x, u).
    fn contains(&self, x: StateId, u: InputId, y: StateId) -> bool;

    /// Iterator over F(x, u) — the successor set of (x, u).
    fn successors(&self, x: StateId, u: InputId) -> Box<dyn Iterator<Item = StateId> + '_>;

    /// Iterator over {(x, u) : y ∈ F(x, u)} — pairs for which y is a successor.
    /// May be O(M) if no reverse index is present.
    fn predecessor_pairs(&self, y: StateId) -> Box<dyn Iterator<Item = (StateId, InputId)> + '_>;

    /// Iterator over admissible inputs at state x: {u : F(x, u) ≠ ∅}.
    fn admissible_inputs(&self, x: StateId) -> Box<dyn Iterator<Item = InputId> + '_>;

    /// Post-image of a state set under optionally filtered inputs.
    fn post(&self, s: &StateSet, filter: Option<&InputSet>) -> StateSet;

    /// All (x, u) pairs whose entire successor set lies in y.
    fn safe_actions(&self, y: &StateSet) -> PairSet;

    /// Controllable predecessor: CPre(Y) = { x : ∃u, F(x,u) ⊆ Y }.
    fn cpre(&self, y: &StateSet) -> StateSet;

    /// Compute the reachability winning region and a progress controller.
    fn reachability(&self, target: &StateSet, constraint: Option<&StateSet>) -> ReachResult;

    /// Compute the maximal controlled invariant subset of `safe`.
    fn invariance(&self, safe: &StateSet) -> SafetyResult;

    // ── Capability flags ──────────────────────────────────────────────────────

    /// True if the backend stores a reverse incidence index (rev_off / rev_pair).
    fn has_reverse_incidence(&self) -> bool {
        false
    }

    /// True if the backend implements CPre natively (e.g., BDD quantification).
    fn has_native_cpre(&self) -> bool {
        false
    }

    /// True if the backend supports incremental `addTransition` after freeze.
    fn supports_incremental_updates(&self) -> bool {
        false
    }
}
