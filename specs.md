# Phase-Specialized Data Structures for Symbolic Abstraction-Based Control
> A representation-independent controlled-transition ADT in Rust, with phase-specialized storage backends and explicit action-hyperedge solvers, targeting reachability and invariance controller synthesis.

## Source
**Document**: "Phase-Specialized Data Structures for Symbolic Abstraction-Based Control — A Research Blueprint for a Representation-Independent Controlled-Transition ADT, Explicit Action-Hypergraph Solvers, and Hybrid Decision-Diagram Backends" (September 2026, 30 pages)
**Origin**: Academic research blueprint / pre-paper
**Relationship to source**: Implementing as described, Phases 0–1 of the 9-phase roadmap. Later phases (symbolic backends, crossover study, incremental updates) are out of scope for this sprint but the architecture must accommodate them without refactoring.

---

## Overview
**Problem**: Symbolic abstraction-based control tools (e.g., SCOTS) encode transition relations with a single representation (BDD) across all computational phases, even though construction is insertion-heavy/mutable while synthesis is read-heavy and repeatedly queries successors, predecessors, and controllable predecessors. Treating one backend as the representation rather than one implementation of an ADT's operations obscures this mismatch and makes fair representation comparison impossible.

**Users**: John Aoga (solo researcher at UCLouvain-INMA-FG). The deliverable is a reusable Rust library and benchmark harness that supports a peer-reviewed empirical study.

**Success criteria**: At the end of Phases 0–1, the project has:
1. A published Rust crate with the `ControlledTransitionAdt` trait, reference CPre implementation, and benchmark trace I/O.
2. Two working backends — `HashBuilder` (mutable, insertion-oriented) and `IndexedFrozen` (forward CSR, read-oriented) — both passing conformance tests against the reference.
3. Microbenchmarks for all ADT operations on both backends, reproducible via `cargo bench` on the five-state example and at least one medium-scale synthetic instance.

---

## Scope

### In scope (Phases 0–1)
- **Semantic layer**: Rust trait `ControlledTransitionAdt` and associated types (`StateSet`, `InputSet`, `PairSet`, `ReachResult`, `SafetyResult`), faithfully transcribing Listing 1 of the blueprint.
- **Reference CPre**: A slow but obviously correct `cpre_reference` function operating on a `HashMap`-backed triple store; used as the oracle for all conformance tests.
- **Benchmark trace format**: Parser and serializer for the text format in Appendix C (header + `state`/`input`/`transition` records); binary variant optional but not required in Phase 1.
- **`HashBuilder` backend**: Mutable `HashMap<(StateId, InputId), Vec<StateId>>` storing `(x, u) → Succ(x,u)`, with optional deduplication, implementing the builder half of the ADT.
- **`IndexedFrozen` backend** (forward only): Compact CSR-like layout — `state_pair_off`, `pair_source`, `pair_input`, `pair_succ_off`, `succ` — built from a completed builder via a `freeze()` call. O(M + H + N) construction.
- **Operation microbenchmarks**: `criterion`-based benchmarks for `contains`, `successors`, `predecessor_pairs`, `admissible_inputs`, `safe_actions`, `cpre`, and a full reachability/invariance run.
- **Reachability attractor** (explicit): Counter-based event-driven solver (Listing 2 of blueprint) specialised to the action-hyperedge structure. Runs on `IndexedFrozen` only for Phase 1.
- **Invariance solver** (explicit): Dual deletion algorithm (Listing 3 of blueprint). Runs on `IndexedFrozen` only for Phase 1.
- **Controller extraction**: Rank-based deterministic progress controller returned alongside the winning set from the reachability solver.
- **Differential test harness**: Property-based tests (via `proptest`) comparing every backend's `cpre` / reachability / invariance output to the reference on randomly generated small systems.

### Out of scope for Phases 0–1
- **Reverse CSR (`rev_off` / `rev_pair`) in `IndexedFrozen`**: Deferred to Phase 2; the forward-only structure must be designed to accommodate it with a single additional pass.
- **BDD backend** (`oxidd`): Deferred to Phase 3. Dependency added to `Cargo.toml` behind a feature flag so it compiles from day one but is not wired.
- **MDD/LDD backend**: Deferred to Phase 3.
- **Hybrid backend**: Deferred to Phase 6.
- **SCOTS integration / plant simulator**: Deferred to Phase 4.
- **Incremental refinement**: Deferred to Phase 7.
- **GPU / parallel build**: Not in scope for any early phase.

---

## Technical Stack

| Component | Choice | Reason |
|-----------|--------|--------|
| Language | Rust 2021 edition | Memory safety, zero-cost abstractions, ideal for data-structure research with C-level benchmarks |
| Build | Cargo | Standard; `cargo bench` drives reproducible benchmarks |
| Benchmarking | `criterion` | Statistical microbenchmarks; HTML reports; standard in Rust perf work |
| Property testing | `proptest` | Randomized small-instance conformance testing |
| BDD (future) | `oxidd` (feature-gated) | Pure-Rust BDD/MDD/LDD; no C FFI; add `oxidd = { version = "...", optional = true }` |
| State sets | `roaring` crate (optional) | Roaring bitmaps for compressed adaptive state sets when winning/frontier sets are sparse; `DenseBitVec` (plain `Vec<u64>`) as default |
| Serialization | `serde` + `serde_json` | Controller serialization; trace format uses a hand-rolled line parser |
| Testing | `cargo test` + `proptest` | Unit + property-based conformance |
| CI | GitHub Actions | `cargo test`, `cargo clippy`, `cargo fmt --check` on push |

No `unsafe` code except where explicitly justified and documented (e.g., a future SIMD bitmap operation). All cross-module APIs are safe Rust.

---

## Architecture

The crate is structured as five modules, mirroring the blueprint's five-layer architecture, adapted to Rust idioms:

```
phase-specialized-cts/
├── src/
│   ├── lib.rs              # re-exports; feature flags
│   ├── semantic/           # Layer 1
│   │   ├── mod.rs          # ControlledTransitionAdt trait; StateId/InputId/PairId newtypes
│   │   ├── types.rs        # StateSet, InputSet, PairSet, ReachResult, SafetyResult
│   │   └── reference.rs    # cpre_reference; reachability_reference; invariance_reference
│   ├── builder/            # Layer 2
│   │   ├── mod.rs          # TransitionBuilder trait
│   │   ├── hash_builder.rs # HashBuilder: HashMap-backed mutable store
│   │   └── trace_io.rs     # Trace parser + serializer (Appendix C format)
│   ├── backend/            # Layer 3
│   │   ├── mod.rs          # Backend trait; freeze() interface
│   │   ├── indexed_frozen.rs # FrozenTransitionStore: forward CSR (Phase 1)
│   │   └── bdd.rs          # BddBackend stub (feature = "oxidd", Phase 3)
│   ├── solver/             # Layer 4
│   │   ├── mod.rs
│   │   ├── attractor.rs    # reachability(): counter-based event-driven (Listing 2)
│   │   └── invariance.rs   # invariance(): dual deletion algorithm (Listing 3)
│   └── controller/         # Layer 5
│       ├── mod.rs
│       └── store.rs        # ControllerStore: rank table + chosen-action lookup
├── benches/
│   └── operations.rs       # criterion benchmark suite
├── tests/
│   ├── five_state.rs       # Appendix B example as integration test
│   ├── conformance.rs      # proptest differential tests
│   └── trace_roundtrip.rs  # parse → serialize → re-parse identity
└── Cargo.toml
```

**Lifecycle** (exactly as blueprint §4.1):

```
plant / transition generator
        │ addTransition(x, u, y) calls
        ▼
   TransitionBuilder (HashBuilder)
        │ freeze()
        ▼
   FrozenTransitionStore (IndexedFrozen)
        │ reachability() / invariance()
        ▼
   ControllerStore
        │ getInputs(x) / choose(x)
        ▼
   deployment / lookup
```

No principle requires the three representations to be the same type or even the same memory layout.

---

## Features (Detailed)

### F1 — `ControlledTransitionAdt` Trait
**What it does**: Defines the representation-independent public interface for a finite controlled transition system S = (X, U, F). All backends implement this trait; solvers are generic over it.

**Inputs**: Implemented by backend structs; called by solvers and benchmarks.

**Outputs / behavior**:
```rust
pub trait ControlledTransitionAdt {
    fn states(&self) -> &StateSet;
    fn inputs(&self) -> &InputSet;

    fn contains(&self, x: StateId, u: InputId, y: StateId) -> bool;
    fn successors(&self, x: StateId, u: InputId) -> impl Iterator<Item = StateId>;
    fn predecessor_pairs(&self, y: StateId) -> impl Iterator<Item = (StateId, InputId)>;
    fn admissible_inputs(&self, x: StateId) -> impl Iterator<Item = InputId>;

    fn post(&self, s: &StateSet, filter: Option<&InputSet>) -> StateSet;
    fn safe_actions(&self, y: &StateSet) -> PairSet;
    fn cpre(&self, y: &StateSet) -> StateSet;

    fn reachability(&self, target: &StateSet, constraint: Option<&StateSet>) -> ReachResult;
    fn invariance(&self, safe: &StateSet) -> SafetyResult;

    // Capability flags (optional; default false)
    fn has_reverse_incidence(&self) -> bool { false }
    fn has_native_cpre(&self) -> bool { false }
    fn supports_incremental_updates(&self) -> bool { false }
}
```

**Edge cases**: Trait object safety — some methods return `impl Iterator`; use `Box<dyn Iterator<...>>` or associated types as needed to maintain object safety for dynamic dispatch in the benchmark harness.

**Source**: Listing 1 of blueprint (directly transcribed, adapted to Rust trait syntax).

---

### F2 — Reference CPre and Reference Solvers
**What it does**: A slow, obviously correct implementation of CPre, reachability, and invariance operating on a `HashMap`-backed triple set. Used exclusively as the conformance oracle; never benchmarked as a competitor.

**Inputs**: A `HashSet<(StateId, InputId, StateId)>` of triples plus a target/safe `StateSet`.

**Outputs / behavior**: Identical semantics to `ControlledTransitionAdt` methods. `cpre_reference` is a direct port of Listing 6.7 of the blueprint (Python → Rust). `reachability_reference` is a fixed-point loop over `cpre_reference`.

**Edge cases**: Must handle empty transition systems; must not panic on states with no outgoing transitions.

**Source**: §6.7, Appendix A of blueprint.

---

### F3 — Benchmark Trace Format (Parser + Serializer)
**What it does**: Reads and writes the text trace format defined in Appendix C. Enables replay: generate a canonical `(x, u, y)` triple stream once, then feed the identical stream to every backend.

**Inputs** (parser): A `BufRead` stream. Handles `#`-prefixed comment lines, `states N`, `inputs A`, `state id name`, `input id name`, `N U Y` triple lines.

**Outputs** (parser): A `TraceHeader` (N, A, optional name maps) and an iterator of `(StateId, InputId, StateId)` triples.

**Outputs** (serializer): Writes a conforming text trace. The canonical sorted triple stream checksum (sorted by `(x, u, y)`, SHA-256) is written as a comment in the header so two backends can verify they solved identical relations.

**Edge cases**: Lines with trailing whitespace; state/input names containing spaces (quoted or not — pick one convention and document it); duplicate triples (parser must pass them through; deduplication is the builder's job).

**Source**: Appendix C of blueprint; §5.1 (replay pipeline principle).

---

### F4 — `HashBuilder` Backend
**What it does**: A mutable, insertion-oriented store. Accepts `addTransition(x, u, y)` calls in any order, deduplicates on insert (via `HashSet` per action pair), and can be frozen into `IndexedFrozen`.

**Inputs**: `addTransition(x: StateId, u: InputId, y: StateId)` calls; `freeze() -> IndexedFrozen`.

**Outputs / behavior**:
```rust
pub struct HashBuilder {
    n_states: usize,
    n_inputs: usize,
    // (x, u) -> HashSet<y>   — O(1) expected insert, auto-deduplicates
    pairs: HashMap<(StateId, InputId), HashSet<StateId>>,
}
impl HashBuilder {
    pub fn add_transition(&mut self, x: StateId, u: InputId, y: StateId);
    pub fn freeze(self) -> IndexedFrozen;
}
```

Expected O(M) total insertion. `freeze()` is O(M + H + N): sorts action pairs by `(x, u)`, assigns dense `PairId`s, builds the forward CSR arrays.

**Edge cases**: States/inputs referenced in transitions but never declared with `state`/`input` in the trace — infer from max id seen, or require explicit bounds passed at construction. Choose one; document it. (Recommendation: require `n_states` and `n_inputs` at construction so bounds are always explicit.)

**Source**: §4.1, §4.6 (`HashDynamic` description), §6.3 of blueprint.

---

### F5 — `IndexedFrozen` Backend (Forward CSR)
**What it does**: The read-optimized frozen representation. After `freeze()`, the transition relation is immutable. Provides O(1+d⁺) `successors`, O(1) `contains` (with local bitmap), and O(M+H) `safe_actions`/`cpre` by full scan. No reverse index in Phase 1.

**Data layout** (from §6.2 of blueprint, adapted to Rust):
```rust
pub struct IndexedFrozen {
    n_states: usize,
    n_inputs: usize,
    n_pairs: usize,   // H = |P|
    n_incidences: usize, // M = |T|

    // state -> action-pair range: pairs[state_pair_off[x]..state_pair_off[x+1]]
    state_pair_off: Vec<u64>,  // len N+1

    // pair metadata
    pair_source: Vec<StateId>, // len H
    pair_input:  Vec<InputId>, // len H

    // pair -> successor range: succ[pair_succ_off[p]..pair_succ_off[p+1]]
    pair_succ_off: Vec<u64>,   // len H+1
    succ: Vec<StateId>,         // len M

    // Phase 2 placeholder (None in Phase 1)
    rev_off:  Option<Vec<u64>>,  // len N+1 when present
    rev_pair: Option<Vec<PairId>>, // len M when present
}
```

`contains(x, u, y)`: Look up the pair id for `(x, u)` (binary search in `state_pair_off` range), then binary search in its successor slice — O(log d⁺). Alternatively, a per-pair `DenseBitVec` (N/64 words) allows O(1) membership; flag this as a configurable option since it trades memory for speed.

**Source**: §6.2, §6.3 of blueprint.

---

### F6 — Reachability Attractor Solver
**What it does**: Implements the counter-based event-driven reachability fixed-point (Listing 2 of blueprint). Runs on `IndexedFrozen`. Returns the winning state set, the rank of each winning state, and the chosen progress action for each state.

**Inputs**: `target: &StateSet`, `succ: &IndexedFrozen`.

**Outputs**: `ReachResult { winning: StateSet, rank: Vec<usize>, chosen: HashMap<StateId, PairId> }`.

**Algorithm** (Phase 1, no reverse index):
- Initialize `remaining[p] = succ_count(p)` for all pairs `p`.
- For each `y` dequeued: iterate over all pairs `p` (full scan — O(M) per step in Phase 1; O(d⁻) in Phase 2 with reverse index).
- If `remaining[p]` reaches 0 and `source[p]` is not yet winning, add it.
- Rank assignment: `rank[x] = 1 + max(rank[z] for z in succ(chosen_pair))`.

**Complexity** (Phase 1, no reverse index): O(k(M+H)) where k ≤ N fixed-point rounds. Phase 2 upgrade to O(M+H+N) by adding the reverse CSR. Document this asymptotic gap explicitly.

**Edge cases**: Cycles in the winning region must not create infinite rank. The rank assignment in the blueprint prevents this: a state is added to winning only once `remaining[p] = 0`, guaranteeing all successors already have finite rank. Self-loops at target states are handled by the initialization (target states start winning at rank 0).

**Source**: §4.3, Listing 2, Proposition 4.1, Appendix A.2 of blueprint.

---

### F7 — Invariance Solver
**What it does**: Implements the dual deletion algorithm for the controlled invariant subset of a safe set (Listing 3 of blueprint). Returns the maximal controlled invariant subset `Y* ⊆ S`.

**Inputs**: `safe: &StateSet`, `store: &IndexedFrozen`.

**Outputs**: `SafetyResult { invariant: StateSet }`.

**Algorithm**: Initialize `bad[p]` = number of successors of `p` outside `S`. Initialize `good_actions[x]` = number of pairs at `x` with `bad[p] = 0`. Any state with `good_actions[x] = 0` enters the deletion queue. When a state `y` is removed from `Y`, iterate over its reverse incidence to update predecessor counters.

**Complexity** (Phase 1): O(k(M+H)) by full scan to rebuild bad/good counters after each deletion round. Upgrade to O(M+H+N) in Phase 2 with reverse index.

**Source**: §4.4, Listing 3, Appendix A.3 of blueprint.

---

### F8 — Controller Store
**What it does**: Stores the synthesized controller as a lookup table. For reachability: maps each winning state to its chosen progress action (the `PairId` with `remaining = 0` and rank assignment). For invariance: maps each invariant state to any safe action pair.

**Inputs**: Built by the solvers; queried at deployment.

**Outputs**:
```rust
pub struct ControllerStore {
    chosen: HashMap<StateId, PairId>,
    pair_input: Vec<InputId>,  // from IndexedFrozen
}
impl ControllerStore {
    pub fn get_inputs(&self, x: StateId) -> Option<InputId>;
    pub fn choose(&self, x: StateId) -> Option<InputId>;
}
```

**Edge cases**: States not in the winning/invariant set return `None`. Multiple valid actions for the same state: the solver picks one deterministically (e.g., the lowest `PairId`). Serialization: write as `state_id input_id` lines; assert `bit-for-bit` semantic equality after round-trip.

**Source**: §2.3 (`Controller` interface), §6.8 (`Serialization test`) of blueprint.

---

### F9 — Microbenchmark Suite
**What it does**: `criterion`-based benchmarks for every ADT operation, run on both `HashBuilder` and `IndexedFrozen`, on the five-state example (Appendix B) and a configurable synthetic instance.

**Benchmark groups**:
- `build/{hash,indexed}` — total insertion + freeze time for M incidences
- `query/contains/{hash,indexed}` — single membership test
- `query/successors/{hash,indexed}` — enumerate Succ(x, u)
- `query/predecessor_pairs/{indexed}` — enumerate Rev(y) (forward-only: O(M) scan)
- `query/safe_actions/{hash,indexed}` — full scan
- `query/cpre/{hash,indexed}` — full scan
- `solve/reachability/{indexed}` — full attractor run
- `solve/invariance/{indexed}` — full invariance run

**Source**: §5.5 (dependent variables), §6.8 (performance regression) of blueprint.

---

## Data Model

```
StateId  = newtype(u32)   // dense 0..N-1; N = |X|
InputId  = newtype(u32)   // dense 0..A-1; A = |U|
PairId   = newtype(u32)   // dense 0..H-1; H = |P| = |admissible (x,u) pairs|

StateSet = DenseBitVec (Vec<u64>, N bits) | RoaringSet (roaring::RoaringBitmap)
           — unified behind a trait; solver hot path keeps a plain Vec<bool> for O(1) membership
InputSet = DenseBitVec (A bits)
PairSet  = DenseBitVec (H bits) | Vec<(StateId, InputId)>

TraceHeader {
    n_states: usize,
    n_inputs: usize,
    state_names: Option<Vec<String>>,  // index = StateId
    input_names: Option<Vec<String>>,  // index = InputId
    checksum: Option<String>,          // SHA-256 of sorted triple stream, hex
}

ReachResult {
    winning: StateSet,
    rank: Vec<usize>,                  // rank[i] = rank of StateId(i); usize::MAX = not winning
    chosen: HashMap<StateId, PairId>,
}

SafetyResult {
    invariant: StateSet,
}
```

**Identifier spaces**: All three dense id spaces (`StateId`, `InputId`, `PairId`) are assigned at freeze time and are stable across all operations on a given `FrozenTransitionStore`. They must not be assumed to match any coordinate-space index used by an abstraction generator.

---

## Error Handling

The library is a research tool, not a production service. Errors fall into two categories:

**Programming errors** (bugs): Panics with descriptive messages are acceptable for out-of-bounds state/input ids, inconsistent freeze (e.g., transition references an undeclared state), or a malformed trace file. Use `assert!` / `expect("…")` liberally during Phase 0–1. These will be hardened in later phases.

**Runtime / I/O errors**: Trace file parse errors return `Result<_, TraceParseError>` where `TraceParseError` is a proper enum with line number and reason. Solver functions do not return `Result`; they assume a valid frozen store (guaranteed if built through the API).

**Logging**: No runtime logging in Phase 1. Benchmarks print timing to stdout via `criterion`. A `--verbose` flag on the example binary prints iteration-by-iteration winning-set size (useful during debugging).

---

## Testing Strategy

Directly mapping §6.8 of the blueprint:

**ADT conformance** (`tests/conformance.rs`):
- `proptest`: generate random `(N, A, M)` systems (N ≤ 20, A ≤ 5, M ≤ 200). For every randomized operation sequence (`contains`, `successors`, `cpre`, `reachability`, `invariance`), assert that `HashBuilder` and `IndexedFrozen` return identical semantic answers as `cpre_reference`.

**Five-state integration test** (`tests/five_state.rs`):
- Hardcode the Appendix B example. Assert: winning set = all five states; `rank[E]=0`, `rank[C]=rank[D]=1`, `rank[B]=2`, `rank[A]=3`; controlled invariant = `{A,B,C,E}`. This must pass before any benchmark is interpreted.

**Freeze equivalence** (`tests/freeze_equivalence.rs`):
- Assert that `HashBuilder::freeze()` produces the exact same set of unique triples as the input (no phantom transitions added, no transitions dropped).

**Trace round-trip** (`tests/trace_roundtrip.rs`):
- Parse a trace → serialize → re-parse; assert identical `TraceHeader` and triple stream. Also assert checksum matches.

**Controller progress test** (inline in `five_state.rs`):
- For every winning state x with `rank[x] > 0`, the chosen action's successors must all have `rank < rank[x]`.

**Safety closure test** (inline in `five_state.rs`):
- Every action returned for an invariant state must have all successors inside the invariant kernel.

**Performance regression** (`benches/operations.rs`):
- Not a correctness gate; `cargo bench` generates HTML reports. Add a CI step that runs benchmarks on a fixed synthetic instance and records wall time to a JSON artifact for trend tracking.

---

## Deployment

This is a library crate (`lib.rs`), not a service. Deployment means:

1. **Local use**: `cargo build --release` in the repo. Researchers run `cargo bench` and `cargo test`.
2. **Example binary**: `src/bin/five_state.rs` — runs the Appendix B example end-to-end and prints the winning set, ranks, and invariant set. Acts as a smoke test.
3. **Trace replay binary**: `src/bin/replay.rs` — reads a `.trace` file from stdin, builds both backends, runs reachability/invariance on each, prints timing and asserts winning-set equality. This is the core artifact for the crossover study (Phase 5).
4. **No deployment target, no container, no server**. Future phases may add a Python FFI (via `pyo3`) for integration with SCOTS-style abstraction generators.

---

## Open Questions

1. **`contains` implementation**: Binary search in successor slice (O(log d⁺)) vs. per-pair dense bitmap (O(1), extra N/64 words per pair). Decide before Phase 2 because it affects the memory accounting in the crossover study. Recommendation: implement both behind a compile-time feature flag and benchmark.

2. **Reverse index in Phase 1 or defer?**: The blueprint keeps it in Phase 2, but the reachability solver in Phase 1 pays O(k(M+H)) instead of O(M+H+N). For large benchmarks this may be too slow to be useful. Consider implementing a forward-only reverse scan in Phase 1 as an intermediate (iterate all pairs, check if `y ∈ succ[p]`) and adding the true reverse CSR in Phase 2.

3. **`StateSet` interface**: Should `DenseBitVec` and `RoaringSet` be unified behind a trait (`trait StateSet: …`) or use an enum? A trait has dynamic dispatch overhead on the solver hot path; an enum is closed. Given solo use, an enum is simpler for Phase 1. Document the trade-off.

4. **Checksum in trace format**: SHA-256 of the sorted triple stream is proposed. Decide on exact byte encoding of each triple (big-endian u32 pairs? text lines?) before the first trace file is generated, to ensure reproducibility across machines.

5. **`oxidd` API stability**: The `oxidd` crate is relatively new. Pin to an exact version in `Cargo.toml` and track their changelog. The BDD backend (Phase 3) should be written against `oxidd`'s `BddManager` trait, not internal types.

---

## Decisions Log

| Decision | Choice | Reason |
|----------|--------|--------|
| Language | Rust (not C++17/20 as blueprinted) | Memory safety, Cargo ecosystem, no need for SCOTS/CUDD C++ ABI compatibility in Phase 1 |
| BDD library | `oxidd` (feature-gated, Phase 3) | Pure-Rust, no C FFI; object-safe manager trait |
| Build system | Cargo | Language-standard; `cargo bench`, `cargo test`, `cargo clippy` cover all CI needs |
| Codebase | Greenfield new repo | No prior code to integrate with |
| Scope | Phases 0–1 only (this spec) | Research roadmap has 9 phases; later phases are exploratory and not yet specifiable at this level |
| Reverse CSR | Deferred to Phase 2 | Adds O(N) memory; Phase 1 correctness does not require it; Phase 1 solver pays O(k(M+H)) instead of O(M+H+N) — acceptable for small instances |
| Conformance testing | `proptest` differential tests vs. `cpre_reference` | Blueprint §6.8 explicitly requires this before any performance claims are considered (Gate 1) |
| Error handling | Panics for programming errors; `Result` for I/O | Research tool; hardening deferred; panics surface bugs fast |
| Solo project | No collaborative tooling required in Phase 1 | All API decisions can be revisited when collaborators join |
