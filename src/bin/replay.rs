//! Trace replay binary.
//!
//! Usage: cargo run --release --bin replay -- [trace.txt]   (omit or `-` for stdin)
//!
//! Parses a trace, verifies its checksum (if present), feeds the triples into
//! HashBuilder, runs reachability (target = last state) and invariance (safe = all
//! states) on the builder, freezes, reruns both on IndexedFrozen — with the reverse
//! incidence index and forward-only — prints timings, and asserts that all backends
//! agree. With `--features oxidd` the BDD and complement-edge BDD backends are
//! replayed as well.

use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::time::{Duration, Instant};

use phase_specialized_cts::builder::{parse_trace, triple_checksum, HashBuilder};
use phase_specialized_cts::semantic::types::StateSet;
use phase_specialized_cts::semantic::ControlledTransitionAdt;

fn timed<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let t = Instant::now();
    let out = f();
    (out, t.elapsed())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).unwrap_or_else(|| "-".into());
    let reader: Box<dyn BufRead> = if path == "-" {
        Box::new(io::stdin().lock())
    } else {
        Box::new(BufReader::new(File::open(&path)?))
    };

    let t = Instant::now();
    let (header, iter) = parse_trace(reader)?;
    let triples = iter.collect::<Result<Vec<_>, _>>()?;
    println!(
        "parse:    {} states, {} inputs, {} triple lines in {:?}",
        header.n_states,
        header.n_inputs,
        triples.len(),
        t.elapsed()
    );

    if let Some(expected) = &header.checksum {
        let actual = triple_checksum(triples.iter().copied());
        if &actual != expected {
            return Err(format!("checksum mismatch: header {expected}, body {actual}").into());
        }
        println!("checksum: ok ({actual})");
    }
    if header.n_states == 0 {
        return Err("trace declares zero states".into());
    }

    let (builder, t_build) = timed(|| {
        let mut b = HashBuilder::new(header.n_states, header.n_inputs);
        for &(x, u, y) in &triples {
            b.add_transition(x, u, y);
        }
        b
    });
    println!(
        "build:    H = {} pairs, M = {} incidences in {:?}",
        builder.n_pairs(),
        builder.n_incidences(),
        t_build
    );

    let mut target = StateSet::zeros(header.n_states);
    target.insert(header.n_states - 1);
    let all = StateSet::ones(header.n_states);

    let (reach_h, t_reach_h) = timed(|| builder.reachability(&target, None));
    let (inv_h, t_inv_h) = timed(|| builder.invariance(&all));

    // Decision-diagram backends are built from the same builder before it is frozen.
    #[cfg(feature = "oxidd")]
    let dd_runs = {
        use phase_specialized_cts::backend::{BcddBackend, BddBackend};
        let (bdd, t_bdd): (BddBackend, _) = timed(|| builder.freeze_dd());
        let (bcdd, t_bcdd): (BcddBackend, _) = timed(|| builder.freeze_dd());
        println!(
            "encode:   bdd {} nodes in {t_bdd:?} | bcdd {} nodes in {t_bcdd:?}",
            bdd.transition_node_count(),
            bcdd.transition_node_count()
        );
        let backends: [(&str, &dyn ControlledTransitionAdt); 2] = [("bdd", &bdd), ("bcdd", &bcdd)];
        backends.map(|(label, dd)| {
            let (reach, t_reach) = timed(|| dd.reachability(&target, None));
            let (inv, t_inv) = timed(|| dd.invariance(&all));
            (label, reach, t_reach, inv, t_inv)
        })
    };

    let (fwd, t_fwd) = timed(|| builder.freeze_forward_only());
    let (reach_s, t_reach_s) = timed(|| fwd.reachability(&target, None));
    let (inv_s, t_inv_s) = timed(|| fwd.invariance(&all));
    let (store, t_rev) = timed(|| fwd.with_reverse());
    println!("freeze:   forward CSR {t_fwd:?} + reverse index {t_rev:?}");
    let (reach_f, t_reach_f) = timed(|| store.reachability(&target, None));
    let (inv_f, t_inv_f) = timed(|| store.invariance(&all));

    println!(
        "reach:    {} winning   hash {:?} | indexed_fwd {:?} | indexed {:?}",
        reach_f.winning.len(),
        t_reach_h,
        t_reach_s,
        t_reach_f
    );
    println!(
        "invar:    {} invariant hash {:?} | indexed_fwd {:?} | indexed {:?}",
        inv_f.invariant.len(),
        t_inv_h,
        t_inv_s,
        t_inv_f
    );

    if reach_h.winning != reach_f.winning || reach_h.rank != reach_f.rank {
        return Err("backends disagree on reachability".into());
    }
    if inv_h.invariant != inv_f.invariant {
        return Err("backends disagree on invariance".into());
    }
    if reach_s.rank != reach_f.rank || reach_s.chosen != reach_f.chosen {
        return Err("forward-only and reverse-index solvers disagree on reachability".into());
    }
    if inv_s.invariant != inv_f.invariant {
        return Err("forward-only and reverse-index solvers disagree on invariance".into());
    }
    #[cfg(feature = "oxidd")]
    for (label, reach, t_reach, inv, t_inv) in dd_runs {
        println!("{label:<9} reach {t_reach:?} | invar {t_inv:?}");
        if reach.winning != reach_f.winning || reach.rank != reach_f.rank {
            return Err(format!("{label} disagrees on reachability").into());
        }
        if inv.invariant != inv_f.invariant {
            return Err(format!("{label} disagrees on invariance").into());
        }
    }
    println!("backends agree");
    Ok(())
}
