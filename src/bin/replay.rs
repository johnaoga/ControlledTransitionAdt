//! Trace replay binary.
//!
//! Usage: cargo run --release --bin replay -- [trace.txt]   (omit or `-` for stdin)
//!
//! Parses a trace, verifies its checksum (if present), feeds the triples into
//! HashBuilder, runs reachability (target = last state) and invariance (safe = all
//! states) on the builder, freezes, reruns both on IndexedFrozen, prints timings,
//! and asserts that both backends agree.

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

    let (store, t_freeze) = timed(|| builder.freeze());
    println!("freeze:   {t_freeze:?}");
    let (reach_f, t_reach_f) = timed(|| store.reachability(&target, None));
    let (inv_f, t_inv_f) = timed(|| store.invariance(&all));

    println!(
        "reach:    {} winning   hash {:?} | indexed {:?}",
        reach_f.winning.len(),
        t_reach_h,
        t_reach_f
    );
    println!(
        "invar:    {} invariant hash {:?} | indexed {:?}",
        inv_f.invariant.len(),
        t_inv_h,
        t_inv_f
    );

    if reach_h.winning != reach_f.winning || reach_h.rank != reach_f.rank {
        return Err("backends disagree on reachability".into());
    }
    if inv_h.invariant != inv_f.invariant {
        return Err("backends disagree on invariance".into());
    }
    println!("backends agree");
    Ok(())
}
