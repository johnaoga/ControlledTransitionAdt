//! Reference CPre and reference solvers — the conformance oracle.
//!
//! See specs.md § F2 — Reference CPre and Reference Solvers
//! These are FULLY IMPLEMENTED (not stubs). They are slow but obviously correct.
//! Never benchmark these as competitors; use only for differential testing.

use crate::semantic::types::{InputId, PairId, ReachResult, SafetyResult, StateId, StateSet};
use std::collections::{HashSet, VecDeque};

/// Reference CPre: { x ∈ X : ∃u ∈ U, Succ(x,u) ⊆ Y }.
///
/// Operates on a flat triple set — no indexing required.
/// Direct port of blueprint §6.7 (Python → Rust).
///
/// # Arguments
/// - `triples`: the transition relation as a set of (x, u, y).
/// - `y`: the target state set Y.
/// - `n_states`: total number of states (bounds the state id space).
pub fn cpre_reference(
    triples: &HashSet<(StateId, InputId, StateId)>,
    y: &StateSet,
    n_states: usize,
) -> StateSet {
    // Collect all distinct (x, u) pairs and their successors.
    // For each pair, check if ALL successors lie in Y.
    // If so, x ∈ CPre(Y).

    // Build pair -> successor sets.
    let mut pair_succs: std::collections::HashMap<(StateId, InputId), HashSet<StateId>> =
        std::collections::HashMap::new();
    for &(x, u, succ) in triples {
        pair_succs.entry((x, u)).or_default().insert(succ);
    }

    let mut result = StateSet::zeros(n_states);
    for ((x, _u), succs) in &pair_succs {
        // Check all successors of (x, u) lie in Y.
        if succs.iter().all(|s| y.contains(s.0 as usize)) {
            result.insert(x.0 as usize);
        }
    }
    result
}

/// Reference reachability: compute winning region by iterating CPre until fixed point.
///
/// Returns the set of states from which `target` is reachable under a controlled strategy.
pub fn reachability_reference(
    triples: &HashSet<(StateId, InputId, StateId)>,
    target: &StateSet,
    n_states: usize,
) -> ReachResult {
    let mut winning = target.clone();
    // Fixed-point iteration: W_{k+1} = W_k ∪ CPre(W_k)
    loop {
        let new_cpre = cpre_reference(triples, &winning, n_states);
        let mut changed = false;
        for id in 0..n_states {
            if new_cpre.contains(id) && !winning.contains(id) {
                winning.insert(id);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    // Rank assignment: BFS from target.
    let mut rank = vec![usize::MAX; n_states];
    let mut queue: VecDeque<usize> = VecDeque::new();
    for id in target.iter() {
        rank[id] = 0;
        queue.push_back(id);
    }
    // Build predecessor map from triples (for rank BFS).
    let mut preds: Vec<Vec<usize>> = vec![vec![]; n_states];
    for &(x, _u, succ) in triples {
        preds[succ.0 as usize].push(x.0 as usize);
    }
    while let Some(y) = queue.pop_front() {
        for &x in &preds[y] {
            if winning.contains(x) && rank[x] == usize::MAX {
                rank[x] = rank[y] + 1;
                queue.push_back(x);
            }
        }
    }

    // Chosen action: pick any (x, u) pair with all successors in winning.
    let mut pair_succs: std::collections::HashMap<(StateId, InputId), HashSet<StateId>> =
        std::collections::HashMap::new();
    for &(x, u, succ) in triples {
        pair_succs.entry((x, u)).or_default().insert(succ);
    }
    let mut chosen = std::collections::HashMap::new();
    // We assign fake PairIds (just sequential) since reference has no CSR.
    for (pair_id, ((x, _u), succs)) in pair_succs.iter().enumerate() {
        if winning.contains(x.0 as usize) && succs.iter().all(|s| winning.contains(s.0 as usize)) {
            // Only insert if not already chosen (first valid pair wins).
            chosen.entry(*x).or_insert(PairId(pair_id as u32));
        }
    }

    ReachResult {
        winning,
        rank,
        chosen,
    }
}

/// Reference invariance: maximal controlled invariant subset of `safe`.
///
/// Iterates CPre ∩ safe until fixed point.
pub fn invariance_reference(
    triples: &HashSet<(StateId, InputId, StateId)>,
    safe: &StateSet,
    n_states: usize,
) -> SafetyResult {
    let mut invariant = safe.clone();
    // Y_{k+1} = CPre(Y_k) ∩ Y_k
    loop {
        let cpre = cpre_reference(triples, &invariant, n_states);
        let mut new_inv = StateSet::zeros(n_states);
        let mut changed = false;
        for id in 0..n_states {
            if invariant.contains(id) && cpre.contains(id) {
                new_inv.insert(id);
            }
        }
        // Check convergence.
        for id in 0..n_states {
            if invariant.contains(id) != new_inv.contains(id) {
                changed = true;
                break;
            }
        }
        invariant = new_inv;
        if !changed {
            break;
        }
    }
    SafetyResult { invariant }
}
