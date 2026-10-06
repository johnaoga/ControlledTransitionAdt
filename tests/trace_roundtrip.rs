//! Parse → serialize → re-parse identity tests for the Appendix C trace format.

use std::io::Cursor;
use phase_specialized_cts::builder::{parse_trace, write_trace};
use phase_specialized_cts::semantic::types::{InputId, StateId, TraceHeader};

fn minimal_header() -> TraceHeader {
    TraceHeader {
        n_states: 3,
        n_inputs: 2,
        state_names: vec![Some("A".into()), Some("B".into()), Some("C".into())],
        input_names: vec![Some("L".into()), Some("R".into())],
        checksum: None,
    }
}

fn minimal_triples() -> Vec<(StateId, InputId, StateId)> {
    vec![
        (StateId(0), InputId(0), StateId(1)),
        (StateId(1), InputId(1), StateId(2)),
        (StateId(2), InputId(0), StateId(0)),
    ]
}

#[test]
fn roundtrip_preserves_triples() {
    let header = minimal_header();
    let triples = minimal_triples();

    // Serialize.
    let mut buf = Vec::new();
    write_trace(&mut buf, &header, triples.iter().cloned()).unwrap();

    // Re-parse.
    let (parsed_header, parsed_triples) = parse_trace(Cursor::new(&buf)).unwrap();
    let parsed: Vec<_> = parsed_triples.map(|t| t.unwrap()).collect();

    assert_eq!(parsed_header.n_states, header.n_states);
    assert_eq!(parsed_header.n_inputs, header.n_inputs);

    // Triple set must match (order may differ after roundtrip).
    let expected: std::collections::HashSet<_> = triples.into_iter().collect();
    let actual: std::collections::HashSet<_> = parsed.into_iter().collect();
    assert_eq!(actual, expected);
}

#[test]
fn roundtrip_preserves_state_names() {
    let header = minimal_header();
    let mut buf = Vec::new();
    write_trace(&mut buf, &header, std::iter::empty()).unwrap();
    let (parsed_header, _) = parse_trace(Cursor::new(&buf)).unwrap();
    assert_eq!(parsed_header.state_names, header.state_names);
    assert_eq!(parsed_header.input_names, header.input_names);
}

#[test]
fn parse_ignores_comment_lines() {
    let trace = b"# this is a comment\nstates 2\ninputs 1\n0 0 1\n1 0 0\n";
    let (header, triples) = parse_trace(Cursor::new(trace.as_slice())).unwrap();
    assert_eq!(header.n_states, 2);
    let ts: Vec<_> = triples.map(|t| t.unwrap()).collect();
    assert_eq!(ts.len(), 2);
}

#[test]
fn parse_allows_duplicate_triples() {
    // Duplicates are passed through; deduplication is the builder's job.
    let trace = b"states 2\ninputs 1\n0 0 1\n0 0 1\n";
    let (_, triples) = parse_trace(Cursor::new(trace.as_slice())).unwrap();
    let ts: Vec<_> = triples.map(|t| t.unwrap()).collect();
    assert_eq!(ts.len(), 2, "parser should pass duplicates through");
}
