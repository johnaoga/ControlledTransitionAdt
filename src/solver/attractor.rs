//! Reachability attractor solver — counter-based event-driven algorithm.
//!
//! See specs.md § F6 — Reachability Attractor Solver
//! Blueprint Listing 2, Proposition 4.1, Appendix A.2

use std::collections::HashMap;

use crate::backend::indexed_frozen::IndexedFrozen;
use crate::semantic::types::{PairId, ReachResult, StateId, StateSet};

/// Compute the reachability winning region and a rank-based progress controller.
///
/// Equivalent to [`reachability_constrained`] with no constraint.
pub fn reachability(target: &StateSet, store: &IndexedFrozen) -> ReachResult {
    reachability_constrained(target, None, store)
}

/// Reach-while-stay: like [`reachability`], but only states in `constraint` (or in
/// `target`) may join the winning set. `None` means unconstrained.
///
/// Dispatches on the store layout: the event-driven O(M+H+N) solver when the reverse
/// incidence index is present, the layer-synchronous O(k(M+H)) scan otherwise. Both
/// return identical results.
///
/// # Ranks and controller (Proposition 4.1)
/// A pair fires at layer k exactly when its last successor won at layer k-1, so
/// `rank[x] = k = 1 + max(rank[z] : z ∈ Succ(chosen[x]))`. This is the optimal
/// worst-case number of steps to the target. `chosen[x]` is the lowest PairId that
/// fires at x's rank (deterministic). Target states have rank 0 and no chosen action.
/// Non-winning states have rank `usize::MAX`.
pub fn reachability_constrained(
    target: &StateSet,
    constraint: Option<&StateSet>,
    store: &IndexedFrozen,
) -> ReachResult {
    match store.reverse_index() {
        Some((rev_off, rev_pair)) => reach_reverse(target, constraint, store, rev_off, rev_pair),
        None => reach_scan(target, constraint, store),
    }
}

/// Listing 2, event-driven over the reverse incidence index — O(M+H+N).
///
/// For each pair p, `remaining[p]` counts the successors of p that have not been
/// dequeued yet. Dequeuing a winning state y decrements only the pairs in `Rev(y)`;
/// when `remaining[p]` hits 0, p "fires" and its source becomes winning. Every
/// incidence is touched once.
///
/// The queue is FIFO, so states are dequeued in nondecreasing rank order and a pair
/// fires at layer `1 + max successor rank`. Several pairs of one state can fire at the
/// same layer in any order, so `chosen` keeps the minimum PairId among them.
fn reach_reverse(
    target: &StateSet,
    constraint: Option<&StateSet>,
    store: &IndexedFrozen,
    rev_off: &[u64],
    rev_pair: &[PairId],
) -> ReachResult {
    const NO_PAIR: u32 = u32::MAX;
    let n = store.n_states;
    let mut winning = target.clone();
    let mut rank = vec![usize::MAX; n];
    // FIFO queue: states are appended once and consumed through `head`.
    let mut queue: Vec<u32> = Vec::with_capacity(n);
    for x in target.iter() {
        rank[x] = 0;
        queue.push(x as u32);
    }
    let mut remaining: Vec<u32> = store
        .pair_succ_off
        .windows(2)
        .map(|w| (w[1] - w[0]) as u32)
        .collect();
    let mut chosen_pair = vec![NO_PAIR; n];

    let mut head = 0;
    while head < queue.len() {
        let y = queue[head] as usize;
        head += 1;
        let layer = rank[y] + 1;
        for &p in &rev_pair[rev_off[y] as usize..rev_off[y + 1] as usize] {
            let r = &mut remaining[p.0 as usize];
            *r -= 1;
            if *r > 0 {
                continue;
            }
            let xi = store.pair_source[p.0 as usize].0 as usize;
            if rank[xi] == usize::MAX {
                if constraint.is_some_and(|c| !c.contains(xi)) {
                    continue;
                }
                rank[xi] = layer;
                chosen_pair[xi] = p.0;
                winning.insert(xi);
                queue.push(xi as u32);
            } else if rank[xi] == layer && p.0 < chosen_pair[xi] {
                chosen_pair[xi] = p.0;
            }
        }
    }

    let chosen: HashMap<StateId, PairId> = chosen_pair
        .iter()
        .enumerate()
        .filter(|(_, &p)| p != NO_PAIR)
        .map(|(x, &p)| (StateId(x as u32), PairId(p)))
        .collect();
    ReachResult {
        winning,
        rank,
        chosen,
    }
}

/// Listing 2, layer-synchronous, for stores without a reverse index — O(k(M+H)).
///
/// Finding the pairs that have a newly winning state y as a successor requires a scan
/// over pairs. Rather than one scan per dequeued state (O(N(M+H))), a whole rank layer
/// is processed per scan: in round k, `remaining[p]` is decremented by the number of
/// successors that won in round k-1. k = number of rounds = max rank + 1. Pairs whose
/// source has already won are dropped from the scan list, so later rounds get cheaper.
/// Pairs are scanned in increasing PairId order, which yields the lowest firing pair.
fn reach_scan(
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
