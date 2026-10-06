//! Criterion benchmark suite for all ADT operations (specs.md § F9).
//!
//! Run: cargo bench                      (reports in target/criterion/)
//!      PSCTS_BENCH_SIDE=200 cargo bench  (synthetic grid side; default 100 → N = 10⁴)
//!
//! Groups: `build/{hash,indexed}`, `query/{contains,successors,predecessor_pairs,
//! safe_actions,cpre}/{hash,indexed}`, `solve/{reachability,invariance}/indexed`.
//! Every benchmark runs on the Appendix B five-state example and on a synthetic grid.

use criterion::{
    black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput,
};
use phase_specialized_cts::backend::IndexedFrozen;
use phase_specialized_cts::builder::HashBuilder;
use phase_specialized_cts::semantic::types::{InputId, StateId, StateSet};
use phase_specialized_cts::semantic::ControlledTransitionAdt;

type Triple = (StateId, InputId, StateId);

/// Number of random queries per iteration in the point-query benchmarks.
const QUERIES: usize = 1024;

/// One benchmark instance: the raw relation plus target/safe sets and query samples.
struct Instance {
    name: String,
    n_states: usize,
    n_inputs: usize,
    triples: Vec<Triple>,
    target: StateSet,
    safe: StateSet,
    /// Half present, half random (mostly absent) triples.
    queries: Vec<Triple>,
}

impl Instance {
    fn builder(&self) -> HashBuilder {
        let mut b = HashBuilder::new(self.n_states, self.n_inputs);
        for &(x, u, y) in &self.triples {
            b.add_transition(x, u, y);
        }
        b
    }
}

/// SplitMix64: tiny deterministic PRNG so instances are identical across runs/machines.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn random_queries(rng: &mut Rng, triples: &[Triple], n: usize, a: usize) -> Vec<Triple> {
    (0..QUERIES)
        .map(|i| {
            if i % 2 == 0 {
                triples[rng.below(triples.len())]
            } else {
                let s = |r: &mut Rng| StateId(r.below(n) as u32);
                (s(rng), InputId(rng.below(a) as u32), s(rng))
            }
        })
        .collect()
}

fn five_state() -> Instance {
    let (a, b, c, d, e) = (StateId(0), StateId(1), StateId(2), StateId(3), StateId(4));
    let (l, r) = (InputId(0), InputId(1));
    let triples = vec![
        (a, l, b),
        (a, r, b),
        (a, r, c),
        (b, l, a),
        (b, r, c),
        (c, l, b),
        (c, l, d),
        (c, r, e),
        (d, l, c),
        (d, r, e),
        (e, l, d),
        (e, r, e),
    ];
    let mut target = StateSet::zeros(5);
    target.insert(4);
    let mut rng = Rng(5);
    let queries = random_queries(&mut rng, &triples, 5, 2);
    Instance {
        name: "five_state".into(),
        n_states: 5,
        n_inputs: 2,
        triples,
        target,
        safe: StateSet::ones(5),
        queries,
    }
}

/// SCOTS-like abstraction of a 2-D integrator on a `side × side` grid.
///
/// Inputs: stay, E, W, N, S. Each (cell, input) moves to the nominal neighbour and,
/// to model disturbance, also to 0–2 random cells adjacent to it. Target is the
/// 5×5 corner block at the origin; the safe set excludes ~10% random obstacle cells.
fn grid(side: usize) -> Instance {
    const DIRS: [(isize, isize); 5] = [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)];
    let n = side * side;
    let clamp = |v: isize| v.clamp(0, side as isize - 1) as usize;
    let id = |i: usize, j: usize| StateId((i * side + j) as u32);
    let mut rng = Rng(0xC0FFEE);
    let mut triples = Vec::new();
    for i in 0..side {
        for j in 0..side {
            for (u, &(di, dj)) in DIRS.iter().enumerate() {
                let (bi, bj) = (i as isize + di, j as isize + dj);
                triples.push((id(i, j), InputId(u as u32), id(clamp(bi), clamp(bj))));
                for _ in 0..rng.below(3) {
                    let (ji, jj) = DIRS[rng.below(DIRS.len())];
                    let y = id(clamp(bi + ji), clamp(bj + jj));
                    triples.push((id(i, j), InputId(u as u32), y));
                }
            }
        }
    }
    let mut target = StateSet::zeros(n);
    for i in 0..side.min(5) {
        for j in 0..side.min(5) {
            target.insert(i * side + j);
        }
    }
    let mut safe = StateSet::ones(n);
    for x in 0..n {
        if rng.below(10) == 0 {
            safe.remove(x);
        }
    }
    let queries = random_queries(&mut rng, &triples, n, DIRS.len());
    Instance {
        name: format!("grid_{n}"),
        n_states: n,
        n_inputs: DIRS.len(),
        triples,
        target,
        safe,
        queries,
    }
}

fn instances() -> Vec<Instance> {
    let side = std::env::var("PSCTS_BENCH_SIDE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);
    vec![five_state(), grid(side)]
}

fn bench_build(c: &mut Criterion) {
    let mut g = c.benchmark_group("build");
    for inst in instances() {
        g.throughput(Throughput::Elements(inst.triples.len() as u64));
        g.bench_function(BenchmarkId::new("hash", &inst.name), |b| {
            b.iter(|| black_box(inst.builder()))
        });
        let builder = inst.builder();
        g.bench_function(BenchmarkId::new("indexed", &inst.name), |b| {
            b.iter_batched(
                || builder.clone(),
                |bld| black_box(bld.freeze()),
                BatchSize::LargeInput,
            )
        });
    }
    g.finish();
}

/// Run `f` on both backends of every instance, under `group/{hash,indexed}/<instance>`.
fn bench_both<F>(c: &mut Criterion, group: &str, per_iter: Option<u64>, f: F)
where
    F: Fn(&dyn ControlledTransitionAdt, &Instance),
{
    let mut g = c.benchmark_group(group);
    for inst in instances() {
        if let Some(k) = per_iter {
            g.throughput(Throughput::Elements(k));
        }
        let hash = inst.builder();
        let indexed: IndexedFrozen = inst.builder().freeze();
        let backends: [(&str, &dyn ControlledTransitionAdt); 2] =
            [("hash", &hash), ("indexed", &indexed)];
        for (label, backend) in backends {
            g.bench_function(BenchmarkId::new(label, &inst.name), |b| {
                b.iter(|| f(black_box(backend), &inst))
            });
        }
    }
    g.finish();
}

fn bench_queries(c: &mut Criterion) {
    bench_both(c, "query/contains", Some(QUERIES as u64), |s, inst| {
        for &(x, u, y) in &inst.queries {
            black_box(s.contains(x, u, y));
        }
    });
    bench_both(c, "query/successors", Some(QUERIES as u64), |s, inst| {
        for &(x, u, _) in &inst.queries {
            black_box(s.successors(x, u).count());
        }
    });
    bench_both(c, "query/safe_actions", None, |s, inst| {
        black_box(s.safe_actions(&inst.target));
    });
    bench_both(c, "query/cpre", None, |s, inst| {
        black_box(s.cpre(&inst.target));
    });

    // Forward-only store: O(M) scan per call (Phase 2 adds the reverse index).
    let mut g = c.benchmark_group("query/predecessor_pairs");
    for inst in instances() {
        let store = inst.builder().freeze();
        let y = inst.queries[0].2;
        g.bench_function(BenchmarkId::new("indexed", &inst.name), |b| {
            b.iter(|| black_box(store.predecessor_pairs(black_box(y)).count()))
        });
    }
    g.finish();
}

fn bench_solvers(c: &mut Criterion) {
    let mut g = c.benchmark_group("solve");
    for inst in instances() {
        let store = inst.builder().freeze();
        g.bench_function(BenchmarkId::new("reachability/indexed", &inst.name), |b| {
            b.iter(|| black_box(store.reachability(black_box(&inst.target), None)))
        });
        g.bench_function(BenchmarkId::new("invariance/indexed", &inst.name), |b| {
            b.iter(|| black_box(store.invariance(black_box(&inst.safe))))
        });
    }
    g.finish();
}

criterion_group!(benches, bench_build, bench_queries, bench_solvers);
criterion_main!(benches);
