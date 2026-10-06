# phase-specialized-cts

Phase-specialized data structures for symbolic abstraction-based controller synthesis.

A representation-independent `ControlledTransitionAdt` trait in Rust, with phase-specialized backends (`HashBuilder` for insertion, `IndexedFrozen` for synthesis) and explicit action-hyperedge solvers (reachability attractor, invariance dual-deletion).

**Status**: Phases 0–1 implemented; all tests pass.

## Quick start

```bash
cargo test            # unit, integration and proptest conformance tests
cargo run --bin five_state -- --verbose     # five-state smoke test (Appendix B)
cargo run --release --bin replay -- t.trace # replay a trace on both backends (or stdin)
cargo bench                                 # criterion suite; PSCTS_BENCH_SIDE=200 for a bigger grid
```

## Architecture

See [CLAUDE.md](CLAUDE.md) for full implementation guide and [specs.md](specs.md) for the research spec.

## Phases

| Phase | Scope | Status |
|-------|-------|--------|
| 0 | ADT contract + reference oracle | Done |
| 1 | HashBuilder, IndexedFrozen, solvers, benchmarks | Done |
| 2 | Reverse CSR → O(M+H+N) solvers | Future |
| 3 | oxidd BDD/MDD/LDD backends | Future |
| 4–8 | SCOTS integration, crossover, hybrid, incremental, paper | Future |

## License

MIT
