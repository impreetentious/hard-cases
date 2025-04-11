//! The verifier's JSON instance reader.
//!
//! A second reader for the same schema, written separately from the solver's.
//! That one builds a value tree and walks it; this one lexes the document into
//! a flat sequence of events and interprets the sequence with an index. There
//! is no recursion anywhere in it, which is why the nesting limit here is a
//! counter rather than a bound on a call stack: a document cannot end this
//! reader by being deep, only by being long, and length is capped before the
//! first byte is examined.
//!
//! The rule the schema rests on is enforced by the lexer rather than by a
//! check somewhere later: there is no floating-point literal in the accepted
//! grammar at all. Every model value is a decimal string, read by the same
//! exact grammar the other two formats use, and a number in one of those
//! positions is refused as a number rather than converted.

use crate::vmodel::{Cell, Column, Constraint, Direction, Instance, Tag};
use crate::vparse::{Names, as_text, check_name, number, representable};
use crate::vrat::Frac;
use crate::vreject::{Code, Rejected};

/// The schema version this reader accepts.
const VERSION: i64 = 1;
/// The deepest nesting the accepted grammar reaches, with room to spare.
const MAX_DEPTH: usize = 8;
/// The largest accepted number of stored coefficients.
const MAX_CELLS: usize = 10_000_000;

/// The members of the document and of each kind of element.
const DOCUMENT: [&str; 6] = [
    "version",
    "sense",
    "objective_constant",
    "vars",
    "rows",
    "nonzeros",
];
const VAR: [&str; 5] = ["name", "lb", "ub", "integer", "obj"];
const ROW: [&str; 4] = ["name", "sense", "rhs", "range"];

/// One event of the flattened document.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    /// An object opens.
    Object,
    /// An array opens.
    Array,
    /// The container that opened most recently closes.
    Shut,
    /// A member name. Always immediately followed by its value.
    Key(String),
    /// A string value.
    Word(String),
    /// An integer value. Never a model value.
    Count(i64),
    /// A boolean value.
    Flag(bool),
    /// The absence a nullable member spells.
    Void,
}

fn refuse(detail: impl Into<String>) -> Rejected {
    Rejected::new(Code::JSON_SCHEMA, detail)
}

/// Reads a JSON instance document.
///
/// # Errors
///
/// Returns a refusal naming the rule that refused it.
pub fn read(raw: &[u8]) -> Result<Instance, Rejected> {
    if raw.len() > crate::vparse::MAX_FILE {
        return Err(Rejected::new(
            Code::FILE_TOO_LARGE,
            format!("file is {} bytes, over the limit", raw.len()),
        ));
    }
    let text = as_text(raw)?;
    let digest = crate::vhash::instance_digest(Tag::Json.byte(), raw);
    let events = flatten(text)?;
    build(&events, digest)
}

// ---------------------------------------------------------------------------
// The lexer
// ---------------------------------------------------------------------------

/// Turns the document into its events, checking the grammar as it goes.
fn flatten(text: &str) -> Result<Vec<Event>, Rejected> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    // One frame per open container: whether it is an object, whether a value
    // is expected next, and the member names it has already carried.
    let mut stack: Vec<(bool, Vec<String>)> = Vec::new();
    let mut at = 0usize;
    let mut expecting = true;

    loop {
        at = past_blanks(bytes, at);
        if stack.is_empty() && !out.is_empty() {
            break;
        }
        let Some(byte) = bytes.get(at).copied() else {
            return Err(refuse("the document ends where a value was expected"));
        };

        if !expecting {
            // Between values: a separator, or the end of a container.
            match byte {
                b',' => {
                    at += 1;
                    expecting = true;
                    if stack.last().is_some_and(|frame| frame.0) {
                        let (name, next) = member(bytes, at)?;
                        at = next;
                        let frame = stack.last_mut().expect("a frame is open");
                        if frame.1.contains(&name) {
                            return Err(refuse(format!(
                                "the member {name:?} appears more than once in one object"
                            )));
                        }
                        frame.1.push(name.clone());
                        out.push(Event::Key(name));
                    }
                    continue;
                }
                b'}' | b']' => {
                    let Some((object, _)) = stack.pop() else {
                        return Err(refuse(format!("a stray closing brace at {at}")));
                    };
                    if object != (byte == b'}') {
                        return Err(refuse(format!("mismatched brackets at {at}")));
                    }
                    at += 1;
                    out.push(Event::Shut);
                    continue;
                }
                _ => return Err(refuse(format!("a separator was expected at {at}"))),
            }
        }

        match byte {
            b'{' | b'[' => {
                if stack.len() >= MAX_DEPTH {
                    return Err(refuse(format!(
                        "the document nests deeper than {MAX_DEPTH} levels"
                    )));
                }
                let object = byte == b'{';
                at += 1;
                out.push(if object { Event::Object } else { Event::Array });
                stack.push((object, Vec::new()));
                at = past_blanks(bytes, at);
                // An empty container closes here; anything else is a first
                // element, and in an object a first element is a member name.
                if bytes.get(at) == Some(if object { &b'}' } else { &b']' }) {
                    at += 1;
                    out.push(Event::Shut);
                    stack.pop();
                    expecting = false;
                    continue;
                }
                if object {
                    let (name, next) = member(bytes, at)?;
                    at = next;
                    let frame = stack.last_mut().expect("a frame is open");
                    frame.1.push(name.clone());
                    out.push(Event::Key(name));
                }
                continue;
            }
            b'"' => {
                let (word, next) = string(bytes, at)?;
                at = next;
                out.push(Event::Word(word));
            }
            b't' if bytes[at..].starts_with(b"true") => {
                at += 4;
                out.push(Event::Flag(true));
            }
            b'f' if bytes[at..].starts_with(b"false") => {
                at += 5;
                out.push(Event::Flag(false));
            }
            b'n' if bytes[at..].starts_with(b"null") => {
                at += 4;
                out.push(Event::Void);
            }
            b'-' | b'0'..=b'9' => {
                let (value, next) = whole(bytes, at)?;
                at = next;
                out.push(Event::Count(value));
            }
            other => {
                return Err(refuse(format!(
                    "byte {:?} at {at} begins no value",
                    other as char
                )));
            }
        }
        expecting = false;
        if stack.is_empty() {
            break;
        }
    }

    if !stack.is_empty() {
        return Err(refuse("the document ends inside a container"));
    }
    if past_blanks(bytes, at) != bytes.len() {
        return Err(refuse("content follows the document"));
    }
    Ok(out)
}

fn past_blanks(bytes: &[u8], from: usize) -> usize {
    let mut at = from;
    while matches!(bytes.get(at), Some(b' ' | b'\t' | b'\n' | b'\r')) {
        at += 1;
    }
    at
}

/// Reads a member name and the colon after it.
fn member(bytes: &[u8], from: usize) -> Result<(String, usize), Rejected> {
    let at = past_blanks(bytes, from);
    if bytes.get(at) != Some(&b'"') {
        return Err(refuse(format!("a member name was expected at {at}")));
    }
    let (name, next) = string(bytes, at)?;
    let after = past_blanks(bytes, next);
    if bytes.get(after) != Some(&b':') {
        return Err(refuse(format!("a colon was expected at {after}")));
    }
    Ok((name, after + 1))
}

/// Reads a string, resolving the accepted escapes.
fn string(bytes: &[u8], from: usize) -> Result<(String, usize), Rejected> {
    let mut at = from + 1;
    let mut out = String::new();
    loop {
        let Some(byte) = bytes.get(at).copied() else {
            return Err(refuse("the document ends inside a string"));
        };
        at += 1;
        match byte {
            b'"' => return Ok((out, at)),
            b'\\' => {
                let Some(escape) = bytes.get(at).copied() else {
                    return Err(refuse("the document ends inside an escape"));
                };
                at += 1;
                out.push(match escape {
                    b'"' => '"',
                    b'\\' => '\\',
                    b'/' => '/',
                    b'b' => '\u{8}',
                    b'f' => '\u{c}',
                    b'n' => '\n',
                    b'r' => '\r',
                    b't' => '\t',
                    // Every accepted input is ASCII, so an escape naming a
                    // code point either repeats a character that could have
                    // been written plainly or names one outside that set.
                    other => {
                        return Err(refuse(format!(
                            "the escape {:?} is outside the accepted set",
                            other as char
                        )));
                    }
                });
            }
            0x20..=0x7e => out.push(byte as char),
            other => {
                return Err(refuse(format!(
                    "byte {other:#04x} is not accepted inside a string"
                )));
            }
        }
    }
}

/// Reads an integer literal. A fractional part or an exponent is refused here,
/// which is what keeps every model value on the exact path.
fn whole(bytes: &[u8], from: usize) -> Result<(i64, usize), Rejected> {
    let mut at = from;
    if bytes.get(at) == Some(&b'-') {
        at += 1;
    }
    let first = at;
    while bytes.get(at).is_some_and(u8::is_ascii_digit) {
        at += 1;
    }
    if at == first {
        return Err(refuse(format!("a number with no digits at {from}")));
    }
    if bytes[first] == b'0' && at - first > 1 {
        return Err(refuse(format!("a leading zero at {first}")));
    }
    if matches!(bytes.get(at), Some(b'.' | b'e' | b'E')) {
        return Err(refuse(
            "a floating-point literal; every model value is a string",
        ));
    }
    let text =
        core::str::from_utf8(&bytes[from..at]).map_err(|_| refuse("a number is not text"))?;
    text.parse::<i64>()
        .map(|value| (value, at))
        .map_err(|_| refuse(format!("the integer {text} does not fit a machine word")))
}

// ---------------------------------------------------------------------------
// The interpreter
// ---------------------------------------------------------------------------

/// The events of one value, and where the value after it begins.
fn span(events: &[Event], from: usize) -> Result<(&[Event], usize), Rejected> {
    let mut at = from;
    let mut depth = 0usize;
    loop {
        match events.get(at) {
            Some(Event::Object | Event::Array) => depth += 1,
            Some(Event::Shut) => {
                depth -= 1;
                if depth == 0 {
                    return Ok((&events[from..=at], at + 1));
                }
            }
            Some(Event::Key(_)) => {}
            Some(_) => {
                if depth == 0 {
                    return Ok((&events[from..=at], at + 1));
                }
            }
            None => return Err(refuse("the document ends inside a value")),
        }
        at += 1;
    }
}

/// The members of an object, in the order they were written.
fn fields<'a>(
    events: &'a [Event],
    expected: &[&str],
    what: &str,
) -> Result<Vec<(String, &'a [Event])>, Rejected> {
    if events.first() != Some(&Event::Object) {
        return Err(refuse(format!("{what} is not an object")));
    }
    let mut out: Vec<(String, &[Event])> = Vec::new();
    let mut at = 1usize;
    while events.get(at) != Some(&Event::Shut) {
        let Some(Event::Key(name)) = events.get(at) else {
            return Err(refuse(format!(
                "{what} carries something that is not a member"
            )));
        };
        let (value, next) = span(events, at + 1)?;
        if !expected.contains(&name.as_str()) {
            return Err(refuse(format!(
                "{what} carries the unknown member {name:?}"
            )));
        }
        out.push((name.clone(), value));
        at = next;
    }
    for name in expected {
        if !out.iter().any(|(known, _)| known == name) {
            return Err(refuse(format!("{what} has no member {name:?}")));
        }
    }
    Ok(out)
}

/// One member of an object, by name.
fn field<'a>(
    members: &'a [(String, &'a [Event])],
    name: &str,
    what: &str,
) -> Result<&'a [Event], Rejected> {
    members
        .iter()
        .find(|(known, _)| known == name)
        .map(|(_, value)| *value)
        .ok_or_else(|| refuse(format!("{what} has no member {name:?}")))
}

/// A member that must be a string.
fn word<'a>(
    members: &'a [(String, &'a [Event])],
    name: &str,
    what: &str,
) -> Result<&'a str, Rejected> {
    match field(members, name, what)? {
        [Event::Word(found)] => Ok(found),
        _ => Err(refuse(format!("the {name} of {what} is not a string"))),
    }
}

/// A member that must be a string or the absence spelled null.
fn maybe<'a>(
    members: &'a [(String, &'a [Event])],
    name: &str,
    what: &str,
) -> Result<Option<&'a str>, Rejected> {
    match field(members, name, what)? {
        [Event::Word(found)] => Ok(Some(found)),
        [Event::Void] => Ok(None),
        _ => Err(refuse(format!(
            "the {name} of {what} is neither a string nor null"
        ))),
    }
}

/// The elements of an array member.
fn elements<'a>(
    members: &'a [(String, &'a [Event])],
    name: &str,
) -> Result<Vec<&'a [Event]>, Rejected> {
    let events = field(members, name, "the document")?;
    if events.first() != Some(&Event::Array) {
        return Err(refuse(format!(
            "the {name} of the document is not an array"
        )));
    }
    let mut out = Vec::new();
    let mut at = 1usize;
    while events.get(at) != Some(&Event::Shut) {
        let (value, next) = span(events, at)?;
        out.push(value);
        at = next;
    }
    Ok(out)
}

/// An exact value from a decimal string, held to every fence.
fn exact(text: &str, what: &str) -> Result<Frac, Rejected> {
    let value = number(text, 0)?;
    representable(&value, what, 0)?;
    Ok(value)
}

/// Turns an accepted document into the instance it describes.
fn build(events: &[Event], digest: [u8; 32]) -> Result<Instance, Rejected> {
    let document = fields(events, &DOCUMENT, "the document")?;

    match field(&document, "version", "the document")? {
        [Event::Count(VERSION)] => {}
        [Event::Count(other)] => {
            return Err(refuse(format!(
                "the document states version {other}, and this reader accepts {VERSION}"
            )));
        }
        _ => return Err(refuse("the version is not an integer")),
    }

    let written = match word(&document, "sense", "the document")? {
        "min" => Direction::Min,
        "max" => Direction::Max,
        other => {
            return Err(refuse(format!(
                "the sense {other:?} is neither min nor max"
            )));
        }
    };
    let flip = written == Direction::Max;

    let stated = exact(
        word(&document, "objective_constant", "the document")?,
        "the objective constant",
    )?;
    let constant = if flip { stated.negated() } else { stated };

    let mut columns = Vec::new();
    let mut column_names = Names::default();
    let declared = elements(&document, "vars")?;
    if declared.is_empty() {
        return Err(refuse(
            "the document declares no variables, so it states nothing to solve",
        ));
    }
    for (index, column) in declared.iter().enumerate() {
        let what = format!("variable {index}");
        let members = fields(column, &VAR, &what)?;
        let name = word(&members, "name", &what)?;
        check_name(name, 0)?;
        if !column_names.insert(name, index) {
            return Err(Rejected::new(
                Code::DUPLICATE_COLUMN,
                format!("two variables share the name {name:?}"),
            ));
        }
        let side = |member: &str| -> Result<Option<Frac>, Rejected> {
            match maybe(&members, member, &what)? {
                None => Ok(None),
                Some(text) => exact(text, "a bound").map(Some),
            }
        };
        let low = side("lb")?;
        let high = side("ub")?;
        let whole = match field(&members, "integer", &what)? {
            [Event::Flag(flag)] => *flag,
            _ => {
                return Err(refuse(format!(
                    "the integer member of {what} is not a boolean"
                )));
            }
        };
        let stated = exact(word(&members, "obj", &what)?, "an objective coefficient")?;
        columns.push(Column {
            low,
            high,
            whole,
            cost: if flip { stated.negated() } else { stated },
            name: name.to_owned(),
        });
    }

    let mut rows = Vec::new();
    let mut row_names = Names::default();
    for (index, row) in elements(&document, "rows")?.iter().enumerate() {
        let what = format!("row {index}");
        let members = fields(row, &ROW, &what)?;
        let name = word(&members, "name", &what)?;
        check_name(name, 0)?;
        if !row_names.insert(name, index) {
            return Err(Rejected::new(
                Code::DUPLICATE_ROW,
                format!("two rows share the name {name:?}"),
            ));
        }
        let sense = match word(&members, "sense", &what)? {
            "L" => Wide::Under,
            "G" => Wide::Over,
            "E" => Wide::Exact,
            other => {
                return Err(refuse(format!(
                    "the row sense {other:?} is outside the accepted set"
                )));
            }
        };
        let base = exact(word(&members, "rhs", &what)?, "a right-hand side")?;
        let range = match maybe(&members, "range", &what)? {
            None => None,
            Some(text) => Some(exact(text, "a range")?),
        };
        let (low, high) = widen(sense, &base, range.as_ref());
        rows.push(Constraint {
            low,
            high,
            name: name.to_owned(),
        });
    }

    let mut totals: Vec<(u32, u32, Frac, u32)> = Vec::new();
    for (index, entry) in elements(&document, "nonzeros")?.iter().enumerate() {
        let [
            Event::Array,
            Event::Count(row),
            Event::Count(column),
            Event::Word(text),
            Event::Shut,
        ] = entry
        else {
            return Err(refuse(format!(
                "coefficient {index} is not a row index, a column index, and a decimal string"
            )));
        };
        let row = inside(*row, rows.len(), "row", index)?;
        let column = inside(*column, columns.len(), "column", index)?;
        let value = exact(text, "a coefficient")?;
        let position = (column, row);
        match totals
            .iter_mut()
            .find(|entry| (entry.0, entry.1) == position)
        {
            Some(found) => {
                found.2 = found.2.plus(&value);
                found.3 += 1;
            }
            None => {
                if totals.len() >= MAX_CELLS {
                    return Err(Rejected::new(
                        Code::TOO_MANY_NONZEROS,
                        "too many stored coefficients".to_owned(),
                    ));
                }
                totals.push((position.0, position.1, value, 1));
            }
        }
    }

    totals.sort_by_key(|entry| (entry.0, entry.1));
    let mut cells = Vec::new();
    let mut cancelled = Vec::new();
    for (column, row, value, contributions) in totals {
        if value.is_zero() {
            // A cancellation and an absence are the same matrix and different
            // documents.
            if contributions > 1 {
                cancelled.push((column, row));
            }
            continue;
        }
        cells.push(Cell { column, row, value });
    }

    Ok(Instance {
        tag: Tag::Json,
        digest,
        written,
        constant,
        columns,
        rows,
        cells,
        cancelled,
        dropped: Vec::new(),
    })
}

/// How a row was written.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Wide {
    Under,
    Over,
    Exact,
}

/// The exact bounds a sense, a right-hand side, and a range produce.
///
/// The table is the one the MPS reader applies, to the character. Two formats
/// that widened a row differently would make a model depend on which of them a
/// caller happened to write it in.
fn widen(sense: Wide, base: &Frac, range: Option<&Frac>) -> (Option<Frac>, Option<Frac>) {
    let Some(range) = range else {
        return match sense {
            Wide::Under => (None, Some(base.clone())),
            Wide::Over => (Some(base.clone()), None),
            Wide::Exact => (Some(base.clone()), Some(base.clone())),
        };
    };
    let size = range.magnitude();
    match sense {
        Wide::Under => (Some(base.minus(&size)), Some(base.clone())),
        Wide::Over => (Some(base.clone()), Some(base.plus(&size))),
        Wide::Exact => {
            if range.is_minus() {
                (Some(base.plus(range)), Some(base.clone()))
            } else {
                // A zero range on an equality row is a no-op rather than an
                // error. It says nothing and is permitted to say nothing.
                (Some(base.clone()), Some(base.plus(range)))
            }
        }
    }
}

/// Checks one index against the array it points into.
fn inside(value: i64, count: usize, what: &str, at: usize) -> Result<u32, Rejected> {
    u32::try_from(value)
        .ok()
        .filter(|found| (*found as usize) < count)
        .ok_or_else(|| {
            refuse(format!(
                "coefficient {at} names {what} {value}, and there are {count}"
            ))
        })
}
