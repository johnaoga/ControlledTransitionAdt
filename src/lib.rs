//! Phase-specialized data structures for symbolic abstraction-based controller synthesis.
//!
//! See `CLAUDE.md` and `specs.md` for the full implementation guide.

pub mod backend;
pub mod builder;
pub mod controller;
pub mod semantic;
pub mod solver;

#[cfg(test)]
mod conformance_tests;
