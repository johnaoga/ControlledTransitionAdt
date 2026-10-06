//! Five-state smoke test binary (Appendix B of blueprint).
//!
//! Run: cargo run --bin five_state [-- --verbose]
//!
//! Expected output:
//!   winning: {A, B, C, D, E}
//!   rank: A=3, B=2, C=1, D=1, E=0
//!   invariant of {A,B,C,E}: {A, B, C, E}
//!
//! `--verbose` additionally prints the winning-set size after each attractor round.

use phase_specialized_cts::builder::HashBuilder;
use phase_specialized_cts::controller::ControllerStore;
use phase_specialized_cts::semantic::types::{InputId, StateId, StateSet};
use phase_specialized_cts::semantic::ControlledTransitionAdt;

const STATE_NAMES: [&str; 5] = ["A", "B", "C", "D", "E"];
const INPUT_NAMES: [&str; 2] = ["L", "R"];

fn names(set: &StateSet) -> String {
    let v: Vec<_> = set.iter().map(|i| STATE_NAMES[i]).collect();
    format!("{{{}}}", v.join(", "))
}

fn print_controller(ctrl: &ControllerStore) {
    let v: Vec<_> = (0..STATE_NAMES.len())
        .filter_map(|i| {
            ctrl.get_inputs(StateId(i as u32))
                .map(|u| format!("{}→{}", STATE_NAMES[i], INPUT_NAMES[u.0 as usize]))
        })
        .collect();
    println!("  controller: {}", v.join(", "));
}

fn main() {
    let verbose = std::env::args().any(|a| a == "--verbose" || a == "-v");

    // States: A=0, B=1, C=2, D=3, E=4
    // Inputs: L=0, R=1
    let n_states = 5;
    let n_inputs = 2;

    let (a, b, c, d, e) = (StateId(0), StateId(1), StateId(2), StateId(3), StateId(4));
    let (l, r) = (InputId(0), InputId(1));

    let mut builder = HashBuilder::new(n_states, n_inputs);

    // Transitions from Appendix B
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
    builder.add_transition(e, r, e);

    let store = builder.freeze();
    println!(
        "frozen: N={} A={} H={} M={}",
        store.n_states(),
        store.n_inputs(),
        store.n_pairs(),
        store.n_incidences()
    );

    // Reachability: target = {E}
    let mut target = StateSet::zeros(n_states);
    target.insert(e.0 as usize);
    let reach = store.reachability(&target, None);

    if verbose {
        let max_rank = reach
            .winning
            .iter()
            .map(|i| reach.rank[i])
            .max()
            .unwrap_or(0);
        for k in 0..=max_rank {
            let size = reach.winning.iter().filter(|&i| reach.rank[i] <= k).count();
            println!("  round {k}: |W| = {size}");
        }
    }

    println!("winning: {}", names(&reach.winning));
    let ranks: Vec<_> = reach
        .winning
        .iter()
        .map(|i| format!("{}={}", STATE_NAMES[i], reach.rank[i]))
        .collect();
    println!("rank: {}", ranks.join(", "));
    print_controller(&ControllerStore::from_reach_result(&reach, &store));

    // Invariance: safe = {A, B, C, E}
    let mut safe = StateSet::zeros(n_states);
    for s in [a, b, c, e] {
        safe.insert(s.0 as usize);
    }
    let inv = store.invariance(&safe);

    println!("invariant of {}: {}", names(&safe), names(&inv.invariant));
    print_controller(&ControllerStore::from_safety_result(&inv, &store));
}
