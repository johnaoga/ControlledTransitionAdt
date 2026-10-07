//! Crate-internal differential and property tests complementing `tests/`.
//!
//! Covers: HashBuilder vs IndexedFrozen on every ADT operation, rank optimality,
//! controller progress / safety closure (specs.md § Testing Strategy), constrained
//! reachability, controller serialization, and trace checksum / error handling.

use std::collections::{HashMap, HashSet};
use std::io::Cursor;

use proptest::prelude::*;

use crate::backend::IndexedFrozen;
use crate::builder::{parse_trace, triple_checksum, write_trace, HashBuilder, TraceError};
use crate::controller::ControllerStore;
use crate::semantic::reference::{cpre_reference, invariance_reference, reachability_reference};
use crate::semantic::types::{InputId, PairId, StateId, StateSet, TraceHeader};
use crate::semantic::ControlledTransitionAdt;

type Triples = HashSet<(StateId, InputId, StateId)>;

fn arb_system() -> impl Strategy<Value = (usize, usize, Triples)> {
    (1..=12usize, 1..=4usize).prop_flat_map(|(n, a)| {
        let t = (0..n as u32, 0..a as u32, 0..n as u32)
            .prop_map(|(x, u, y)| (StateId(x), InputId(u), StateId(y)));
        (Just(n), Just(a), prop::collection::hash_set(t, 0..=60))
    })
}

fn arb_set(n: usize) -> impl Strategy<Value = StateSet> {
    prop::collection::vec(any::<bool>(), n).prop_map(|bits| {
        let mut s = StateSet::zeros(bits.len());
        for (i, _) in bits.iter().enumerate().filter(|(_, &b)| b) {
            s.insert(i);
        }
        s
    })
}

/// A random system with two random state sets and a random input filter.
fn arb_case() -> impl Strategy<Value = (usize, usize, Triples, StateSet, StateSet, StateSet)> {
    arb_system().prop_flat_map(|(n, a, t)| {
        (
            Just(n),
            Just(a),
            Just(t),
            arb_set(n),
            arb_set(n),
            arb_set(a),
        )
    })
}

fn build(n: usize, a: usize, triples: &Triples) -> HashBuilder {
    let mut b = HashBuilder::new(n, a);
    for &(x, u, y) in triples {
        b.add_transition(x, u, y);
    }
    b
}

fn sorted<T: Ord>(it: impl Iterator<Item = T>) -> Vec<T> {
    let mut v: Vec<T> = it.collect();
    v.sort_unstable();
    v
}

/// Constrained ranks by definition (the oracle has no constraint parameter):
/// rank[x] = least k with x ∈ W_k, W_0 = target, W_k = W_{k-1} ∪ (CPre(W_{k-1}) ∩ C).
fn layered_ranks(
    triples: &Triples,
    target: &StateSet,
    c: Option<&StateSet>,
    n: usize,
) -> Vec<usize> {
    let mut rank = vec![usize::MAX; n];
    let mut w = target.clone();
    for x in w.iter() {
        rank[x] = 0;
    }
    for k in 1.. {
        let pre = cpre_reference(triples, &w, n);
        let new: Vec<_> = pre
            .iter()
            .filter(|&x| !w.contains(x) && c.is_none_or(|c| c.contains(x)))
            .collect();
        if new.is_empty() {
            break;
        }
        for x in new {
            rank[x] = k;
            w.insert(x);
        }
    }
    rank
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Every query agrees between the builder and the frozen store.
    #[test]
    fn hash_and_indexed_queries_agree((n, a, triples, y, _, filter) in arb_case()) {
        let hash = build(n, a, &triples);
        let idx = hash.clone().freeze();
        prop_assert_eq!(idx.n_pairs(), hash.n_pairs());
        prop_assert_eq!(idx.n_incidences(), triples.len());

        for x in (0..n as u32).map(StateId) {
            prop_assert_eq!(sorted(hash.admissible_inputs(x)), idx.admissible_inputs(x).collect::<Vec<_>>());
            prop_assert_eq!(sorted(hash.predecessor_pairs(x)), sorted(idx.predecessor_pairs(x)));
            for u in (0..a as u32).map(InputId) {
                let s = idx.successors(x, u).collect::<Vec<_>>();
                prop_assert_eq!(sorted(hash.successors(x, u)), s.clone());
                prop_assert!(s.windows(2).all(|w| w[0] < w[1]), "successors sorted & unique");
                for z in (0..n as u32).map(StateId) {
                    let expect = triples.contains(&(x, u, z));
                    prop_assert_eq!(hash.contains(x, u, z), expect);
                    prop_assert_eq!(idx.contains(x, u, z), expect);
                }
            }
        }

        prop_assert_eq!(hash.post(&y, None), idx.post(&y, None));
        prop_assert_eq!(hash.post(&y, Some(&filter)), idx.post(&y, Some(&filter)));
        prop_assert_eq!(hash.cpre(&y), cpre_reference(&triples, &y, n));
        prop_assert_eq!(idx.cpre(&y), cpre_reference(&triples, &y, n));

        // Pair id spaces differ, so compare safe actions as (x, u) sets.
        let h_safe: HashSet<_> = hash.safe_actions(&y).into_iter().map(|p| hash.pair_of(p)).collect();
        let i_safe: HashSet<_> = idx.safe_actions(&y).into_iter()
            .map(|p| (idx.pair_source(p), idx.pair_input(p))).collect();
        prop_assert_eq!(h_safe, i_safe);
    }

    /// Winning sets and ranks match the oracle and the layered-CPre definition;
    /// chosen actions are rank-decreasing progress actions.
    #[test]
    fn reachability_ranks_and_progress((n, a, triples, target, constraint, _) in arb_case()) {
        let hash = build(n, a, &triples);
        let idx: IndexedFrozen = hash.clone().freeze();

        let r = idx.reachability(&target, None);
        // The oracle uses freeze's canonical pair numbering, so the whole result
        // (winning set, ranks, chosen PairIds) must match exactly.
        let oracle = reachability_reference(&triples, &target, n);
        prop_assert_eq!(&r.winning, &oracle.winning);
        prop_assert_eq!(&r.rank, &oracle.rank);
        prop_assert_eq!(&r.chosen, &oracle.chosen);
        let rh = hash.reachability(&target, None);
        prop_assert_eq!(&rh.winning, &r.winning);
        prop_assert_eq!(&rh.rank, &r.rank);

        for x in 0..n {
            let chosen = r.chosen.get(&StateId(x as u32));
            if !r.winning.contains(x) || target.contains(x) {
                prop_assert!(chosen.is_none());
                continue;
            }
            let p = *chosen.expect("non-target winning state has an action");
            prop_assert_eq!(idx.pair_source(p), StateId(x as u32));
            let max_succ = idx.succs_for_pair(p).iter().map(|s| r.rank[s.0 as usize]).max().unwrap();
            prop_assert_eq!(r.rank[x], max_succ + 1, "rank = 1 + max succ rank");
        }

        let rc = idx.reachability(&target, Some(&constraint));
        let expect = layered_ranks(&triples, &target, Some(&constraint), n);
        prop_assert_eq!(&rc.rank, &expect);
        prop_assert_eq!(&hash.reachability(&target, Some(&constraint)).rank, &expect);
        for (x, &k) in expect.iter().enumerate() {
            prop_assert_eq!(rc.winning.contains(x), k != usize::MAX);
        }
    }

    /// Invariance matches the oracle on arbitrary safe sets, and the extracted safety
    /// controller keeps every invariant state inside the kernel.
    #[test]
    fn invariance_and_safety_closure((n, a, triples, safe, _, _) in arb_case()) {
        let hash = build(n, a, &triples);
        let idx = hash.clone().freeze();
        let inv = idx.invariance(&safe);
        prop_assert_eq!(&inv.invariant, &invariance_reference(&triples, &safe, n).invariant);
        prop_assert_eq!(&hash.invariance(&safe).invariant, &inv.invariant);

        let ctrl = ControllerStore::from_safety_result(&inv, &idx);
        prop_assert_eq!(ctrl.len(), inv.invariant.len());
        for x in inv.invariant.iter() {
            let p = ctrl.choose(StateId(x as u32)).expect("invariant state has an action");
            prop_assert_eq!(idx.pair_source(p), StateId(x as u32));
            prop_assert!(idx.succs_for_pair(p).iter().all(|s| inv.invariant.contains(s.0 as usize)));
        }
    }

    /// write → parse preserves header, triple multiset order, and checksum.
    #[test]
    fn trace_roundtrip_random((n, a, triples) in arb_system()) {
        let mut stream: Vec<_> = triples.iter().copied().collect();
        stream.extend(stream.clone().into_iter().take(3)); // duplicates pass through
        let header = TraceHeader {
            n_states: n,
            n_inputs: a,
            state_names: (0..n).map(|i| (i % 2 == 0).then(|| format!("s {i}"))).collect(),
            input_names: vec![None; a],
            checksum: None,
        };
        let mut buf = Vec::new();
        write_trace(&mut buf, &header, stream.iter().copied()).unwrap();
        let (h, it) = parse_trace(Cursor::new(&buf)).unwrap();
        let parsed: Vec<_> = it.collect::<Result<_, _>>().unwrap();
        prop_assert_eq!(&parsed, &stream);
        prop_assert_eq!(h.state_names, header.state_names);
        prop_assert_eq!(h.input_names, header.input_names);
        prop_assert_eq!(h.checksum, Some(triple_checksum(triples.iter().copied())));
    }
}

fn five_state() -> IndexedFrozen {
    let t = [
        (0, 0, 1),
        (0, 1, 1),
        (0, 1, 2),
        (1, 0, 0),
        (1, 1, 2),
        (2, 0, 1),
        (2, 0, 3),
        (2, 1, 4),
        (3, 0, 2),
        (3, 1, 4),
        (4, 0, 3),
        (4, 1, 4),
    ];
    let mut b = HashBuilder::new(5, 2);
    for (x, u, y) in t {
        b.add_transition(StateId(x), InputId(u), StateId(y));
    }
    b.freeze()
}

#[test]
fn five_state_controller_is_deterministic_and_roundtrips() {
    let store = five_state();
    let mut target = StateSet::zeros(5);
    target.insert(4);
    let reach = store.reachability(&target, None);
    let ctrl = ControllerStore::from_reach_result(&reach, &store);

    // Lowest-PairId choice: A→L (A-L and A-R both reach rank 3), B→R, C→R, D→R.
    let inputs: HashMap<u32, u32> = (0..4)
        .map(|x| (x, ctrl.get_inputs(StateId(x)).unwrap().0))
        .collect();
    assert_eq!(inputs, HashMap::from([(0, 0), (1, 1), (2, 1), (3, 1)]));
    assert_eq!(
        ctrl.get_inputs(StateId(4)),
        None,
        "target has no progress action"
    );

    let mut buf = Vec::new();
    ctrl.write_to(&mut buf).unwrap();
    assert_eq!(
        String::from_utf8(buf.clone()).unwrap(),
        "0 0\n1 1\n2 1\n3 1\n"
    );
    let back = ControllerStore::read_from(Cursor::new(buf), &store).unwrap();
    assert_eq!(back, ctrl);

    assert!(ControllerStore::read_from(Cursor::new("0 7\n"), &store).is_err());
}

#[test]
fn system_without_transitions() {
    let store = HashBuilder::new(4, 2).freeze();
    let mut target = StateSet::zeros(4);
    target.insert(1);
    let r = store.reachability(&target, None);
    assert_eq!(r.winning, target);
    assert_eq!(r.rank, vec![usize::MAX, 0, usize::MAX, usize::MAX]);
    assert!(store.invariance(&StateSet::ones(4)).invariant.is_empty());
    assert!(store.cpre(&StateSet::ones(4)).is_empty());
    assert_eq!(store.successors(StateId(3), InputId(1)).count(), 0);
    assert!(!store.contains(StateId(0), InputId(0), StateId(0)));
}

#[test]
fn checksum_ignores_order_and_duplicates() {
    let t = |x, u, y| (StateId(x), InputId(u), StateId(y));
    let a = triple_checksum([t(0, 0, 1), t(1, 1, 0)]);
    let b = triple_checksum([t(1, 1, 0), t(0, 0, 1), t(1, 1, 0)]);
    assert_eq!(a, b);
    assert_eq!(a.len(), 64);
    assert_ne!(a, triple_checksum([t(0, 0, 1)]));
    // SHA-256 of the empty stream.
    assert_eq!(
        triple_checksum([]),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn trace_parse_errors_report_line_numbers() {
    let line_of = |src: &str| -> usize {
        let err = match parse_trace(Cursor::new(src.as_bytes().to_vec())) {
            Err(e) => e,
            Ok((_, mut it)) => it.find_map(Result::err).expect("expected an error"),
        };
        match err {
            TraceError::Parse { line, .. } => line,
            TraceError::Io(e) => panic!("unexpected I/O error {e}"),
        }
    };
    assert_eq!(line_of("states 2\n0 0 1\n"), 2, "missing inputs header");
    assert_eq!(
        line_of("states 2\ninputs 1\n\n0 0 1\n0 0 2\n"),
        5,
        "state out of range"
    );
    assert_eq!(
        line_of("states 2\ninputs 1\n0 1 1\n"),
        3,
        "input out of range"
    );
    assert_eq!(line_of("states 2\ninputs 1\n0 0\n"), 3, "short triple");
    assert_eq!(
        line_of("states 2\ninputs 1\nstate 2 X\n"),
        3,
        "named state out of range"
    );
    assert_eq!(line_of("states 2\nstates 3\n"), 2, "duplicate header");
    assert_eq!(line_of("# c\n"), 1, "empty trace");
}

#[test]
fn trace_names_with_spaces_and_trailing_whitespace() {
    let src = "states 2  \ninputs 1\nstate 0   Left  bank  \nstate 1 R\ninput 0 go\n0 0 1   \n# mid\n1 0 0\n";
    let (h, it) = parse_trace(Cursor::new(src.as_bytes())).unwrap();
    assert_eq!(
        h.state_names,
        vec![Some("Left  bank".into()), Some("R".into())]
    );
    assert_eq!(it.count(), 2);

    let bad = TraceHeader {
        n_states: 1,
        n_inputs: 1,
        state_names: vec![Some(" padded".into())],
        input_names: vec![None],
        checksum: None,
    };
    assert!(write_trace(Vec::new(), &bad, []).is_err());
}

#[test]
fn indexed_frozen_is_object_safe() {
    let store = five_state();
    let dynamic: &dyn ControlledTransitionAdt = &store;
    assert_eq!(dynamic.states().len(), 5);
    assert_eq!(dynamic.inputs().len(), 2);
    assert!(!dynamic.has_reverse_incidence());
    assert_eq!(store.find_pair(StateId(2), InputId(1)), Some(PairId(5)));
}
