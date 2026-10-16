//! CSR frozen transition store: forward incidence plus an optional reverse index.
//!
//! See specs.md § F5 — IndexedFrozen Backend (Forward CSR); the reverse CSR
//! (`rev_off` / `rev_pair`) is the Phase 2 addition.

use crate::semantic::types::{
    InputId, InputSet, PairId, PairSet, ReachResult, SafetyResult, StateId, StateSet,
};
use crate::semantic::ControlledTransitionAdt;

/// Read-optimized frozen transition store using a compact CSR-like layout.
///
/// After construction via `HashBuilder::freeze()`, the relation is immutable.
///
/// # Layout (blueprint §6.2)
/// - `state_pair_off[x]..state_pair_off[x+1]` → range of PairIds for state x
/// - `pair_source[p]`, `pair_input[p]` → metadata for pair p
/// - `pair_succ_off[p]..pair_succ_off[p+1]` → range of successors for pair p
/// - `succ[i]` → StateId
///
/// # Invariants (established by `freeze`)
/// - Pairs are sorted by `(source, input)`, so the inputs inside one state's pair range
///   are strictly increasing (binary-searchable).
/// - Each pair's successor slice is non-empty, sorted and duplicate-free.
///
/// # Reverse incidence (Phase 2)
/// - `rev_pair[rev_off[y]..rev_off[y+1]]` → PairIds having y as a successor, in
///   increasing PairId order.
///
/// `HashBuilder::freeze` builds it (one extra counting pass over `succ`, M + N + 1
/// extra words); the solvers then run in O(M+H+N). `freeze_forward_only` /
/// [`IndexedFrozen::without_reverse`] give the Phase 1 layout, on which the solvers
/// fall back to the O(k(M+H)) scans.
pub struct IndexedFrozen {
    pub(crate) n_states: usize,
    pub(crate) n_inputs: usize,
    /// H = number of distinct (source, input) action pairs
    pub(crate) n_pairs: usize,
    /// M = total number of (x, u, y) incidences
    pub(crate) n_incidences: usize,

    /// State → pair range. Length N+1. state_pair_off[x]..state_pair_off[x+1] gives PairIds.
    pub(crate) state_pair_off: Vec<u64>,

    /// Pair metadata. Length H each.
    pub(crate) pair_source: Vec<StateId>,
    pub(crate) pair_input: Vec<InputId>,

    /// Pair → successor range. Length H+1.
    pub(crate) pair_succ_off: Vec<u64>,

    /// Successor list. Length M.
    pub(crate) succ: Vec<StateId>,

    /// Reverse incidence: state → range into `rev_pair`. Length N+1 when present.
    pub(crate) rev_off: Option<Vec<u64>>,
    /// Pairs having a given state as successor. Length M when present.
    pub(crate) rev_pair: Option<Vec<PairId>>,

    /// Universe sets returned by `states()` / `inputs()`.
    pub(crate) all_states: StateSet,
    pub(crate) all_inputs: InputSet,
}

impl IndexedFrozen {
    /// Number of states N.
    pub fn n_states(&self) -> usize {
        self.n_states
    }

    /// Number of inputs A.
    pub fn n_inputs(&self) -> usize {
        self.n_inputs
    }

    /// Number of action pairs H.
    pub fn n_pairs(&self) -> usize {
        self.n_pairs
    }

    /// Number of incidences M.
    pub fn n_incidences(&self) -> usize {
        self.n_incidences
    }

    /// True if the reverse incidence index is present.
    pub fn has_reverse(&self) -> bool {
        self.rev_off.is_some()
    }

    /// Build the reverse incidence index by counting sort over `succ`. O(M+H+N);
    /// no-op if already present.
    pub fn build_reverse_index(&mut self) {
        if self.has_reverse() {
            return;
        }
        let n = self.n_states;
        let mut rev_off = vec![0u64; n + 1];
        for y in &self.succ {
            rev_off[y.0 as usize + 1] += 1;
        }
        for y in 0..n {
            rev_off[y + 1] += rev_off[y];
        }
        let mut cursor: Vec<u64> = rev_off[..n].to_vec();
        let mut rev_pair = vec![PairId(0); self.n_incidences];
        // Pairs are visited in increasing order, so each state's list ends up sorted.
        for p in 0..self.n_pairs as u32 {
            for y in self.succs_for_pair(PairId(p)) {
                let c = &mut cursor[y.0 as usize];
                rev_pair[*c as usize] = PairId(p);
                *c += 1;
            }
        }
        self.rev_off = Some(rev_off);
        self.rev_pair = Some(rev_pair);
    }

    /// This store with the reverse incidence index built.
    pub fn with_reverse(mut self) -> Self {
        self.build_reverse_index();
        self
    }

    /// This store with the reverse incidence index dropped (Phase 1 layout).
    pub fn without_reverse(mut self) -> Self {
        self.rev_off = None;
        self.rev_pair = None;
        self
    }

    /// The reverse CSR arrays `(rev_off, rev_pair)`, if present.
    #[inline]
    pub(crate) fn reverse_index(&self) -> Option<(&[u64], &[PairId])> {
        Some((self.rev_off.as_deref()?, self.rev_pair.as_deref()?))
    }

    /// Pairs having y as a successor, in increasing PairId order; `None` without a
    /// reverse index.
    pub fn rev_pairs(&self, y: StateId) -> Option<&[PairId]> {
        let (off, pairs) = self.reverse_index()?;
        Some(&pairs[off[y.0 as usize] as usize..off[y.0 as usize + 1] as usize])
    }

    /// Returns the pair id range for state x as (start, end) into pair_source/pair_input.
    pub fn pair_range_for_state(&self, x: StateId) -> (usize, usize) {
        let start = self.state_pair_off[x.0 as usize] as usize;
        let end = self.state_pair_off[x.0 as usize + 1] as usize;
        (start, end)
    }

    /// Slice of successor StateIds for pair p (sorted, duplicate-free).
    #[inline]
    pub fn succs_for_pair(&self, p: PairId) -> &[StateId] {
        let start = self.pair_succ_off[p.0 as usize] as usize;
        let end = self.pair_succ_off[p.0 as usize + 1] as usize;
        &self.succ[start..end]
    }

    /// Source state of pair p.
    #[inline]
    pub fn pair_source(&self, p: PairId) -> StateId {
        self.pair_source[p.0 as usize]
    }

    /// Input of pair p.
    #[inline]
    pub fn pair_input(&self, p: PairId) -> InputId {
        self.pair_input[p.0 as usize]
    }

    /// Input id of every pair, indexed by PairId.
    pub fn pair_inputs(&self) -> &[InputId] {
        &self.pair_input
    }

    /// The PairId of (x, u), or `None` if u is not admissible at x. O(log d_x).
    pub fn find_pair(&self, x: StateId, u: InputId) -> Option<PairId> {
        let (start, end) = self.pair_range_for_state(x);
        self.pair_input[start..end]
            .binary_search(&u)
            .ok()
            .map(|i| PairId((start + i) as u32))
    }

    /// True iff every successor of pair p lies in `y`.
    #[inline]
    pub(crate) fn pair_inside(&self, p: usize, y: &StateSet) -> bool {
        self.succs_for_pair(PairId(p as u32))
            .iter()
            .all(|s| y.contains(s.0 as usize))
    }
}

impl ControlledTransitionAdt for IndexedFrozen {
    fn states(&self) -> &StateSet {
        &self.all_states
    }

    fn inputs(&self) -> &InputSet {
        &self.all_inputs
    }

    /// O(log d_x + log d⁺): binary search for the pair, then in its successor slice.
    fn contains(&self, x: StateId, u: InputId, y: StateId) -> bool {
        self.find_pair(x, u)
            .is_some_and(|p| self.succs_for_pair(p).binary_search(&y).is_ok())
    }

    fn successors(&self, x: StateId, u: InputId) -> Box<dyn Iterator<Item = StateId> + '_> {
        let slice = match self.find_pair(x, u) {
            Some(p) => self.succs_for_pair(p),
            None => &[],
        };
        Box::new(slice.iter().copied())
    }

    /// O(1 + d⁻(y)) with the reverse index; otherwise a full scan over all pairs.
    /// Either way the pairs come out in increasing (source, input) order.
    fn predecessor_pairs(&self, y: StateId) -> Box<dyn Iterator<Item = (StateId, InputId)> + '_> {
        if let Some(pairs) = self.rev_pairs(y) {
            return Box::new(
                pairs
                    .iter()
                    .map(|&p| (self.pair_source(p), self.pair_input(p))),
            );
        }
        Box::new((0..self.n_pairs).filter_map(move |p| {
            self.succs_for_pair(PairId(p as u32))
                .binary_search(&y)
                .ok()
                .map(|_| (self.pair_source[p], self.pair_input[p]))
        }))
    }

    fn admissible_inputs(&self, x: StateId) -> Box<dyn Iterator<Item = InputId> + '_> {
        let (start, end) = self.pair_range_for_state(x);
        Box::new(self.pair_input[start..end].iter().copied())
    }

    fn post(&self, s: &StateSet, filter: Option<&InputSet>) -> StateSet {
        let mut out = StateSet::zeros(self.n_states);
        for x in s.iter() {
            let (start, end) = self.pair_range_for_state(StateId(x as u32));
            for p in start..end {
                if filter.is_some_and(|f| !f.contains(self.pair_input[p].0 as usize)) {
                    continue;
                }
                for y in self.succs_for_pair(PairId(p as u32)) {
                    out.insert(y.0 as usize);
                }
            }
        }
        out
    }

    /// O(M + H) full scan.
    fn safe_actions(&self, y: &StateSet) -> PairSet {
        (0..self.n_pairs)
            .filter(|&p| self.pair_inside(p, y))
            .map(|p| PairId(p as u32))
            .collect()
    }

    /// O(M + H) full scan, short-circuiting per state once one safe pair is found.
    fn cpre(&self, y: &StateSet) -> StateSet {
        let mut out = StateSet::zeros(self.n_states);
        for x in 0..self.n_states {
            let (start, end) = (
                self.state_pair_off[x] as usize,
                self.state_pair_off[x + 1] as usize,
            );
            if (start..end).any(|p| self.pair_inside(p, y)) {
                out.insert(x);
            }
        }
        out
    }

    fn reachability(&self, target: &StateSet, constraint: Option<&StateSet>) -> ReachResult {
        crate::solver::attractor::reachability_constrained(target, constraint, self)
    }

    fn invariance(&self, safe: &StateSet) -> SafetyResult {
        crate::solver::invariance::invariance(safe, self)
    }

    fn has_reverse_incidence(&self) -> bool {
        self.has_reverse()
    }
}
