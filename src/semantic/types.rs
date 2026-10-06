//! Core types for the controlled-transition ADT.
//!
//! See specs.md — Data Model section.

use std::collections::HashSet;

// ── Identifier newtypes ───────────────────────────────────────────────────────

/// Opaque state identifier. Dense in [0, N).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StateId(pub u32);

/// Opaque input (action) identifier. Dense in [0, A).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InputId(pub u32);

/// Opaque action-pair identifier. Dense in [0, H) after freeze.
/// Identifies one (source, input) pair in the CSR layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PairId(pub u32);

// ── Set types ─────────────────────────────────────────────────────────────────

/// Dense bit-vector state set: N bits stored in ceil(N/64) u64 words.
/// Default state-set representation for Phase 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DenseBitVec {
    words: Vec<u64>,
    /// Number of bits (= number of states).
    n_bits: usize,
}

impl DenseBitVec {
    /// Create an empty (all-zeros) bit vector with `n_bits` bits.
    pub fn zeros(n_bits: usize) -> Self {
        let n_words = n_bits.div_ceil(64);
        Self {
            words: vec![0u64; n_words],
            n_bits,
        }
    }

    /// Create a full (all-ones) bit vector.
    pub fn ones(n_bits: usize) -> Self {
        let n_words = n_bits.div_ceil(64);
        let mut words = vec![!0u64; n_words];
        // Clear padding bits in the last word.
        let rem = n_bits % 64;
        if rem != 0 {
            if let Some(last) = words.last_mut() {
                *last = (1u64 << rem) - 1;
            }
        }
        Self { words, n_bits }
    }

    pub fn contains(&self, id: usize) -> bool {
        debug_assert!(id < self.n_bits);
        (self.words[id / 64] >> (id % 64)) & 1 == 1
    }

    pub fn insert(&mut self, id: usize) {
        debug_assert!(id < self.n_bits);
        self.words[id / 64] |= 1u64 << (id % 64);
    }

    pub fn remove(&mut self, id: usize) {
        debug_assert!(id < self.n_bits);
        self.words[id / 64] &= !(1u64 << (id % 64));
    }

    pub fn is_empty(&self) -> bool {
        self.words.iter().all(|&w| w == 0)
    }

    pub fn len(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// Iterate over set bits (state ids).
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.words
            .iter()
            .enumerate()
            .flat_map(|(wi, &w)| BitIter(w).map(move |bit| wi * 64 + bit))
            .filter(move |&id| id < self.n_bits)
    }

    /// Bitwise OR in place (union).
    pub fn union_with(&mut self, other: &Self) {
        debug_assert_eq!(self.n_bits, other.n_bits);
        for (a, b) in self.words.iter_mut().zip(other.words.iter()) {
            *a |= b;
        }
    }

    /// Bitwise AND in place (intersection).
    pub fn intersect_with(&mut self, other: &Self) {
        debug_assert_eq!(self.n_bits, other.n_bits);
        for (a, b) in self.words.iter_mut().zip(other.words.iter()) {
            *a &= b;
        }
    }

    pub fn n_bits(&self) -> usize {
        self.n_bits
    }
}

/// Helper: iterate set bits of a single u64 word.
struct BitIter(u64);
impl Iterator for BitIter {
    type Item = usize;
    fn next(&mut self) -> Option<usize> {
        if self.0 == 0 {
            return None;
        }
        let bit = self.0.trailing_zeros() as usize;
        self.0 &= self.0 - 1;
        Some(bit)
    }
}

/// A state set (Phase 1: dense bit vector).
pub type StateSet = DenseBitVec;

/// An input set (Phase 1: dense bit vector over input ids).
pub type InputSet = DenseBitVec;

/// A pair set (Phase 1: a HashSet of PairIds; may be replaced with a bit vector).
pub type PairSet = HashSet<PairId>;

// ── Result types ──────────────────────────────────────────────────────────────

/// Result of a reachability computation.
#[derive(Debug, Clone)]
pub struct ReachResult {
    /// Winning state set W: states from which target is reachable.
    pub winning: StateSet,
    /// Rank of each winning state (distance to target, in steps).
    /// `rank[x.0 as usize]` is meaningful only if x ∈ winning.
    pub rank: Vec<usize>,
    /// Chosen progress action for each winning state.
    pub chosen: std::collections::HashMap<StateId, PairId>,
}

/// Result of a controlled invariance computation.
#[derive(Debug, Clone)]
pub struct SafetyResult {
    /// Maximal controlled invariant subset Y* ⊆ safe.
    pub invariant: StateSet,
}

// ── Trace types ───────────────────────────────────────────────────────────────

/// Parsed trace file header (Appendix C format).
#[derive(Debug, Clone)]
pub struct TraceHeader {
    pub n_states: usize,
    pub n_inputs: usize,
    /// Optional human-readable names (index = id).
    pub state_names: Vec<Option<String>>,
    pub input_names: Vec<Option<String>>,
    /// SHA-256 checksum of sorted (x,u,y) triples, if present in header comments.
    pub checksum: Option<String>,
}
