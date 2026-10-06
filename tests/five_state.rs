//! Integration test: five-state example from Appendix B of the blueprint.
//!
//! This is the primary acceptance criterion. All assertions must pass.

use phase_specialized_cts::builder::HashBuilder;
use phase_specialized_cts::semantic::ControlledTransitionAdt;
use phase_specialized_cts::semantic::types::{InputId, StateId, StateSet};

fn five_state_store() -> (phase_specialized_cts::backend::IndexedFrozen, usize) {
    let n_states = 5;
    let n_inputs = 2;

    let (a, b, c, d, e) = (StateId(0), StateId(1), StateId(2), StateId(3), StateId(4));
    let (l, r) = (InputId(0), InputId(1));

    let mut builder = HashBuilder::new(n_states, n_inputs);

    // Appendix B transitions
    builder.add_transition(a, l, b);
    builder.add_transition(a, r, b);
    builder.add_transition(a, r, c);
    builder.add_transition(b, l, a);
    builder.add_transition(b, r, c);
    builder.add_transition(c, l, b);
    builder.add_transition(c, l, d);
    builder.add_transition(c, r, e);
    builder.add_transition(d, l, c);
    builder.add_transition(d, r, e);
    builder.add_transition(e, l, d);
    builder.add_transition(e, r, e);  // self-loop

    (builder.freeze(), n_states)
}

fn target_e(n_states: usize) -> StateSet {
    let mut t = StateSet::zeros(n_states);
    t.insert(4); // E = 4
    t
}

// ── Reachability tests ────────────────────────────────────────────────────────

#[test]
fn five_state_reachability_winning_is_all() {
    let (store, n) = five_state_store();
    let target = target_e(n);
    let result = store.reachability(&target, None);

    for id in 0..n {
        assert!(result.winning.contains(id), "state {} should be winning", id);
    }
}

#[test]
fn five_state_reachability_rank_e_is_0() {
    let (store, n) = five_state_store();
    let result = store.reachability(&target_e(n), None);
    assert_eq!(result.rank[4], 0, "E (id=4) should have rank 0");
}

#[test]
fn five_state_reachability_rank_c_is_1() {
    let (store, n) = five_state_store();
    let result = store.reachability(&target_e(n), None);
    assert_eq!(result.rank[2], 1, "C (id=2) should have rank 1");
}

#[test]
fn five_state_reachability_rank_d_is_1() {
    let (store, n) = five_state_store();
    let result = store.reachability(&target_e(n), None);
    assert_eq!(result.rank[3], 1, "D (id=3) should have rank 1");
}

#[test]
fn five_state_reachability_rank_b_is_2() {
    let (store, n) = five_state_store();
    let result = store.reachability(&target_e(n), None);
    assert_eq!(result.rank[1], 2, "B (id=1) should have rank 2");
}

#[test]
fn five_state_reachability_rank_a_is_3() {
    let (store, n) = five_state_store();
    let result = store.reachability(&target_e(n), None);
    assert_eq!(result.rank[0], 3, "A (id=0) should have rank 3");
}

#[test]
fn five_state_reachability_chosen_action_exists_for_all_winning() {
    let (store, n) = five_state_store();
    let result = store.reachability(&target_e(n), None);
    for id in 0..n {
        if result.winning.contains(id) && id != 4 {
            // Target state E may have no chosen action (already at goal).
            assert!(
                result.chosen.contains_key(&StateId(id as u32)),
                "winning state {} should have a chosen action",
                id
            );
        }
    }
}

// ── Invariance tests ──────────────────────────────────────────────────────────

fn safe_abce(n_states: usize) -> StateSet {
    let mut s = StateSet::zeros(n_states);
    s.insert(0); // A
    s.insert(1); // B
    s.insert(2); // C
    s.insert(4); // E
    s
}

#[test]
fn five_state_invariance_of_abce_is_abce() {
    let (store, n) = five_state_store();
    let safe = safe_abce(n);
    let result = store.invariance(&safe);

    // Expected: {A, B, C, E} — all have a staying action within the set.
    for id in [0usize, 1, 2, 4] {
        assert!(
            result.invariant.contains(id),
            "state {} should be in invariant",
            id
        );
    }
    // D (id=3) must NOT be in invariant.
    assert!(
        !result.invariant.contains(3),
        "D (id=3) should NOT be in invariant of {{A,B,C,E}}"
    );
}

#[test]
fn five_state_invariance_of_all_states_is_all() {
    let (store, n) = five_state_store();
    let all = StateSet::ones(n);
    let result = store.invariance(&all);
    for id in 0..n {
        assert!(result.invariant.contains(id), "state {} should be in invariant of all", id);
    }
}

// ── CSR structure tests ───────────────────────────────────────────────────────

#[test]
fn five_state_frozen_has_correct_counts() {
    let (store, _n) = five_state_store();
    // 12 transitions in the example; 11 distinct (x,u) pairs (A-L, A-R, B-L, B-R, C-L, C-R,
    // D-L, D-R, E-L, E-R) = 10 pairs. M = 12 (A-R has 2 succs, C-L has 2 succs).
    assert_eq!(store.n_pairs(), 10, "five-state example has 10 distinct action pairs");
    assert_eq!(store.n_incidences(), 12, "five-state example has 12 (x,u,y) triples");
}

#[test]
fn five_state_contains_known_transitions() {
    let (store, _n) = five_state_store();
    assert!(store.contains(StateId(0), InputId(0), StateId(1)), "A-L-B should exist");
    assert!(store.contains(StateId(4), InputId(1), StateId(4)), "E-R-E self-loop should exist");
    assert!(!store.contains(StateId(0), InputId(0), StateId(0)), "A-L-A should NOT exist");
}
