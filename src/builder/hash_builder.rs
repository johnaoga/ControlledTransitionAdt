//! HashBuilder: HashMap-backed mutable transition store.
//!
//! See specs.md § F4 — HashBuilder Backend

use std::collections::{HashMap, HashSet};

use crate::backend::indexed_frozen::IndexedFrozen;
use crate::builder::TransitionBuilder;
use crate::semantic::types::{
    InputId, InputSet, PairId, PairSet, ReachResult, SafetyResult, StateId, StateSet,
};
use crate::semantic::ControlledTransitionAdt;

/// Mutable insertion-oriented transition store.
///
/// Uses `HashMap<(StateId, InputId), HashSet<StateId>>` for O(1) expected insert
/// with automatic deduplication.
///
/// Requires explicit `n_states` and `n_inputs` at construction; ids outside these
/// bounds are a programming error (checked with `debug_assert!`).
///
/// # Pair ids before freeze
/// The builder has no dense pair numbering (that is assigned by `freeze`). Where the
/// ADT needs a `PairId` (`safe_actions`, `ReachResult::chosen`), the builder uses the
/// grid encoding `x * n_inputs + u`; decode it with [`HashBuilder::pair_of`]. These ids
/// are *not* comparable with the `PairId`s of the frozen store.
#[derive(Clone)]
pub struct HashBuilder {
    n_states: usize,
    n_inputs: usize,
    /// Map from (source, input) pairs to their successor sets.
    pairs: HashMap<(StateId, InputId), HashSet<StateId>>,
    /// Raw insertion count (including duplicates).
    n_raw: usize,
    /// Universe sets returned by `states()` / `inputs()`.
    all_states: StateSet,
    all_inputs: InputSet,
}

impl HashBuilder {
    /// Create a new builder with explicit state/input bounds.
    pub fn new(n_states: usize, n_inputs: usize) -> Self {
        Self {
            n_states,
            n_inputs,
            pairs: HashMap::new(),
            n_raw: 0,
            all_states: StateSet::ones(n_states),
            all_inputs: InputSet::ones(n_inputs),
        }
    }

    /// Number of states N.
    pub fn n_states(&self) -> usize {
        self.n_states
    }

    /// Number of inputs A.
    pub fn n_inputs(&self) -> usize {
        self.n_inputs
    }

    /// Returns the number of distinct (source, input) action pairs inserted.
    pub fn n_pairs(&self) -> usize {
        self.pairs.len()
    }

    /// Returns the number of distinct (x, u, y) triples (after deduplication).
    pub fn n_incidences(&self) -> usize {
        self.pairs.values().map(|s| s.len()).sum()
    }

    /// Record the transition x --u--> y.
    ///
    /// Convenience method matching `TransitionBuilder::add_transition`; callers
    /// need not import the trait.
    pub fn add_transition(&mut self, x: StateId, u: InputId, y: StateId) {
        debug_assert!((x.0 as usize) < self.n_states, "state id out of bounds");
        debug_assert!((u.0 as usize) < self.n_inputs, "input id out of bounds");
        debug_assert!((y.0 as usize) < self.n_states, "successor id out of bounds");
        self.n_raw += 1;
        self.pairs.entry((x, u)).or_default().insert(y);
    }

    /// Grid-encoded pair id of (x, u) — see the type-level docs.
    pub fn pair_id(&self, x: StateId, u: InputId) -> PairId {
        let id = x.0 as u64 * self.n_inputs as u64 + u.0 as u64;
        PairId(u32::try_from(id).expect("grid pair id x*A+u overflows u32"))
    }

    /// Decode a grid-encoded pair id back into (x, u).
    pub fn pair_of(&self, p: PairId) -> (StateId, InputId) {
        let a = self.n_inputs as u32;
        (StateId(p.0 / a), InputId(p.0 % a))
    }

    /// Consume the builder and produce a forward-CSR `IndexedFrozen`.
    ///
    /// Pairs are counting-sorted by source, then each state's (at most A) pairs are
    /// sorted by input and each successor slice is sorted. Complexity:
    /// O(N + H + M + Σ_x d_x log d_x + Σ_p d⁺_p log d⁺_p) — linear up to the small
    /// per-state / per-pair sorts.
    pub fn freeze(self) -> IndexedFrozen {
        let n = self.n_states;
        let h = self.pairs.len();
        let m = self.n_incidences();

        // 1. state_pair_off: per-state pair counts, prefix-summed.
        let mut state_pair_off = vec![0u64; n + 1];
        for &(x, _) in self.pairs.keys() {
            state_pair_off[x.0 as usize + 1] += 1;
        }
        for x in 0..n {
            state_pair_off[x + 1] += state_pair_off[x];
        }

        // 2. Bucket pairs by source (counting sort), then sort each bucket by input.
        let mut cursor: Vec<u64> = state_pair_off[..n].to_vec();
        let mut slots: Vec<(InputId, HashSet<StateId>)> =
            (0..h).map(|_| (InputId(0), HashSet::new())).collect();
        for ((x, u), set) in self.pairs {
            let c = &mut cursor[x.0 as usize];
            slots[*c as usize] = (u, set);
            *c += 1;
        }

        // 3. Emit pair metadata and the successor CSR.
        let mut pair_source = Vec::with_capacity(h);
        let mut pair_input = Vec::with_capacity(h);
        let mut pair_succ_off = Vec::with_capacity(h + 1);
        let mut succ = Vec::with_capacity(m);
        pair_succ_off.push(0u64);
        for x in 0..n {
            let bucket = &mut slots[state_pair_off[x] as usize..state_pair_off[x + 1] as usize];
            bucket.sort_unstable_by_key(|&(u, _)| u);
            for (u, set) in bucket.iter_mut() {
                pair_source.push(StateId(x as u32));
                pair_input.push(*u);
                let start = succ.len();
                succ.extend(std::mem::take(set));
                succ[start..].sort_unstable();
                pair_succ_off.push(succ.len() as u64);
            }
        }

        IndexedFrozen {
            n_states: n,
            n_inputs: self.n_inputs,
            n_pairs: h,
            n_incidences: m,
            state_pair_off,
            pair_source,
            pair_input,
            pair_succ_off,
            succ,
            rev_off: None,
            rev_pair: None,
            all_states: self.all_states,
            all_inputs: self.all_inputs,
        }
    }

    /// True iff every successor of (x, u) lies in `y`.
    fn pair_inside(succs: &HashSet<StateId>, y: &StateSet) -> bool {
        succs.iter().all(|s| y.contains(s.0 as usize))
    }
}

impl TransitionBuilder for HashBuilder {
    fn add_transition(&mut self, x: StateId, u: InputId, y: StateId) {
        // Delegate to the inherent method.
        HashBuilder::add_transition(self, x, u, y);
    }

    fn n_transitions_raw(&self) -> usize {
        self.n_raw
    }
}

/// Builder-side ADT queries, so both phases can be benchmarked on the same operations.
///
/// The solvers here are naive CPre-style fixed points (O(k(M+H))); production synthesis
/// runs on the frozen store.
impl ControlledTransitionAdt for HashBuilder {
    fn states(&self) -> &StateSet {
        &self.all_states
    }

    fn inputs(&self) -> &InputSet {
        &self.all_inputs
    }

    fn contains(&self, x: StateId, u: InputId, y: StateId) -> bool {
        self.pairs.get(&(x, u)).is_some_and(|s| s.contains(&y))
    }

    fn successors(&self, x: StateId, u: InputId) -> Box<dyn Iterator<Item = StateId> + '_> {
        match self.pairs.get(&(x, u)) {
            Some(s) => Box::new(s.iter().copied()),
            None => Box::new(std::iter::empty()),
        }
    }

    /// O(M) full scan.
    fn predecessor_pairs(&self, y: StateId) -> Box<dyn Iterator<Item = (StateId, InputId)> + '_> {
        Box::new(
            self.pairs
                .iter()
                .filter(move |(_, s)| s.contains(&y))
                .map(|(&k, _)| k),
        )
    }

    /// O(A) probes, yielded in increasing input order.
    fn admissible_inputs(&self, x: StateId) -> Box<dyn Iterator<Item = InputId> + '_> {
        Box::new(
            (0..self.n_inputs as u32)
                .map(InputId)
                .filter(move |&u| self.pairs.contains_key(&(x, u))),
        )
    }

    fn post(&self, s: &StateSet, filter: Option<&InputSet>) -> StateSet {
        let mut out = StateSet::zeros(self.n_states);
        for (&(x, u), succs) in &self.pairs {
            if s.contains(x.0 as usize) && filter.is_none_or(|f| f.contains(u.0 as usize)) {
                for y in succs {
                    out.insert(y.0 as usize);
                }
            }
        }
        out
    }

    /// Grid-encoded pair ids (see type-level docs).
    fn safe_actions(&self, y: &StateSet) -> PairSet {
        self.pairs
            .iter()
            .filter(|(_, s)| Self::pair_inside(s, y))
            .map(|(&(x, u), _)| self.pair_id(x, u))
            .collect()
    }

    fn cpre(&self, y: &StateSet) -> StateSet {
        let mut out = StateSet::zeros(self.n_states);
        for (&(x, _), succs) in &self.pairs {
            if !out.contains(x.0 as usize) && Self::pair_inside(succs, y) {
                out.insert(x.0 as usize);
            }
        }
        out
    }

    /// Layered CPre fixed point: round k adds every non-winning state with a pair whose
    /// successors all lie in W_{k-1}; those states get rank k and the lowest such
    /// (grid) pair id as their chosen action.
    fn reachability(&self, target: &StateSet, constraint: Option<&StateSet>) -> ReachResult {
        let mut winning = target.clone();
        let mut rank = vec![usize::MAX; self.n_states];
        for x in target.iter() {
            rank[x] = 0;
        }
        let mut chosen = HashMap::new();
        for round in 1.. {
            let mut layer: HashMap<StateId, PairId> = HashMap::new();
            for (&(x, u), succs) in &self.pairs {
                let xi = x.0 as usize;
                if winning.contains(xi)
                    || constraint.is_some_and(|c| !c.contains(xi))
                    || !Self::pair_inside(succs, &winning)
                {
                    continue;
                }
                let p = self.pair_id(x, u);
                layer.entry(x).and_modify(|q| *q = (*q).min(p)).or_insert(p);
            }
            if layer.is_empty() {
                break;
            }
            for (x, p) in layer {
                winning.insert(x.0 as usize);
                rank[x.0 as usize] = round;
                chosen.insert(x, p);
            }
        }
        ReachResult {
            winning,
            rank,
            chosen,
        }
    }

    /// Greatest fixed point Y_{k+1} = Y_k ∩ CPre(Y_k).
    fn invariance(&self, safe: &StateSet) -> SafetyResult {
        let mut invariant = safe.clone();
        loop {
            let mut next = self.cpre(&invariant);
            next.intersect_with(&invariant);
            if next == invariant {
                return SafetyResult { invariant };
            }
            invariant = next;
        }
    }
}
