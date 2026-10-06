//! ControllerStore: chosen-action lookup table.
//!
//! See specs.md § F8 — Controller Store

use std::collections::HashMap;
use std::io::{self, BufRead, Write};

use crate::backend::indexed_frozen::IndexedFrozen;
use crate::semantic::types::{InputId, PairId, ReachResult, SafetyResult, StateId, StateSet};

/// Synthesized controller: maps winning states to their chosen input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControllerStore {
    /// Chosen action pair for each winning/invariant state.
    chosen: HashMap<StateId, PairId>,
    /// Input id for each pair (from IndexedFrozen).
    pair_input: Vec<InputId>,
}

impl ControllerStore {
    /// Build a controller store from a reachability result (the rank-decreasing
    /// progress actions chosen by the attractor; target states have no entry).
    pub fn from_reach_result(result: &ReachResult, store: &IndexedFrozen) -> Self {
        Self {
            chosen: result.chosen.clone(),
            pair_input: store.pair_inputs().to_vec(),
        }
    }

    /// Build a controller store from a safety result: each invariant state gets its
    /// lowest-PairId action whose successors all stay inside the invariant.
    pub fn from_safety_result(result: &SafetyResult, store: &IndexedFrozen) -> Self {
        let inv: &StateSet = &result.invariant;
        let chosen = inv
            .iter()
            .filter_map(|x| {
                let (start, end) = store.pair_range_for_state(StateId(x as u32));
                (start..end)
                    .find(|&p| store.pair_inside(p, inv))
                    .map(|p| (StateId(x as u32), PairId(p as u32)))
            })
            .collect();
        Self {
            chosen,
            pair_input: store.pair_inputs().to_vec(),
        }
    }

    /// Returns the chosen input for state x, if x is in the winning/invariant set.
    pub fn get_inputs(&self, x: StateId) -> Option<InputId> {
        let pair = *self.chosen.get(&x)?;
        self.pair_input.get(pair.0 as usize).copied()
    }

    /// Returns the chosen pair id for state x.
    pub fn choose(&self, x: StateId) -> Option<PairId> {
        self.chosen.get(&x).copied()
    }

    /// Number of states with a chosen action.
    pub fn len(&self) -> usize {
        self.chosen.len()
    }

    /// True if no state has a chosen action.
    pub fn is_empty(&self) -> bool {
        self.chosen.is_empty()
    }

    /// Serialize as `state_id input_id` lines, sorted by state id.
    pub fn write_to<W: Write>(&self, mut w: W) -> io::Result<()> {
        let mut entries: Vec<_> = self.chosen.iter().map(|(&x, &p)| (x, p)).collect();
        entries.sort_unstable();
        for (x, p) in entries {
            writeln!(w, "{} {}", x.0, self.pair_input[p.0 as usize].0)?;
        }
        Ok(())
    }

    /// Parse the format written by [`ControllerStore::write_to`], resolving each
    /// `(state, input)` to its PairId in `store`. Blank lines and `#` comments are skipped.
    pub fn read_from<R: BufRead>(r: R, store: &IndexedFrozen) -> io::Result<Self> {
        let invalid = |line: usize, msg: &str| {
            io::Error::new(io::ErrorKind::InvalidData, format!("line {line}: {msg}"))
        };
        let mut chosen = HashMap::new();
        for (i, line) in r.lines().enumerate() {
            let line = line?;
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut it = line.split_whitespace().map(str::parse::<u32>);
            let (Some(Ok(x)), Some(Ok(u)), None) = (it.next(), it.next(), it.next()) else {
                return Err(invalid(i + 1, "expected `state_id input_id`"));
            };
            if x as usize >= store.n_states() {
                return Err(invalid(i + 1, "state id out of range"));
            }
            let p = store
                .find_pair(StateId(x), InputId(u))
                .ok_or_else(|| invalid(i + 1, "input not admissible at state"))?;
            chosen.insert(StateId(x), p);
        }
        Ok(Self {
            chosen,
            pair_input: store.pair_inputs().to_vec(),
        })
    }
}
