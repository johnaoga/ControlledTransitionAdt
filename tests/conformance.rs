//! Differential conformance tests: compare HashBuilder+IndexedFrozen against reference oracle.
//!
//! Uses proptest to generate random small transition systems and asserts that
//! cpre, reachability, and invariance match the reference on every instance.

use phase_specialized_cts::builder::HashBuilder;
use phase_specialized_cts::semantic::reference::{
    cpre_reference, invariance_reference, reachability_reference,
};
use phase_specialized_cts::semantic::types::{InputId, StateId, StateSet};
use phase_specialized_cts::semantic::ControlledTransitionAdt;
use proptest::prelude::*;
use std::collections::HashSet;

/// Generate a random small transition system.
fn arb_system(
    max_states: usize,
    max_inputs: usize,
    max_triples: usize,
) -> impl Strategy<Value = (usize, usize, HashSet<(StateId, InputId, StateId)>)> {
    (2..=max_states, 2..=max_inputs).prop_flat_map(move |(n, a)| {
        let triple_strategy = prop::collection::hash_set(
            (0..n as u32, 0..a as u32, 0..n as u32)
                .prop_map(|(x, u, y)| (StateId(x), InputId(u), StateId(y))),
            0..=max_triples,
        );
        (Just(n), Just(a), triple_strategy)
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// CPre from IndexedFrozen matches cpre_reference on random systems.
    #[test]
    fn cpre_matches_reference((n, a, triples) in arb_system(8, 4, 30)) {
        let mut builder = HashBuilder::new(n, a);
        for &(x, u, y) in &triples {
            builder.add_transition(x, u, y);
        }
        let store = builder.freeze();

        // Random target set.
        let target: StateSet = {
            let mut s = StateSet::zeros(n);
            for i in 0..n { if i % 2 == 0 { s.insert(i); } }
            s
        };

        let ref_cpre = cpre_reference(&triples, &target, n);
        let idx_cpre = store.cpre(&target);

        for id in 0..n {
            prop_assert_eq!(
                ref_cpre.contains(id), idx_cpre.contains(id),
                "CPre disagreement at state {}", id
            );
        }
    }

    /// Reachability winning set matches reference on random systems.
    #[test]
    fn reachability_winning_matches_reference((n, a, triples) in arb_system(8, 4, 30)) {
        let mut builder = HashBuilder::new(n, a);
        for &(x, u, y) in &triples {
            builder.add_transition(x, u, y);
        }
        let store = builder.freeze();

        // Target = state 0.
        let mut target = StateSet::zeros(n);
        target.insert(0);

        let ref_result = reachability_reference(&triples, &target, n);
        let idx_result = store.reachability(&target, None);

        for id in 0..n {
            prop_assert_eq!(
                ref_result.winning.contains(id),
                idx_result.winning.contains(id),
                "Reachability winning disagreement at state {}", id
            );
        }
    }

    /// Invariance result matches reference on random systems.
    #[test]
    fn invariance_matches_reference((n, a, triples) in arb_system(8, 4, 30)) {
        let mut builder = HashBuilder::new(n, a);
        for &(x, u, y) in &triples {
            builder.add_transition(x, u, y);
        }
        let store = builder.freeze();

        // Safe = all states.
        let safe = StateSet::ones(n);

        let ref_result = invariance_reference(&triples, &safe, n);
        let idx_result = store.invariance(&safe);

        for id in 0..n {
            prop_assert_eq!(
                ref_result.invariant.contains(id),
                idx_result.invariant.contains(id),
                "Invariance disagreement at state {}", id
            );
        }
    }
}
