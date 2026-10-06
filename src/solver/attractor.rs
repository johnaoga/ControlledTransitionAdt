//! Reachability attractor solver — counter-based event-driven algorithm.
//!
//! See specs.md § F6 — Reachability Attractor Solver
//! Blueprint Listing 2, Proposition 4.1, Appendix A.2

use std::collections::HashMap;

use crate::backend::indexed_frozen::IndexedFrozen;
use crate::semantic::types::{PairId, ReachResult, StateSet};

/// Compute the reachability winning region and a rank-based progress controller.
///
/// Equivalent to [`reachability_constrained`] with no constraint.
pub fn reachability(target: &StateSet, store: &IndexedFrozen) -> ReachResult {
    reachability_constrained(target, None, store)
}

/// Reach-while-stay: like [`reachability`], but only states in `constraint` (or in
/// `target`) may join the winning set. `None` means unconstrained.
///
/// # Algorithm (Listing 2, layer-synchronous; Phase 1 — no reverse index)
///
/// For each pair p, `remaining[p]` counts the successors of p that are not yet winning.
/// When `remaining[p]` hits 0, p "fires" and its source becomes winning.
///
/// Without a reverse index, finding the pairs that have a newly winning state y as a
/// successor requires a scan over pairs. Rather than one scan per dequeued state
/// (O(N(M+H))), the solver processes a whole rank layer per scan: in round k it
/// decrements `remaining[p]` by the number of successors that won in round k-1. Total
/// cost is O(k(M+H)) where k = number of rounds = max rank + 1. Pairs whose source has
/// already won are dropped from the scan list, so later rounds get cheaper.
///
/// Phase 2 upgrade: with `rev_off` / `rev_pair`, each newly winning y touches only its
/// O(d⁻(y)) predecessor pairs, giving O(M+H+N) total.
///
/// # Ranks and controller (Proposition 4.1)
/// A pair fires in round k exactly when its last successor won in round k-1, so
/// `rank[x] = k = 1 + max(rank[z] : z ∈ Succ(chosen[x]))`. This is the optimal
/// worst-case number of steps to the target. Pairs are scanned in increasing PairId
/// order, so `chosen[x]` is the lowest PairId that fires at x's rank (deterministic).
/// Target states have rank 0 and no chosen action. Non-winning states have rank
/// `usize::MAX`.
pub fn reachability_constrained(
    target: &StateSet,
    constraint: Option<&StateSet>,
    store: &IndexedFrozen,
) -> ReachResult {
    let n = store.n_states;
    let mut winning = target.clone();
    let mut rank = vec![usize::MAX; n];
    let mut frontier_len = 0usize;
    for x in target.iter() {
        rank[x] = 0;
        frontier_len += 1;
    }

    // remaining[p] = |Succ(p)|; only pairs whose source may still win are scanned.
    let mut remaining: Vec<u32> = store
        .pair_succ_off
        .windows(2)
        .map(|w| (w[1] - w[0]) as u32)
        .collect();
    let mut live: Vec<u32> = (0..store.n_pairs as u32)
        .filter(|&p| {
            let x = store.pair_source[p as usize].0 as usize;
            rank[x] == usize::MAX && constraint.is_none_or(|c| c.contains(x))
        })
        .collect();

    let mut chosen = HashMap::new();
    let mut round = 0usize;
    while frontier_len > 0 && !live.is_empty() {
        let prev = round;
        round += 1;
        frontier_len = 0;
        live.retain(|&p| {
            let x = store.pair_source[p as usize];
            let xi = x.0 as usize;
            if rank[xi] != usize::MAX {
                // Source already won (earlier round or a lower PairId this round).
                return false;
            }
            // Only the previous layer counts: states winning in this round carry
            // rank == round and are therefore excluded.
            let newly = store
                .succs_for_pair(PairId(p))
                .iter()
                .filter(|s| rank[s.0 as usize] == prev)
                .count() as u32;
            let r = &mut remaining[p as usize];
            *r -= newly;
            if *r > 0 {
                return true;
            }
            rank[xi] = round;
            winning.insert(xi);
            chosen.insert(x, PairId(p));
            frontier_len += 1;
            false
        });
    }

    ReachResult {
        winning,
        rank,
        chosen,
    }
}
