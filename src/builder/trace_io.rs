//! Benchmark trace format parser and serializer (Appendix C format).
//!
//! See specs.md § F3 — Benchmark Trace Format
//!
//! # Format
//! ```text
//! # checksum: <sha256 hex>        (optional; see `triple_checksum`)
//! # any other comment
//! states N
//! inputs A
//! state 0 Name of state zero
//! input 0 L
//! X U Y
//! ...
//! ```
//! - `states` and `inputs` must both appear before any `state`/`input`/triple line.
//! - A name is the remainder of the line after its id, trimmed, so names may contain
//!   internal spaces without quoting. Names must be non-empty and contain no newline.
//! - Blank lines and lines starting with `#` are ignored everywhere. Trailing
//!   whitespace is ignored.
//! - The header ends at the first line that is not a comment or header record. Every
//!   later line must be a triple `X U Y` of decimal ids within the declared bounds.

use std::io::{BufRead, Write};

use sha2::{Digest, Sha256};

use crate::semantic::types::{InputId, StateId, TraceHeader};

/// Error type for trace I/O.
#[derive(Debug)]
pub enum TraceError {
    Io(std::io::Error),
    Parse { line: usize, msg: String },
}

impl From<std::io::Error> for TraceError {
    fn from(e: std::io::Error) -> Self {
        TraceError::Io(e)
    }
}

impl std::fmt::Display for TraceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TraceError::Io(e) => write!(f, "I/O error: {}", e),
            TraceError::Parse { line, msg } => write!(f, "parse error at line {}: {}", line, msg),
        }
    }
}

impl std::error::Error for TraceError {}

/// A parsed `(x, u, y)` triple or the error for its line.
pub type TripleResult = Result<(StateId, InputId, StateId), TraceError>;

const CHECKSUM_PREFIX: &str = "checksum:";

fn parse_err(line: usize, msg: impl Into<String>) -> TraceError {
    TraceError::Parse {
        line,
        msg: msg.into(),
    }
}

/// Canonical SHA-256 checksum of a triple stream, as lowercase hex.
///
/// The relation is canonicalized by sorting the triples by `(x, u, y)` and removing
/// duplicates. Each triple is then hashed as 12 bytes: `x`, `u`, `y` as big-endian
/// `u32`. The result depends only on the relation, not on insertion order, duplicates
/// or machine endianness.
pub fn triple_checksum<I>(triples: I) -> String
where
    I: IntoIterator<Item = (StateId, InputId, StateId)>,
{
    let mut v: Vec<_> = triples.into_iter().collect();
    v.sort_unstable();
    v.dedup();
    checksum_sorted_unique(&v)
}

fn checksum_sorted_unique(sorted: &[(StateId, InputId, StateId)]) -> String {
    let mut h = Sha256::new();
    for &(x, u, y) in sorted {
        h.update(x.0.to_be_bytes());
        h.update(u.0.to_be_bytes());
        h.update(y.0.to_be_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Parse an Appendix C format trace from a buffered reader.
///
/// Returns the header and a lazy iterator of `(StateId, InputId, StateId)` triples.
/// Duplicate triples are passed through; deduplication is the builder's responsibility.
/// Name vectors in the header always have length `n_states` / `n_inputs`, with `None`
/// for undeclared names. `header.checksum` is the value of a `# checksum:` comment, if
/// any. The parser does not verify it, because triples are read lazily. Compare it
/// against [`triple_checksum`] after consuming the stream.
pub fn parse_trace<'a, R: BufRead + 'a>(
    reader: R,
) -> Result<(TraceHeader, Box<dyn Iterator<Item = TripleResult> + 'a>), TraceError> {
    let mut lines = reader.lines().enumerate().map(|(i, l)| (i + 1, l));
    let mut n_states: Option<usize> = None;
    let mut n_inputs: Option<usize> = None;
    let mut state_names: Vec<Option<String>> = Vec::new();
    let mut input_names: Vec<Option<String>> = Vec::new();
    let mut checksum = None;
    let mut first_triple: Option<(usize, String)> = None;
    let mut last_line = 0;

    for (no, line) in lines.by_ref() {
        let line = line?;
        last_line = no;
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(comment) = t.strip_prefix('#') {
            if let Some(sum) = comment.trim_start().strip_prefix(CHECKSUM_PREFIX) {
                checksum = Some(sum.trim().to_string());
            }
            continue;
        }
        let (kw, rest) = t.split_once(char::is_whitespace).unwrap_or((t, ""));
        let rest = rest.trim_start();
        match kw {
            "states" | "inputs" => {
                let count: usize = rest
                    .parse()
                    .map_err(|_| parse_err(no, format!("`{kw}` expects a count")))?;
                let (slot, names) = if kw == "states" {
                    (&mut n_states, &mut state_names)
                } else {
                    (&mut n_inputs, &mut input_names)
                };
                if slot.replace(count).is_some() {
                    return Err(parse_err(no, format!("duplicate `{kw}` record")));
                }
                *names = vec![None; count];
            }
            "state" | "input" => {
                let (bound, names) = if kw == "state" {
                    (n_states, &mut state_names)
                } else {
                    (n_inputs, &mut input_names)
                };
                let bound =
                    bound.ok_or_else(|| parse_err(no, format!("`{kw}` before `{kw}s` count")))?;
                let (id, name) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
                let id: usize = id
                    .parse()
                    .map_err(|_| parse_err(no, format!("`{kw}` expects an id")))?;
                let name = name.trim();
                if id >= bound {
                    return Err(parse_err(no, format!("{kw} id {id} out of range")));
                }
                if name.is_empty() {
                    return Err(parse_err(no, format!("{kw} {id} has an empty name")));
                }
                if names[id].replace(name.to_string()).is_some() {
                    return Err(parse_err(no, format!("{kw} {id} named twice")));
                }
            }
            _ => {
                first_triple = Some((no, line));
                break;
            }
        }
    }

    let (n_states, n_inputs) = match (n_states, n_inputs) {
        (Some(n), Some(a)) => (n, a),
        _ => {
            let at = first_triple.as_ref().map_or(last_line, |(no, _)| *no);
            return Err(parse_err(at, "missing `states` or `inputs` header"));
        }
    };

    let header = TraceHeader {
        n_states,
        n_inputs,
        state_names,
        input_names,
        checksum,
    };
    let rest = first_triple
        .into_iter()
        .map(Ok::<_, std::io::Error>)
        .chain(lines.map(|(no, l)| l.map(|l| (no, l))));
    let triples = rest.filter_map(move |item| match item {
        Err(e) => Some(Err(TraceError::Io(e))),
        Ok((no, line)) => parse_triple_line(no, &line, n_states, n_inputs),
    });
    Ok((header, Box::new(triples)))
}

/// Parse one body line: `None` for blank/comment lines, else a triple or an error.
fn parse_triple_line(
    no: usize,
    line: &str,
    n_states: usize,
    n_inputs: usize,
) -> Option<TripleResult> {
    let t = line.trim();
    if t.is_empty() || t.starts_with('#') {
        return None;
    }
    let mut it = t.split_whitespace().map(str::parse::<u32>);
    let (Some(Ok(x)), Some(Ok(u)), Some(Ok(y)), None) =
        (it.next(), it.next(), it.next(), it.next())
    else {
        return Some(Err(parse_err(no, format!("expected `X U Y`, got `{t}`"))));
    };
    if x as usize >= n_states || y as usize >= n_states {
        return Some(Err(parse_err(no, "state id out of range")));
    }
    if u as usize >= n_inputs {
        return Some(Err(parse_err(no, "input id out of range")));
    }
    Some(Ok((StateId(x), InputId(u), StateId(y))))
}

/// Serialize a trace in Appendix C format.
///
/// Writes:
/// - A `# checksum: <sha256>` comment computed from `triples` (see [`triple_checksum`]).
///   Any `header.checksum` is ignored, so the written checksum always matches the body.
/// - `states N` / `inputs A` header.
/// - `state ID NAME` / `input ID NAME` lines for every `Some` name.
/// - `X U Y` triple lines, in the given order (duplicates preserved).
///
/// Names must be non-empty, have no leading/trailing whitespace and no line breaks.
/// Otherwise they would not round-trip, and an `InvalidInput` I/O error is returned.
pub fn write_trace<W: Write, I>(
    writer: W,
    header: &TraceHeader,
    triples: I,
) -> Result<(), TraceError>
where
    I: IntoIterator<Item = (StateId, InputId, StateId)>,
{
    let triples: Vec<_> = triples.into_iter().collect();
    let mut canonical = triples.clone();
    canonical.sort_unstable();
    canonical.dedup();
    let sum = checksum_sorted_unique(&canonical);

    let mut w = std::io::BufWriter::new(writer);
    writeln!(w, "# {CHECKSUM_PREFIX} {sum}")?;
    writeln!(w, "states {}", header.n_states)?;
    writeln!(w, "inputs {}", header.n_inputs)?;
    for (kw, names) in [
        ("state", &header.state_names),
        ("input", &header.input_names),
    ] {
        for (id, name) in names.iter().enumerate() {
            if let Some(name) = name {
                if name.is_empty() || name.trim() != name || name.contains(['\n', '\r']) {
                    return Err(TraceError::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("{kw} {id}: name {name:?} would not round-trip"),
                    )));
                }
                writeln!(w, "{kw} {id} {name}")?;
            }
        }
    }
    for (x, u, y) in triples {
        writeln!(w, "{} {} {}", x.0, u.0, y.0)?;
    }
    w.flush()?;
    Ok(())
}
