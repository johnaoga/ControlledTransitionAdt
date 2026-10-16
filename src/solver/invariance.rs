//! Invariance solver — dual deletion algorithm.
//!
//! See specs.md § F7 — Invariance Solver
//! Blueprint Listing 3, Appendix A.3

use crate::backend::indexed_frozen::IndexedFrozen;
use crate::semantic::types::{PairId, SafetyResult, StateSet};

/// Compute the maximal controlled invariant subset Y* ⊆ safe.
///
/// Dispatches on the store layout: the event-driven O(M+H+N) solver when the reverse
/// incidence index is present, the O(k(M+H)) deletion sweeps otherwise. The greatest
/// fixed point is unique, so both return the same set.
pub fn invariance(safe: &StateSet, store: &IndexedFrozen) -> SafetyResult {
    match store.reverse_index() {
        Some((rev_off, rev_pair)) => invariance_reverse(safe, store, rev_off, rev_pair),
        None => invariance_scan(safe, store),
    }
}

/// Listing 3, event-driven over the reverse incidence index — O(M+H+N).
///
/// `bad[p]` records that some successor of p is outside Y (Listing 3 keeps a count;
/// only the zero test matters). `good_actions[x]` counts the pairs at x that are not
/// bad; when it reaches 0, x is deleted from Y and queued. Deleting y turns only the
/// pairs in `Rev(y)` bad, so every incidence is touched at most once after the
/// O(M+H) initial pass.
fn invariance_reverse(
    safe: &StateSet,
    store: &IndexedFrozen,
    rev_off: &[u64],
    rev_pair: &[PairId],
) -> SafetyResult {
    let n = store.n_states;
    let mut in_y = vec![false; n];
    for x in safe.iter() {
        in_y[x] = true;
    }

    // Initial pass against Y = safe: bad[p] ⇔ Succ(p) ⊄ safe.
    let mut bad = vec![false; store.n_pairs];
    let mut good_actions = vec![0u32; n];
    let mut queue: Vec<u32> = Vec::new();
    for x in safe.iter() {
        let (start, end) = (
            store.state_pair_off[x] as usize,
            store.state_pair_off[x + 1] as usize,
        );
        for (p, is_bad) in bad[start..end].iter_mut().enumerate() {
            if all_in(store, (start + p) as u32, &in_y) {
                good_actions[x] += 1;
            } else {
                *is_bad = true;
            }
        }
        if good_actions[x] == 0 {
            queue.push(x as u32);
        }
    }
    // Deletions are applied only after the pass, so it saw Y = safe throughout.
    for &x in &queue {
        in_y[x as usize] = false;
    }

    while let Some(y) = queue.pop() {
        let y = y as usize;
        for &p in &rev_pair[rev_off[y] as usize..rev_off[y + 1] as usize] {
            let is_bad = &mut bad[p.0 as usize];
            if *is_bad {
                continue;
            }
            *is_bad = true;
            // Pairs sourced outside `safe` were never counted; `in_y` filters them.
            let x = store.pair_source[p.0 as usize].0 as usize;
            if in_y[x] {
                good_actions[x] -= 1;
                if good_actions[x] == 0 {
                    in_y[x] = false;
                    queue.push(x as u32);
                }
            }
        }
    }

    SafetyResult {
        invariant: to_state_set(&in_y),
    }
}

/// Listing 3 for stores without a reverse index — O(k(M+H)).
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
fn invariance_scan(safe: &StateSet, store: &IndexedFrozen) -> SafetyResult {
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

    SafetyResult {
        invariant: to_state_set(&in_y),
    }
}

fn to_state_set(in_y: &[bool]) -> StateSet {
    let mut set = StateSet::zeros(in_y.len());
    for (x, _) in in_y.iter().enumerate().filter(|(_, &b)| b) {
        set.insert(x);
    }
    set
}

#[inline]
fn all_in(store: &IndexedFrozen, p: u32, in_y: &[bool]) -> bool {
    store
        .succs_for_pair(PairId(p))
        .iter()
        .all(|s| in_y[s.0 as usize])
}
