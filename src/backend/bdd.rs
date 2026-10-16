//! Symbolic decision-diagram backends on top of the `oxidd` crate (feature = "oxidd").
//!
//! [`DdBackend`] stores the transition relation T(x, u, y) as one decision diagram
//! over a binary encoding of the three id spaces and implements CPre natively by
//! quantification. It is generic over the diagram kind; two instantiations are
//! provided: [`BddBackend`] (plain reduced ordered BDDs) and [`BcddBackend`] (BDDs with
//! complement edges). oxidd ships no multi-valued (MDD) or list (LDD) diagrams, so
//! those are not available here.
//!
//! # Encoding
//! With `bx = ⌈log₂ N⌉` and `bu = ⌈log₂ A⌉` (at least 1 each), the variable order is,
//! top to bottom, `x₀ y₀ x₁ y₁ … u₀ u₁ …`, most significant bit first. Interleaving
//! source and successor bits keeps both the relation and the x ↔ y renaming small.
//!
//! # Pair ids
//! A diagram has no dense pair numbering, so — like `HashBuilder` — the backend uses
//! the grid encoding `x * A + u` wherever the ADT needs a `PairId`; decode it with
//! [`DdBackend::pair_of`]. These ids are *not* comparable with `IndexedFrozen` PairIds.
//!
//! # Cost model
//! The ADT speaks explicit `StateSet`s, so every ADT call converts its arguments into
//! diagrams and its result back (O(|set| · bits)). The `*_dd` methods expose the purely
//! symbolic operations for callers that want to stay inside the diagram.

use std::collections::HashMap;

use oxidd::bcdd::BCDDFunction;
use oxidd::bdd::BDDFunction;
use oxidd::util::AllocResult;
use oxidd::{BooleanFunctionQuant, BooleanOperator, Function, LevelNo, Manager, ManagerRef, VarNo};

use crate::semantic::types::{
    InputId, InputSet, PairId, PairSet, ReachResult, SafetyResult, StateId, StateSet,
};
use crate::semantic::ControlledTransitionAdt;

/// Sizing of the oxidd manager backing one [`DdBackend`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DdConfig {
    /// Maximum number of inner nodes; exceeding it panics.
    pub inner_node_capacity: usize,
    /// Number of apply-cache entries.
    pub apply_cache_capacity: usize,
    /// Worker threads of the manager.
    pub threads: u32,
}

impl Default for DdConfig {
    fn default() -> Self {
        Self {
            inner_node_capacity: 1 << 22,
            apply_cache_capacity: 1 << 18,
            threads: 1,
        }
    }
}

/// A decision-diagram kind usable as a [`DdBackend`] representation.
pub trait DdKind: BooleanFunctionQuant {
    /// Create a fresh manager for this diagram kind.
    fn new_manager(config: DdConfig) -> Self::ManagerRef;

    /// Level of the top variable of this function (`LevelNo::MAX` for a terminal).
    fn top_level(&self) -> LevelNo;
}

impl DdKind for BDDFunction {
    fn new_manager(c: DdConfig) -> Self::ManagerRef {
        oxidd::bdd::new_manager(c.inner_node_capacity, c.apply_cache_capacity, c.threads)
    }

    fn top_level(&self) -> LevelNo {
        self.with_manager_shared(|manager, edge| manager.get_node(edge).level())
    }
}

impl DdKind for BCDDFunction {
    fn new_manager(c: DdConfig) -> Self::ManagerRef {
        oxidd::bcdd::new_manager(c.inner_node_capacity, c.apply_cache_capacity, c.threads)
    }

    fn top_level(&self) -> LevelNo {
        self.with_manager_shared(|manager, edge| manager.get_node(edge).level())
    }
}

/// Transition store backed by plain BDDs.
pub type BddBackend = DdBackend<BDDFunction>;

/// Transition store backed by BDDs with complement edges.
pub type BcddBackend = DdBackend<BCDDFunction>;

/// Symbolic transition store: T(x, u, y) as a single decision diagram.
///
/// See the module docs for the encoding, pair ids and cost model.
pub struct DdBackend<F: DdKind> {
    n_states: usize,
    n_inputs: usize,
    /// H = number of distinct (source, input) action pairs
    n_pairs: usize,
    /// M = total number of (x, u, y) incidences
    n_incidences: usize,

    /// Variables of each id space, most significant bit first.
    x_vars: Vec<F>,
    y_vars: Vec<F>,
    u_vars: Vec<F>,
    /// Levels of the variables above, in the same (top to bottom) order.
    x_levels: Vec<LevelNo>,
    y_levels: Vec<LevelNo>,
    u_levels: Vec<LevelNo>,
    /// `x_levels` followed by `u_levels`; all x variables are above all u variables.
    xu_levels: Vec<LevelNo>,

    /// Conjunctions of variables, used as quantification sets.
    x_cube: F,
    y_cube: F,
    u_cube: F,
    xu_cube: F,
    /// ⋀ⱼ (xⱼ ↔ yⱼ), for renaming between the x and y spaces.
    eq_xy: F,

    /// Constant ⊥.
    ff: F,
    /// The transition relation T(x, u, y).
    trans: F,
    /// Admissible pairs ∃y. T(x, u, y).
    admissible: F,

    /// Universe sets returned by `states()` / `inputs()`.
    all_states: StateSet,
    all_inputs: InputSet,
}

/// Unwrap an oxidd allocation result.
fn ok<T>(r: AllocResult<T>) -> T {
    r.expect("decision-diagram node table exhausted; raise DdConfig::inner_node_capacity")
}

/// Bits needed to encode ids `0..n`, at least 1.
fn bits_for(n: usize) -> usize {
    (usize::BITS - n.saturating_sub(1).leading_zeros()).max(1) as usize
}

/// The diagram whose satisfying assignments over `vars` (top to bottom, first variable
/// = most significant key bit) are exactly `keys`. `keys` must be sorted and
/// duplicate-free. O(|keys| · |vars|) node constructions, no intermediate blow-up.
fn from_sorted_keys<F: DdKind>(keys: &[u128], vars: &[F], ff: &F, tt: &F) -> F {
    if keys.is_empty() {
        return ff.clone();
    }
    let Some((var, rest)) = vars.split_first() else {
        return tt.clone();
    };
    let bit = rest.len();
    let split = keys.partition_point(|k| (k >> bit) & 1 == 0);
    let lo = from_sorted_keys(&keys[..split], rest, ff, tt);
    let hi = from_sorted_keys(&keys[split..], rest, ff, tt);
    ok(var.ite(&hi, &lo))
}

/// Append the satisfying assignments of `f` over the variables at `levels` (top to
/// bottom) to `out`, in increasing key order. `f` must depend on those variables only.
/// O(|output| · |levels|).
fn enumerate<F: DdKind>(f: &F, levels: &[LevelNo], key: u64, out: &mut Vec<u64>) {
    if !f.satisfiable() {
        return;
    }
    let Some((&level, rest)) = levels.split_first() else {
        debug_assert!(f.valid(), "diagram depends on variables outside the group");
        out.push(key);
        return;
    };
    let top = f.top_level();
    debug_assert!(
        top >= level,
        "diagram depends on variables outside the group"
    );
    if top == level {
        let (hi, lo) = f.cofactors().expect("inner node has cofactors");
        enumerate(&lo, rest, key << 1, out);
        enumerate(&hi, rest, key << 1 | 1, out);
    } else {
        // The variable at `level` is a don't-care here.
        enumerate(f, rest, key << 1, out);
        enumerate(f, rest, key << 1 | 1, out);
    }
}

impl<F: DdKind> DdBackend<F> {
    /// Build the backend from a triple stream (duplicates allowed) with the default
    /// manager sizing.
    pub fn from_triples(
        n_states: usize,
        n_inputs: usize,
        triples: impl IntoIterator<Item = (StateId, InputId, StateId)>,
    ) -> Self {
        Self::with_config(n_states, n_inputs, triples, DdConfig::default())
    }

    /// Like [`DdBackend::from_triples`], with explicit manager sizing.
    pub fn with_config(
        n_states: usize,
        n_inputs: usize,
        triples: impl IntoIterator<Item = (StateId, InputId, StateId)>,
        config: DdConfig,
    ) -> Self {
        let (bx, bu) = (bits_for(n_states), bits_for(n_inputs));
        let manager = F::new_manager(config);

        // Variables are created top to bottom: x₀ y₀ x₁ y₁ … u₀ u₁ …
        let (x_vars, y_vars, u_vars, ff, tt) = manager.with_manager_exclusive(|m| {
            m.add_vars((2 * bx + bu) as VarNo);
            let var = |i: usize| ok(F::var(m, i as VarNo));
            let xs: Vec<F> = (0..bx).map(|j| var(2 * j)).collect();
            let ys: Vec<F> = (0..bx).map(|j| var(2 * j + 1)).collect();
            let us: Vec<F> = (0..bu).map(|j| var(2 * bx + j)).collect();
            (xs, ys, us, F::f(m), F::t(m))
        });
        let levels = |vars: &[F]| vars.iter().map(F::top_level).collect::<Vec<_>>();
        let (x_levels, y_levels, u_levels) = (levels(&x_vars), levels(&y_vars), levels(&u_vars));
        let xu_levels = [x_levels.as_slice(), u_levels.as_slice()].concat();

        let conj = |vars: &[F]| vars.iter().fold(tt.clone(), |acc, v| ok(acc.and(v)));
        let (x_cube, y_cube, u_cube) = (conj(&x_vars), conj(&y_vars), conj(&u_vars));
        let xu_cube = ok(x_cube.and(&u_cube));
        let eq_xy = x_vars
            .iter()
            .zip(&y_vars)
            .rev()
            .fold(tt.clone(), |acc, (x, y)| ok(ok(x.equiv(y)).and(&acc)));

        // One key per triple, bits laid out in variable order.
        let mut keys: Vec<u128> = triples
            .into_iter()
            .map(|(x, u, y)| {
                debug_assert!((x.0 as usize) < n_states, "state id out of bounds");
                debug_assert!((u.0 as usize) < n_inputs, "input id out of bounds");
                debug_assert!((y.0 as usize) < n_states, "successor id out of bounds");
                let mut key = 0u128;
                for j in (0..bx).rev() {
                    key = key << 2 | ((x.0 >> j) as u128 & 1) << 1 | ((y.0 >> j) as u128 & 1);
                }
                key << bu | u.0 as u128
            })
            .collect();
        keys.sort_unstable();
        keys.dedup();
        let all_vars: Vec<F> = x_vars
            .iter()
            .zip(&y_vars)
            .flat_map(|(x, y)| [x.clone(), y.clone()])
            .chain(u_vars.iter().cloned())
            .collect();
        let trans = from_sorted_keys(&keys, &all_vars, &ff, &tt);
        let admissible = ok(trans.exists(&y_cube));

        let mut backend = Self {
            n_states,
            n_inputs,
            n_pairs: 0,
            n_incidences: keys.len(),
            x_vars,
            y_vars,
            u_vars,
            x_levels,
            y_levels,
            u_levels,
            xu_levels,
            x_cube,
            y_cube,
            u_cube,
            xu_cube,
            eq_xy,
            ff,
            trans,
            admissible,
            all_states: StateSet::ones(n_states),
            all_inputs: InputSet::ones(n_inputs),
        };
        backend.n_pairs = backend
            .keys_of(&backend.admissible, &backend.xu_levels)
            .len();
        backend
    }

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

    /// Number of diagram nodes representing the transition relation (the symbolic
    /// counterpart of the CSR array lengths).
    pub fn transition_node_count(&self) -> usize {
        self.trans.node_count()
    }

    /// The transition relation T(x, u, y).
    pub fn transition_dd(&self) -> &F {
        &self.trans
    }

    /// Grid-encoded pair id of (x, u) — see the module docs.
    pub fn pair_id(&self, x: StateId, u: InputId) -> PairId {
        let id = x.0 as u64 * self.n_inputs as u64 + u.0 as u64;
        PairId(u32::try_from(id).expect("grid pair id x*A+u overflows u32"))
    }

    /// Decode a grid-encoded pair id back into (x, u).
    pub fn pair_of(&self, p: PairId) -> (StateId, InputId) {
        let a = self.n_inputs as u32;
        (StateId(p.0 / a), InputId(p.0 % a))
    }

    // ── Explicit ↔ symbolic conversions ───────────────────────────────────────

    /// `set` as a diagram over the x (source) variables.
    pub fn state_set_x_dd(&self, set: &StateSet) -> F {
        self.set_dd(set, &self.x_vars)
    }

    /// `set` as a diagram over the y (successor) variables.
    pub fn state_set_y_dd(&self, set: &StateSet) -> F {
        self.set_dd(set, &self.y_vars)
    }

    /// The explicit state set of a diagram over the x variables.
    pub fn to_state_set(&self, f: &F) -> StateSet {
        let mut out = StateSet::zeros(self.n_states);
        for x in self.keys_of(f, &self.x_levels) {
            out.insert(x as usize);
        }
        out
    }

    /// Rename a diagram over the x variables into the same set over the y variables.
    pub fn rename_x_to_y(&self, f: &F) -> F {
        ok(f.apply_exists(BooleanOperator::And, &self.eq_xy, &self.x_cube))
    }

    fn set_dd(&self, set: &DenseSet, vars: &[F]) -> F {
        let keys: Vec<u128> = set.iter().map(|i| i as u128).collect();
        from_sorted_keys(&keys, vars, &self.ff, &self.tt())
    }

    /// The single assignment `value` over `vars`, as a conjunction of literals.
    fn minterm(&self, value: u32, vars: &[F]) -> F {
        debug_assert!(value as u64 >> vars.len() == 0, "id out of bounds");
        from_sorted_keys(&[value as u128], vars, &self.ff, &self.tt())
    }

    /// Constant ⊤.
    fn tt(&self) -> F {
        ok(self.ff.not())
    }

    fn keys_of(&self, f: &F, levels: &[LevelNo]) -> Vec<u64> {
        let mut out = Vec::new();
        enumerate(f, levels, 0, &mut out);
        out
    }

    /// Split a key over `xu_levels` into (x, u).
    fn split_xu(&self, key: u64) -> (StateId, InputId) {
        let bu = self.u_levels.len();
        (
            StateId((key >> bu) as u32),
            InputId((key & ((1 << bu) - 1)) as u32),
        )
    }

    // ── Symbolic operations ───────────────────────────────────────────────────

    /// Pairs (x, u) whose whole successor set lies in `w_y` (a set over the y
    /// variables): `A(x,u) ∧ ¬∃y. T(x,u,y) ∧ ¬W(y)`.
    pub fn safe_pairs_dd(&self, w_y: &F) -> F {
        let escaping =
            ok(self
                .trans
                .apply_exists(BooleanOperator::And, &ok(w_y.not()), &self.y_cube));
        ok(self.admissible.and(&ok(escaping.not())))
    }

    /// Native CPre: `∃u. safe_pairs(W)`, mapping a set over the y variables to a set
    /// over the x variables.
    pub fn cpre_dd(&self, w_y: &F) -> F {
        ok(self.safe_pairs_dd(w_y).exists(&self.u_cube))
    }
}

/// Explicit sets all share the dense bit-vector representation.
type DenseSet = StateSet;

impl<F: DdKind> ControlledTransitionAdt for DdBackend<F> {
    fn states(&self) -> &StateSet {
        &self.all_states
    }

    fn inputs(&self) -> &InputSet {
        &self.all_inputs
    }

    /// One root-to-terminal walk of T: O(2·bx + bu).
    fn contains(&self, x: StateId, u: InputId, y: StateId) -> bool {
        // Variable numbers follow the creation order x₀ y₀ x₁ y₁ … u₀ u₁ …
        let (bx, bu) = (self.x_vars.len(), self.u_vars.len());
        let bit = |v: u32, n: usize, j: usize| (v as u64 >> (n - 1 - j)) & 1 == 1;
        if x.0 as usize >= self.n_states || y.0 as usize >= self.n_states {
            return false;
        }
        self.trans.eval(
            (0..bx)
                .flat_map(|j| {
                    [
                        ((2 * j) as VarNo, bit(x.0, bx, j)),
                        ((2 * j + 1) as VarNo, bit(y.0, bx, j)),
                    ]
                })
                .chain((0..bu).map(|j| ((2 * bx + j) as VarNo, bit(u.0, bu, j)))),
        )
    }

    /// Successors in increasing id order.
    fn successors(&self, x: StateId, u: InputId) -> Box<dyn Iterator<Item = StateId> + '_> {
        let xu = ok(self
            .minterm(x.0, &self.x_vars)
            .and(&self.minterm(u.0, &self.u_vars)));
        let succ = ok(self.trans.restrict(&xu));
        let keys = self.keys_of(&succ, &self.y_levels);
        Box::new(keys.into_iter().map(|y| StateId(y as u32)))
    }

    /// Predecessor pairs in increasing (source, input) order.
    fn predecessor_pairs(&self, y: StateId) -> Box<dyn Iterator<Item = (StateId, InputId)> + '_> {
        let pre = ok(self.trans.restrict(&self.minterm(y.0, &self.y_vars)));
        let keys = self.keys_of(&pre, &self.xu_levels);
        Box::new(keys.into_iter().map(|k| self.split_xu(k)))
    }

    fn admissible_inputs(&self, x: StateId) -> Box<dyn Iterator<Item = InputId> + '_> {
        let at_x = ok(self.admissible.restrict(&self.minterm(x.0, &self.x_vars)));
        let keys = self.keys_of(&at_x, &self.u_levels);
        Box::new(keys.into_iter().map(|u| InputId(u as u32)))
    }

    /// `∃x,u. T(x,u,y) ∧ S(x) ∧ filter(u)`.
    fn post(&self, s: &StateSet, filter: Option<&InputSet>) -> StateSet {
        let mut from = self.state_set_x_dd(s);
        if let Some(f) = filter {
            from = ok(from.and(&self.set_dd(f, &self.u_vars)));
        }
        let image = ok(self
            .trans
            .apply_exists(BooleanOperator::And, &from, &self.xu_cube));
        let mut out = StateSet::zeros(self.n_states);
        for y in self.keys_of(&image, &self.y_levels) {
            out.insert(y as usize);
        }
        out
    }

    /// Grid-encoded pair ids (see module docs).
    fn safe_actions(&self, y: &StateSet) -> PairSet {
        let safe = self.safe_pairs_dd(&self.state_set_y_dd(y));
        self.keys_of(&safe, &self.xu_levels)
            .into_iter()
            .map(|k| {
                let (x, u) = self.split_xu(k);
                self.pair_id(x, u)
            })
            .collect()
    }

    fn cpre(&self, y: &StateSet) -> StateSet {
        self.to_state_set(&self.cpre_dd(&self.state_set_y_dd(y)))
    }

    /// Symbolic layered fixed point W_k = W_{k-1} ∪ (CPre(W_{k-1}) ∩ C).
    ///
    /// Round k adds the states of layer k with rank k; their chosen action is the
    /// lowest admissible input whose successors all lie in W_{k-1} (as a grid pair id).
    /// Ranks and chosen actions are explicit outputs, so each layer is enumerated once.
    fn reachability(&self, target: &StateSet, constraint: Option<&StateSet>) -> ReachResult {
        let mut winning = target.clone();
        let mut rank = vec![usize::MAX; self.n_states];
        for x in target.iter() {
            rank[x] = 0;
        }
        let mut chosen = HashMap::new();

        let allowed = constraint.map(|c| self.state_set_x_dd(c));
        let mut w_x = self.state_set_x_dd(target);
        let mut w_y = self.state_set_y_dd(target);
        for round in 1.. {
            // Safe pairs at allowed, not yet winning sources.
            let mut layer = ok(self.safe_pairs_dd(&w_y).and(&ok(w_x.not())));
            if let Some(c) = &allowed {
                layer = ok(layer.and(c));
            }
            if !layer.satisfiable() {
                break;
            }
            // Keys ascend in (x, u), so the first key of each state has its lowest input.
            let mut last = None;
            for key in self.keys_of(&layer, &self.xu_levels) {
                let (x, u) = self.split_xu(key);
                if last == Some(x) {
                    continue;
                }
                last = Some(x);
                rank[x.0 as usize] = round;
                winning.insert(x.0 as usize);
                chosen.insert(x, self.pair_id(x, u));
            }
            let new_x = ok(layer.exists(&self.u_cube));
            w_y = ok(w_y.or(&self.rename_x_to_y(&new_x)));
            w_x = ok(w_x.or(&new_x));
        }

        ReachResult {
            winning,
            rank,
            chosen,
        }
    }

    /// Symbolic greatest fixed point Y_{k+1} = Y_k ∩ CPre(Y_k).
    fn invariance(&self, safe: &StateSet) -> SafetyResult {
        let mut inv = self.state_set_x_dd(safe);
        loop {
            let next = ok(inv.and(&self.cpre_dd(&self.rename_x_to_y(&inv))));
            if next == inv {
                break;
            }
            inv = next;
        }
        SafetyResult {
            invariant: self.to_state_set(&inv),
        }
    }

    fn has_native_cpre(&self) -> bool {
        true
    }
}
