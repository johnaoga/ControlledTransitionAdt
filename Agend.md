# CLAUDE.md — phase-specialized-cts

> A representation-independent controlled-transition ADT in Rust, with phase-specialized storage backends and explicit action-hyperedge solvers, targeting reachability and invariance controller synthesis.

## What this is

This crate implements a **finite controlled transition system (CTS)** abstraction `S = (X, U, F)` where `F: X×U → 2^X` (set-valued). The core insight from the research blueprint: SCOTS-style tools use one representation (BDD) for both the mutable build phase (insertion-heavy) and the read-only synthesis phase (CPre-query-heavy). This project separates them into phase-specialized backends behind a common trait, enabling fair empirical comparison of representations.

The end deliverable is a Rust crate + benchmark harness supporting a peer-reviewed empirical study at UCLouvain-INMA-FG.

## Current state

Phases 0–1 implemented. All tests in `tests/` pass; `tests/` is the source of truth and must not be edited without discussing it with the user first. Extra differential/property tests live in `src/conformance_tests.rs` (`#[cfg(test)]`), including HashBuilder ≡ IndexedFrozen on every ADT op, rank optimality, controller progress, safety closure, and trace round-trip.

Local toolchain: `cargo` is not on the default PATH here; use `export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"`.

## Module map

```
src/semantic/mod.rs       — ControlledTransitionAdt trait (the ADT contract)
src/semantic/types.rs     — StateId, InputId, PairId, StateSet, ReachResult, SafetyResult
src/semantic/reference.rs — cpre_reference / reachability_reference / invariance_reference (oracle)
src/builder/mod.rs        — TransitionBuilder trait
src/builder/hash_builder.rs — HashBuilder: HashMap<(StateId,InputId), HashSet<StateId>>
src/builder/trace_io.rs   — Appendix C text trace parser + serializer
src/backend/mod.rs        — Backend trait; freeze() interface
src/backend/indexed_frozen.rs — FrozenTransitionStore: forward CSR (Phase 1)
src/backend/bdd.rs        — BddBackend stub (feature="oxidd", Phase 3, not wired)
src/solver/attractor.rs   — reachability(): counter-based event-driven (Listing 2)
src/solver/invariance.rs  — invariance(): dual deletion algorithm (Listing 3)
src/controller/store.rs   — ControllerStore: rank table + chosen-action lookup
src/bin/five_state.rs     — Appendix B smoke test
src/bin/replay.rs         — trace replay binary
```

## What to implement

### Feature 1: ControlledTransitionAdt trait (semantic/mod.rs)
- Tests: `tests/freeze_equivalence.rs`
- Stub: `src/semantic/mod.rs`
- What: Define the trait with all methods from the blueprint Listing 1. Note: `impl Iterator` return types break object safety — use `Box<dyn Iterator<...>>` or define associated iterator types.

### Feature 2: Reference CPre + solvers (semantic/reference.rs)
- Tests: `tests/conformance.rs` (reference IS the oracle; test that it matches expected on five-state)
- Stub: `src/semantic/reference.rs`
- What: `cpre_reference(triples: &HashSet<(StateId,InputId,StateId)>, y: &StateSet, n_states: usize) -> StateSet` — direct port of blueprint §6.7 Python. `reachability_reference` loops cpre until fixed point. `invariance_reference` computes maximal controlled invariant by iterating CPre-inside.
- Key: These are FULLY IMPLEMENTED (not stubs) — they are the oracle.

### Feature 3: Trace format (builder/trace_io.rs)
- Tests: `tests/trace_roundtrip.rs`
- Stub: `src/builder/trace_io.rs`
- What: Parse Appendix C format: `# comment`, `states N`, `inputs A`, `state ID NAME`, `input ID NAME`, `X U Y` triple lines. Return `TraceHeader` + iterator of triples. Serializer writes the same format with a SHA-256 checksum comment.

### Feature 4: HashBuilder (builder/hash_builder.rs)
- Tests: `tests/freeze_equivalence.rs`
- Stub: `src/builder/hash_builder.rs`
- What: `add_transition(x, u, y)` stores into `HashMap<(StateId,InputId), HashSet<StateId>>`. `freeze(self) -> IndexedFrozen`: sort pairs by (source, input), assign dense PairIds, build CSR arrays.

### Feature 5: IndexedFrozen (backend/indexed_frozen.rs)
- Tests: `tests/five_state.rs`, `tests/freeze_equivalence.rs`
- Stub: `src/backend/indexed_frozen.rs`
- Layout: `state_pair_off: Vec<u64>` (N+1), `pair_source: Vec<StateId>` (H), `pair_input: Vec<InputId>` (H), `pair_succ_off: Vec<u64>` (H+1), `succ: Vec<StateId>` (M). `rev_off`, `rev_pair` are `None` in Phase 1.
- `successors(x, u)`: find pair by binary search in `[state_pair_off[x]..state_pair_off[x+1]]`, then slice `succ`.
- `contains(x, u, y)`: binary search in successors slice.
- `cpre(y)`: full scan all pairs — O(M+H); no reverse index needed.

### Feature 6: Reachability attractor (solver/attractor.rs)
- Tests: `tests/five_state.rs` — must return winning={A,B,C,D,E}, rank E=0,C=D=1,B=2,A=3
- What: Listing 2 of blueprint. `remaining[p] = succ_count(p)`. Queue starts with target states. For each dequeued y: scan all pairs (Phase 1 full scan), decrement remaining for each pair with y in successors, if remaining hits 0 add source to winning. Returns `ReachResult`.
- Note: Phase 1 runs O(k(M+H)); Phase 2 upgrade uses reverse index. Document this.

### Feature 7: Invariance solver (solver/invariance.rs)
- Tests: `tests/five_state.rs` — invariant of {A,B,C,E} should be {A,B,C,E} (all safe, since all have a staying action)
- What: Listing 3. `bad[p]` = successors outside safe. `good_actions[x]` = pairs with bad=0. Queue states where good_actions=0. On removal of y: scan reverse (Phase 1: full scan) to update bad/good.

### Feature 8: ControllerStore (controller/store.rs)
- Tests: `tests/five_state.rs`
- What: Wraps `HashMap<StateId, PairId>` (chosen action per state) + `pair_input: Vec<InputId>` (from IndexedFrozen). `get_inputs(x) -> Option<InputId>`. `choose(x) -> Option<PairId>`.

## Five-state example (Appendix B) — canonical test
States: A(0) B(1) C(2) D(3) E(4). Inputs: L(0) R(1).
Transitions:
  A-L→{B}, A-R→{B,C}, B-L→{A}, B-R→{C}, C-L→{B,D}, C-R→{E},
  D-L→{C}, D-R→{E}, E-L→{D}, E-R→{E}
Target: {E}
Expected reachability: winning={A,B,C,D,E}, rank E=0, C=D=1, B=2, A=3
Expected invariance of {A,B,C,E}: invariant={A,B,C,E}

## How to run tests

```bash
cargo test                   # all tests
cargo test five_state        # integration test
cargo test conformance       # proptest differential
cargo bench                  # criterion microbenchmarks
cargo run --bin five_state   # smoke test binary
cargo run --bin replay -- trace.txt  # replay a trace file
```

## Decisions already made

| Decision | Choice | Reason |
|----------|--------|--------|
| Language | Rust (not C++17/20) | No need for SCOTS/CUDD C++ ABI compat in Phase 1; Rust memory safety ideal for data-structure research |
| BDD | oxidd (feature-gated, Phase 3) | Pure-Rust BDD/MDD/LDD; wired in Cargo.toml but not implemented |
| State bounds | Explicit at construction (n_states, n_inputs) | Avoids inferring bounds from max id seen; cleaner API |
| StateSet | DenseBitVec (Vec<u64>) | Default; roaring optional later |
| Reverse CSR | Deferred to Phase 2 | Phase 1 runs O(k(M+H)) without it; forward-only structure accommodates it with one extra pass |
| Solver scan granularity | One full pair scan per rank layer (reach) / per deletion sweep (invariance), not per dequeued state | Matches the documented O(k(M+H)) bound with k = rounds; per-state scans would be O(N(M+H)). Live-pair lists shrink each round |
| Reach ranks / controller | rank = layer index = 1 + max succ rank (optimal); chosen = lowest PairId firing at that layer | Deterministic; verified against layered-CPre definition |
| Trace checksum | `# checksum: <hex>` = SHA-256 over sorted, deduplicated triples, each 12 bytes big-endian `x‖u‖y` | Order/duplicate/endianness independent (spec open question 4) |
| Trace names | Name = rest of line after id, trimmed (spaces allowed, no quoting); header name vecs always have length N / A | Simplest unambiguous convention |
| `parse_trace` signature | Returns `Box<dyn Iterator + 'a>` bound to the reader's lifetime | Lazy parsing of borrowed readers (`Cursor<&Vec<u8>>` in tests) |
| HashBuilder as ADT | Implements `ControlledTransitionAdt` (for `hash` benchmarks); pre-freeze PairIds use grid encoding `x*A+u` (`pair_of` decodes) | Builder has no dense pair numbering; ids are not comparable with frozen PairIds |

## Things to be careful about

- **Trait object safety**: `impl Iterator` in trait methods breaks object safety. Use `Box<dyn Iterator<Item=StateId>>` etc. in the trait, or use GATs with `type SuccIter<'a>: Iterator<Item=StateId>`.
- **Phase 1 solver complexity**: Both attractor and invariance do a full scan of all pairs per step — O(k(M+H)), not O(M+H+N). This is intentional and documented; Phase 2 adds the reverse index.
- **Rank assignment**: A state enters winning only when all successors are already winning (remaining=0), so rank assignment is safe: `rank[x] = 1 + max(rank[z] for z in succ(chosen_pair))`. No cycle issues.
- **Self-loops**: E-R→{E} in the five-state example. Target states start winning at rank 0; self-loop means remaining stays 0 after removing target's self-count? Be careful: initialize remaining[p] = |Succ(p)|; when we dequeue E and process E-R (which has E as successor), we decrement remaining[E-R]. If remaining[E-R] = 0 and source(E-R)=E is already winning, skip. This is fine.
- **Empty transitions**: `cpre_reference` and all backends must handle states with no outgoing pairs (not in any pair map).

## Out of scope (don't implement these)

- Reverse CSR (`rev_off`, `rev_pair`) — Phase 2
- BDD backend (`oxidd`) — Phase 3
- MDD/LDD, hybrid backend — Phases 3, 6
- SCOTS integration, plant simulator — Phase 4
- Incremental refinement — Phase 7

## Success

All tests pass. The five-state integration test (`cargo test five_state`) is the primary acceptance criterion.
