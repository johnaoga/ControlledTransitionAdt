//! Tests that HashBuilder → freeze() preserves the triple set exactly.

use phase_specialized_cts::builder::HashBuilder;
use phase_specialized_cts::semantic::types::{InputId, StateId};
use phase_specialized_cts::semantic::ControlledTransitionAdt;
use std::collections::HashSet;

fn build_and_collect(
    n_states: usize,
    n_inputs: usize,
    triples: &[(StateId, InputId, StateId)],
) -> HashSet<(StateId, InputId, StateId)> {
    let mut builder = HashBuilder::new(n_states, n_inputs);
    for &(x, u, y) in triples {
        builder.add_transition(x, u, y);
    }
    let store = builder.freeze();
    // Reconstruct triple set from IndexedFrozen via successors().
    let mut result = HashSet::new();
    for x_id in 0..n_states as u32 {
        let x = StateId(x_id);
        for u_id in 0..n_inputs as u32 {
            let u = InputId(u_id);
            for y in store.successors(x, u) {
                result.insert((x, u, y));
            }
        }
    }
    result
}

#[test]
fn freeze_preserves_five_state_triples() {
    let n = 5;
    let a = 2;
    let triples = vec![
        (StateId(0), InputId(0), StateId(1)), // A-L-B
        (StateId(0), InputId(1), StateId(1)), // A-R-B
        (StateId(0), InputId(1), StateId(2)), // A-R-C
        (StateId(1), InputId(0), StateId(0)), // B-L-A
        (StateId(1), InputId(1), StateId(2)), // B-R-C
        (StateId(2), InputId(0), StateId(1)), // C-L-B
        (StateId(2), InputId(0), StateId(3)), // C-L-D
        (StateId(2), InputId(1), StateId(4)), // C-R-E
        (StateId(3), InputId(0), StateId(2)), // D-L-C
        (StateId(3), InputId(1), StateId(4)), // D-R-E
        (StateId(4), InputId(0), StateId(3)), // E-L-D
        (StateId(4), InputId(1), StateId(4)), // E-R-E
    ];
    let expected: HashSet<_> = triples.iter().cloned().collect();
    let actual = build_and_collect(n, a, &triples);
    assert_eq!(actual, expected);
}

#[test]
fn freeze_deduplicates_duplicate_inserts() {
    let n = 3;
    let a = 2;
    let triples = vec![
        (StateId(0), InputId(0), StateId(1)),
        (StateId(0), InputId(0), StateId(1)), // duplicate
        (StateId(1), InputId(1), StateId(2)),
    ];
    let actual = build_and_collect(n, a, &triples);
    assert_eq!(
        actual.len(),
        2,
        "duplicates should be deduplicated after freeze"
    );
}

#[test]
fn freeze_empty_builder() {
    let store = HashBuilder::new(5, 3).freeze();
    assert_eq!(store.n_pairs(), 0);
    assert_eq!(store.n_incidences(), 0);
}
