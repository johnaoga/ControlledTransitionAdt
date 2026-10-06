//! Invariance solver — dual deletion algorithm.
//!
//! See specs.md § F7 — Invariance Solver
//! Blueprint Listing 3, Appendix A.3

use crate::backend::indexed_frozen::IndexedFrozen;
use crate::semantic::types::{PairId, SafetyResult, StateSet};

/// Compute the maximal controlled invariant subset Y* ⊆ safe.
///
/// # Algorithm (Listing 3; Phase 1 — no reverse index, O(k(M+H)))
///
/// A pair p is *good* while all its successors lie in the current Y (`bad[p] = 0` in
/// Listing 3; only the zero test matters, so the counter is kept as a boolean: a pair
/// leaves the `good` list the first time one of its successors is deleted).
/// `good_actions[x]` counts the good pairs at x; when it reaches 0, x is deleted from Y.
///
/// Without a reverse index the solver finds pairs affected by a deletion by scanning
/// the remaining good pairs. Each scan applies all deletions seen so far, and a
/// deletion takes effect immediately within the same scan. Scans repeat until one
/// deletes nothing. Every scan before the last deletes at least one state, so
/// k ≤ |safe| + 1 scans at O(M+H) each. Pairs that turn bad, or whose source was
/// deleted, are dropped from the scan list. Deletion order does not affect the result,
/// because the greatest fixed point is unique.
///
/// Phase 2 upgrade: with `rev_off` / `rev_pair`, a deletion of y touches only its
/// O(d⁻(y)) predecessor pairs, giving O(M+H+N) total.
pub fn invariance(safe: &StateSet, store: &IndexedFrozen) -> SafetyResult {
    let n = store.n_states;
    let mut in_y = vec![false; n];
    for x in safe.iter() {
        in_y[x] = true;
    }

    let mut good_actions = vec![0u32; n];
    let mut good: Vec<u32> = Vec::with_capacity(store.n_pairs);
    let mut changed = false;

    // Initial pass: bad[p] = |Succ(p) \ S|; keep only pairs with bad[p] = 0.
    for x in safe.iter() {
        let (start, end) = (
            store.state_pair_off[x] as usize,
            store.state_pair_off[x + 1] as usize,
        );
        for p in start..end {
            if all_in(store, p as u32, &in_y) {
                good.push(p as u32);
                good_actions[x] += 1;
            }
        }
        if good_actions[x] == 0 {
            in_y[x] = false;
            changed = true;
        }
    }

    // Deletion rounds.
    while changed {
        changed = false;
        good.retain(|&p| {
            let x = store.pair_source[p as usize].0 as usize;
            if !in_y[x] {
                return false;
            }
            if all_in(store, p, &in_y) {
                return true;
            }
            good_actions[x] -= 1;
            if good_actions[x] == 0 {
                in_y[x] = false;
                changed = true;
            }
            false
        });
    }

    let mut invariant = StateSet::zeros(n);
    for (x, _) in in_y.iter().enumerate().filter(|(_, &b)| b) {
        invariant.insert(x);
    }
    SafetyResult { invariant }
}

#[inline]
fn all_in(store: &IndexedFrozen, p: u32, in_y: &[bool]) -> bool {
    store
        .succs_for_pair(PairId(p))
        .iter()
        .all(|s| in_y[s.0 as usize])
}
