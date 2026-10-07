//! Reference CPre and reference solvers — the conformance oracle.
//!
//! See specs.md § F2 — Reference CPre and Reference Solvers
//! These are FULLY IMPLEMENTED (not stubs). They are slow but obviously correct.
//! Never benchmark these as competitors; use only for differential testing.

use crate::semantic::types::{InputId, PairId, ReachResult, SafetyResult, StateId, StateSet};
use std::collections::{BTreeMap, HashMap, HashSet};

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
    let mut pair_succs: HashMap<(StateId, InputId), HashSet<StateId>> = HashMap::new();
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
///
/// Layers: W_0 = target, W_k = W_{k-1} ∪ CPre(W_{k-1}).
/// - `rank[x]` = least k with x ∈ W_k (`usize::MAX` if never), i.e. the optimal
///   worst-case number of steps to the target.
/// - `chosen[x]` (non-target winning x only) = the lowest canonical PairId at x whose
///   successors all lie in W_{rank[x]-1}. Canonical PairIds number the distinct
///   (x, u) pairs in sorted (x, u) order — the same numbering `HashBuilder::freeze`
///   assigns — so `chosen` is directly comparable with the frozen store's controller.
pub fn reachability_reference(
    triples: &HashSet<(StateId, InputId, StateId)>,
    target: &StateSet,
    n_states: usize,
) -> ReachResult {
    // Canonical pair numbering: sorted distinct (x, u) pairs with their successor sets.
    let mut pair_succs: BTreeMap<(StateId, InputId), HashSet<StateId>> = BTreeMap::new();
    for &(x, u, succ) in triples {
        pair_succs.entry((x, u)).or_default().insert(succ);
    }

    let mut winning = target.clone();
    let mut rank = vec![usize::MAX; n_states];
    for id in target.iter() {
        rank[id] = 0;
    }
    let mut chosen = HashMap::new();

    for k in 1.. {
        // Layer k: states outside W_{k-1} that are in CPre(W_{k-1}).
        let layer = cpre_reference(triples, &winning, n_states);
        let new: Vec<usize> = layer.iter().filter(|&x| !winning.contains(x)).collect();
        if new.is_empty() {
            break;
        }
        for &x in &new {
            rank[x] = k;
            // Lowest canonical pair at x with all successors in W_{k-1}.
            let pair_id = pair_succs
                .iter()
                .position(|(&(px, _), succs)| {
                    px.0 as usize == x && succs.iter().all(|s| winning.contains(s.0 as usize))
                })
                .expect("x ∈ CPre(W) implies a pair at x inside W");
            chosen.insert(StateId(x as u32), PairId(pair_id as u32));
        }
        // Grow W only after the whole layer is decided, so every choice above
        // refers to W_{k-1}.
        for x in new {
            winning.insert(x);
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
