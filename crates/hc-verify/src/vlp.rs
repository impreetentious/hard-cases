//! The verifier's LP-format reader.
//!
//! A second reader for the same subset, written separately from the solver's.
//! The construction is deliberately unlike its counterpart: that one carries a
//! pending statement and extends a vector of tokens as lines arrive, and this
//! one walks the file twice — once to cut it into logical lines with the
//! section each belongs to, and once to interpret them — and reads each
//! statement with a predictive cursor that never materialises a token list.
//!
//! Neither construction is better. Different is the property being bought:
//! two implementations that make the same choices tend to make the same
//! mistakes, and the point of the second one is that its mistakes are
//! somewhere else.
//!
//! There is no lenient mode here, as there is none anywhere in this crate. A
//! file the solver read leniently must be independently accepted by this
//! reader, strictly, before anything about it can carry a claim.

use crate::vmodel::{Cell, Column, Constraint, Direction, Instance, Tag};
use crate::vparse::{Names, as_text, check_name, number, representable};
use crate::vrat::Frac;
use crate::vreject::{Code, Rejected};

/// The sections, in the order they must appear.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Objective,
    Rows,
    Bounds,
    Whole,
    Zeroone,
    Stop,
}

impl Part {
    const fn rank(self) -> usize {
        match self {
            Self::Objective => 0,
            Self::Rows => 1,
            Self::Bounds => 2,
            Self::Whole => 3,
            Self::Zeroone => 4,
            Self::Stop => 5,
        }
    }

    const fn word(self) -> &'static str {
        match self {
            Self::Objective => "the objective section",
            Self::Rows => "the constraint section",
            Self::Bounds => "the bound section",
            Self::Whole => "the general integrality section",
            Self::Zeroone => "the binary integrality section",
            Self::Stop => "the terminator",
        }
    }
}

/// What a keyword line opens, with the direction where it carries one.
fn keyword(shape: &str) -> Option<(Part, Option<Direction>)> {
    match shape {
        "MINIMIZE" | "MINIMISE" | "MINIMUM" | "MIN" => {
            Some((Part::Objective, Some(Direction::Min)))
        }
        "MAXIMIZE" | "MAXIMISE" | "MAXIMUM" | "MAX" => {
            Some((Part::Objective, Some(Direction::Max)))
        }
        "SUBJECT TO" | "SUCH THAT" | "ST" | "ST." | "S.T." => Some((Part::Rows, None)),
        "BOUNDS" | "BOUND" => Some((Part::Bounds, None)),
        "GENERAL" | "GENERALS" | "GEN" => Some((Part::Whole, None)),
        "BINARY" | "BINARIES" | "BIN" => Some((Part::Zeroone, None)),
        "END" => Some((Part::Stop, None)),
        _ => None,
    }
}

/// Sections this subset knows about and does not accept.
fn unsupported(shape: &str) -> bool {
    matches!(
        shape,
        "SEMI-CONTINUOUS"
            | "SEMIS"
            | "SEMI"
            | "SOS"
            | "SOS1"
            | "SOS2"
            | "INDICATORS"
            | "LAZY CONSTRAINTS"
            | "USER CUTS"
            | "PWL"
            | "QMATRIX"
            | "QUADRATIC OBJECTIVE"
    )
}

/// Whether a word may never be a label.
fn reserved(word: &str) -> bool {
    let shape = word.to_ascii_uppercase();
    keyword(&shape).is_some()
        || matches!(
            shape.as_str(),
            "SUBJECT" | "SUCH" | "THAT" | "TO" | "FREE" | "INF" | "INFINITY"
        )
}

/// One logical line: the section it belongs to, where it started, its text.
struct Logical {
    part: Part,
    at: usize,
    text: String,
}

/// Reads an LP-format file.
///
/// # Errors
///
/// Returns a refusal naming the rule that refused the file.
pub fn read(raw: &[u8]) -> Result<Instance, Rejected> {
    if raw.len() > crate::vparse::MAX_FILE {
        return Err(Rejected::new(
            Code::FILE_TOO_LARGE,
            format!("file is {} bytes, over the limit", raw.len()),
        ));
    }
    let text = as_text(raw)?;
    let digest = crate::vhash::instance_digest(Tag::Lp.byte(), raw);
    // The cut reports a structural defect by handing it back rather than by
    // returning: every statement it gathered began on an earlier line than the
    // defect did, so the statements are interpreted first and the structural
    // refusal is reported only when they are all sound. A refusal names the
    // earliest line carrying a defect, never the first check that ran.
    let (lines, written, structural) = cut(text);
    let instance = assemble(&lines, written, digest)?;
    match structural {
        Some(refusal) => Err(refusal),
        None => Ok(instance),
    }
}

/// The first walk: comments away, keywords recognised, statements gathered.
///
/// A structural defect stops the walk and is returned beside what was gathered
/// before it, because a statement that began earlier is refused earlier.
fn cut(text: &str) -> (Vec<Logical>, Direction, Option<Rejected>) {
    let mut out: Vec<Logical> = Vec::new();
    let mut part: Option<Part> = None;
    let mut seen = [false; 6];
    let mut rank = 0usize;
    let mut stopped = false;
    let mut written = Direction::Min;
    let mut carrying = false;

    for (offset, whole) in text.lines().enumerate() {
        let at = offset + 1;
        let body = match whole.find('\\') {
            Some(cut) => whole[..cut].trim(),
            None => whole.trim(),
        };
        if body.is_empty() {
            continue;
        }

        let shape = collapse(body);
        if unsupported(&shape) {
            return (
                out,
                written,
                Some(
                    Rejected::new(
                        Code::UNKNOWN_SECTION,
                        format!("{body:?} is a section this subset does not accept"),
                    )
                    .on_line(at),
                ),
            );
        }
        if let Some(found) = opens(&shape) {
            let (opened, direction) = match found {
                Ok(pair) => pair,
                Err(refusal) => return (out, written, Some(refusal.on_line(at))),
            };
            if seen[opened.rank()] {
                let refusal = Rejected::new(
                    Code::SECTION_ORDER,
                    format!("{} appears twice", opened.word()),
                )
                .on_line(at);
                return (out, written, Some(refusal));
            }
            if opened.rank() < rank {
                let refusal = Rejected::new(
                    Code::SECTION_ORDER,
                    format!("{} appears out of order", opened.word()),
                )
                .on_line(at);
                return (out, written, Some(refusal));
            }
            seen[opened.rank()] = true;
            rank = opened.rank();
            if let Some(direction) = direction {
                written = direction;
            }
            part = Some(opened);
            carrying = false;
            if opened == Part::Stop {
                stopped = true;
                break;
            }
            continue;
        }

        let Some(inside) = part else {
            let refusal = Rejected::new(
                Code::LP_GRAMMAR,
                "content appears before any section".to_owned(),
            )
            .on_line(at);
            return (out, written, Some(refusal));
        };

        match inside {
            Part::Objective | Part::Rows => {
                if labelled(body) {
                    out.push(Logical {
                        part: inside,
                        at,
                        text: body.to_owned(),
                    });
                    carrying = true;
                } else {
                    if !carrying {
                        let refusal = Rejected::new(
                            Code::LP_GRAMMAR,
                            "a statement begins without a label".to_owned(),
                        )
                        .on_line(at);
                        return (out, written, Some(refusal));
                    }
                    let last = out.last_mut().expect("carrying implies a statement");
                    last.text.push(' ');
                    last.text.push_str(body);
                }
            }
            _ => out.push(Logical {
                part: inside,
                at,
                text: body.to_owned(),
            }),
        }
    }

    if !stopped {
        let refusal = Rejected::new(
            Code::UNEXPECTED_END,
            "input ends before the terminator".to_owned(),
        );
        return (out, written, Some(refusal));
    }
    for (rank, part) in [(0, Part::Objective), (1, Part::Rows)] {
        if !seen[rank] {
            let refusal =
                Rejected::new(Code::SECTION_MISSING, format!("{} is missing", part.word()));
            return (out, written, Some(refusal));
        }
    }
    (out, written, None)
}

/// Whether a shape opens a section, and whether it does so cleanly.
fn opens(shape: &str) -> Option<Result<(Part, Option<Direction>), Rejected>> {
    if let Some(found) = keyword(shape) {
        return Some(Ok(found));
    }
    // A keyword sharing its line with content would put the reader back in the
    // business of guessing where statements begin.
    let head = shape.split(' ').next().unwrap_or("");
    let pair = shape.splitn(3, ' ').take(2).collect::<Vec<_>>().join(" ");
    for candidate in [head, pair.as_str()] {
        if candidate.len() < shape.len() {
            if let Some((part, _)) = keyword(candidate) {
                return Some(Err(Rejected::new(
                    Code::LP_GRAMMAR,
                    format!("{} shares its line with other content", part.word()),
                )));
            }
        }
    }
    None
}

/// Upper-cases a line and collapses its runs of blanks.
fn collapse(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    for word in line.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&word.to_ascii_uppercase());
    }
    out
}

/// Whether a line opens a statement rather than continuing one.
fn labelled(body: &str) -> bool {
    let mut cursor = Cursor::over(body);
    cursor.word().is_some() && cursor.byte(b':')
}

/// A predictive cursor over one logical line.
struct Cursor<'a> {
    text: &'a str,
    at: usize,
}

/// A relational operator.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Rel {
    Le,
    Ge,
    Eq,
}

impl<'a> Cursor<'a> {
    fn over(text: &'a str) -> Self {
        Self { text, at: 0 }
    }

    fn blanks(&mut self) {
        while let Some(byte) = self.text.as_bytes().get(self.at) {
            if *byte == b' ' || *byte == b'\t' {
                self.at += 1;
            } else {
                break;
            }
        }
    }

    fn done(&mut self) -> bool {
        self.blanks();
        self.at >= self.text.len()
    }

    /// Consumes one byte when it is the one asked for.
    fn byte(&mut self, want: u8) -> bool {
        self.blanks();
        if self.text.as_bytes().get(self.at) == Some(&want) {
            self.at += 1;
            return true;
        }
        false
    }

    /// Consumes a name.
    fn word(&mut self) -> Option<&'a str> {
        self.blanks();
        let bytes = self.text.as_bytes();
        let first = *bytes.get(self.at)?;
        if !(first.is_ascii_alphabetic() || first == b'_') {
            return None;
        }
        let start = self.at;
        while let Some(byte) = bytes.get(self.at) {
            if byte.is_ascii_alphanumeric() || *byte == b'_' || *byte == b'.' {
                self.at += 1;
            } else {
                break;
            }
        }
        Some(&self.text[start..self.at])
    }

    /// Consumes a numeric token, exponent and all.
    fn digits(&mut self) -> Option<&'a str> {
        self.blanks();
        let bytes = self.text.as_bytes();
        let first = *bytes.get(self.at)?;
        if !(first.is_ascii_digit() || first == b'.') {
            return None;
        }
        let start = self.at;
        while let Some(byte) = bytes.get(self.at) {
            if byte.is_ascii_digit() || *byte == b'.' {
                self.at += 1;
            } else {
                break;
            }
        }
        // The exponent is taken only when digits follow it, so a name
        // beginning with the letter after a bare number stays a name.
        if matches!(bytes.get(self.at), Some(b'e' | b'E')) {
            let mut ahead = self.at + 1;
            if matches!(bytes.get(ahead), Some(b'+' | b'-')) {
                ahead += 1;
            }
            if bytes.get(ahead).is_some_and(u8::is_ascii_digit) {
                self.at = ahead;
                while bytes.get(self.at).is_some_and(u8::is_ascii_digit) {
                    self.at += 1;
                }
            }
        }
        Some(&self.text[start..self.at])
    }

    /// Consumes a sign, reporting whether it was a plus.
    fn sign(&mut self) -> Option<bool> {
        if self.byte(b'+') {
            return Some(true);
        }
        if self.byte(b'-') {
            return Some(false);
        }
        None
    }

    /// Consumes a relational operator.
    fn relation(&mut self) -> Option<Rel> {
        self.blanks();
        let bytes = self.text.as_bytes();
        match bytes.get(self.at) {
            Some(b'<') => {
                self.at += 1;
                if bytes.get(self.at) == Some(&b'=') {
                    self.at += 1;
                }
                Some(Rel::Le)
            }
            Some(b'>') => {
                self.at += 1;
                if bytes.get(self.at) == Some(&b'=') {
                    self.at += 1;
                }
                Some(Rel::Ge)
            }
            Some(b'=') => {
                self.at += 1;
                Some(Rel::Eq)
            }
            _ => None,
        }
    }
}

/// A column being built.
struct Slot {
    name: String,
    whole: bool,
    cost: Frac,
    low: Option<Frac>,
    high: Option<Frac>,
    low_default: bool,
    fixed_low: bool,
    fixed_high: bool,
    listed: bool,
}

/// The second walk: logical lines into the instance they describe.
fn assemble(lines: &[Logical], written: Direction, digest: [u8; 32]) -> Result<Instance, Rejected> {
    let mut columns: Vec<Slot> = Vec::new();
    let mut column_names = Names::default();
    let mut rows: Vec<Constraint> = Vec::new();
    let mut row_names = Names::default();
    let mut cells: Vec<(u32, u32, Frac, u32)> = Vec::new();
    let mut constant = Frac::zero();

    for logical in lines {
        let at = logical.at;
        let mut cursor = Cursor::over(&logical.text);
        match logical.part {
            Part::Objective => {
                label(&mut cursor, at)?;
                let form = linear(&mut cursor, at, &mut columns, &mut column_names, true)?;
                if !cursor.done() {
                    return Err(grammar(
                        "the objective statement does not end where it should",
                        at,
                    ));
                }
                for (column, value) in form.terms {
                    columns[column].cost = columns[column].cost.plus(&value);
                }
                constant = constant.plus(&form.constant);
            }
            Part::Rows => {
                let name = label(&mut cursor, at)?;
                let (low, high, terms) = row(&mut cursor, at, &mut columns, &mut column_names)?;
                let index = rows.len();
                if !row_names.insert(name, index) {
                    return Err(Rejected::new(
                        Code::DUPLICATE_ROW,
                        format!("two constraints share the label {name:?}"),
                    )
                    .on_line(at));
                }
                rows.push(Constraint {
                    low,
                    high,
                    name: name.to_owned(),
                });
                for (column, value) in terms {
                    let position = (column as u32, index as u32);
                    match cells
                        .iter_mut()
                        .find(|entry| (entry.0, entry.1) == position)
                    {
                        Some(entry) => {
                            entry.2 = entry.2.plus(&value);
                            entry.3 += 1;
                        }
                        None => {
                            if cells.len() >= 10_000_000 {
                                return Err(Rejected::new(
                                    Code::TOO_MANY_NONZEROS,
                                    "too many stored coefficients".to_owned(),
                                )
                                .on_line(at));
                            }
                            cells.push((position.0, position.1, value, 1));
                        }
                    }
                }
            }
            Part::Bounds => bound(&mut cursor, at, &mut columns, &column_names)?,
            Part::Whole | Part::Zeroone => whole(
                &mut cursor,
                at,
                &mut columns,
                &column_names,
                logical.part == Part::Zeroone,
            )?,
            Part::Stop => unreachable!("the terminator ends the first walk"),
        }
    }

    let flip = written == Direction::Max;
    let built = columns
        .iter()
        .map(|slot| Column {
            low: slot.low.clone(),
            high: slot.high.clone(),
            whole: slot.whole,
            cost: if flip {
                slot.cost.negated()
            } else {
                slot.cost.clone()
            },
            name: slot.name.clone(),
        })
        .collect();

    cells.sort_by_key(|entry| (entry.0, entry.1));
    let mut kept = Vec::new();
    let mut cancelled = Vec::new();
    for (column, row, value, contributions) in cells {
        if value.is_zero() {
            if contributions > 1 {
                cancelled.push((column, row));
            }
            continue;
        }
        kept.push(Cell { column, row, value });
    }

    Ok(Instance {
        tag: Tag::Lp,
        digest,
        written,
        constant: if flip { constant.negated() } else { constant },
        columns: built,
        rows,
        cells: kept,
        cancelled,
        dropped: Vec::new(),
    })
}

fn grammar(detail: &str, at: usize) -> Rejected {
    Rejected::new(Code::LP_GRAMMAR, detail.to_owned()).on_line(at)
}

/// Reads the label that opens a statement.
fn label<'a>(cursor: &mut Cursor<'a>, at: usize) -> Result<&'a str, Rejected> {
    let Some(name) = cursor.word() else {
        return Err(grammar("a statement begins without a label", at));
    };
    if !cursor.byte(b':') {
        return Err(grammar("a label is closed by a colon", at));
    }
    if reserved(name) {
        return Err(grammar("a reserved word cannot label a statement", at));
    }
    check_name(name, at)?;
    Ok(name)
}

/// A linear form: its terms in order, and the constant it carried.
struct Form {
    terms: Vec<(usize, Frac)>,
    constant: Frac,
    constants: usize,
}

/// Reads a linear form, stopping at a relational operator or the end.
fn linear(
    cursor: &mut Cursor<'_>,
    at: usize,
    columns: &mut Vec<Slot>,
    names: &mut Names,
    declaring: bool,
) -> Result<Form, Rejected> {
    let mut form = Form {
        terms: Vec::new(),
        constant: Frac::zero(),
        constants: 0,
    };
    let mut first = true;
    loop {
        let mark = cursor.at;
        let sign = cursor.sign();
        if sign.is_some() && cursor.sign().is_some() {
            return Err(grammar("two signs in a row", at));
        }
        if sign.is_none() {
            if !first {
                // Anything that is not a sign ends the form; the caller reads
                // whatever comes next.
                cursor.at = mark;
                return Ok(form);
            }
            cursor.at = mark;
        }
        let positive = sign.unwrap_or(true);

        let token = cursor.digits();
        let magnitude = match token {
            Some(text) => number(text, at)?,
            None => Frac::one(),
        };
        let value = if positive {
            magnitude
        } else {
            magnitude.negated()
        };

        let mark = cursor.at;
        match cursor.word() {
            Some(name) if !reserved(name) => {
                representable(&value, "a coefficient", at)?;
                check_name(name, at)?;
                let column = match names.find(name) {
                    Some(found) => found,
                    None => {
                        if !declaring {
                            return Err(Rejected::new(
                                Code::UNKNOWN_COLUMN,
                                format!("{name:?} appears in no objective or constraint term"),
                            )
                            .on_line(at));
                        }
                        let index = columns.len();
                        columns.push(Slot {
                            name: name.to_owned(),
                            whole: false,
                            cost: Frac::zero(),
                            low: Some(Frac::zero()),
                            high: None,
                            low_default: true,
                            fixed_low: false,
                            fixed_high: false,
                            listed: false,
                        });
                        names.insert(name, index);
                        index
                    }
                };
                form.terms.push((column, value));
            }
            Some(_) => return Err(grammar("a reserved word cannot be a variable", at)),
            None => {
                cursor.at = mark;
                if token.is_none() {
                    if sign.is_some() {
                        return Err(grammar("a sign stands with no term after it", at));
                    }
                    return Ok(form);
                }
                representable(&value, "a constant", at)?;
                form.constant = form.constant.plus(&value);
                form.constants += 1;
            }
        }
        first = false;
    }
}

/// Reads the body of a constraint statement.
type Sides = (Option<Frac>, Option<Frac>, Vec<(usize, Frac)>);
fn row(
    cursor: &mut Cursor<'_>,
    at: usize,
    columns: &mut Vec<Slot>,
    names: &mut Names,
) -> Result<Sides, Rejected> {
    let opening = linear(cursor, at, columns, names, true)?;
    let Some(first) = cursor.relation() else {
        return Err(grammar("a constraint carries a relational operator", at));
    };

    // A form that turned out to be one bare number was the near endpoint of a
    // two-sided constraint; anything else was the left-hand side of a
    // one-sided one. The two shapes are told apart here rather than by
    // counting operators first, because the reader is walking forwards.
    if opening.terms.is_empty() && opening.constants == 1 {
        let middle = linear(cursor, at, columns, names, true)?;
        let Some(second) = cursor.relation() else {
            return Err(grammar("a two-sided constraint carries two operators", at));
        };
        if second != first {
            return Err(grammar(
                "the two operators of a two-sided constraint point different ways",
                at,
            ));
        }
        if first == Rel::Eq {
            return Err(grammar(
                "a two-sided constraint cannot be written with equalities",
                at,
            ));
        }
        if middle.constants > 0 {
            return Err(grammar(
                "a constant inside a constraint belongs on its right-hand side",
                at,
            ));
        }
        if middle.terms.is_empty() {
            return Err(grammar("a constraint states a fact about no variables", at));
        }
        let far = lone(cursor, at)?;
        if !cursor.done() {
            return Err(grammar("content follows the end of a constraint", at));
        }
        return Ok(match first {
            Rel::Le => (Some(opening.constant), Some(far), middle.terms),
            _ => (Some(far), Some(opening.constant), middle.terms),
        });
    }

    if opening.constants > 0 {
        return Err(grammar(
            "a constant inside a constraint belongs on its right-hand side",
            at,
        ));
    }
    if opening.terms.is_empty() {
        return Err(grammar("a constraint states a fact about no variables", at));
    }
    let value = lone(cursor, at)?;
    if !cursor.done() {
        return Err(grammar("content follows the end of a constraint", at));
    }
    Ok(match first {
        Rel::Le => (None, Some(value), opening.terms),
        Rel::Ge => (Some(value), None, opening.terms),
        Rel::Eq => (Some(value.clone()), Some(value), opening.terms),
    })
}

/// Reads a signed number standing on its own.
fn lone(cursor: &mut Cursor<'_>, at: usize) -> Result<Frac, Rejected> {
    let positive = cursor.sign().unwrap_or(true);
    let Some(token) = cursor.digits() else {
        return Err(grammar("a number was expected here", at));
    };
    let value = number(token, at)?;
    let value = if positive { value } else { value.negated() };
    representable(&value, "a right-hand side", at)?;
    Ok(value)
}

/// A bound endpoint: a number, or the absence of that side.
enum Edge {
    Number(Frac),
    /// True when the spelling carried a plus.
    Away(bool),
}

/// Reads a bound endpoint, which may be an absence.
fn edge(cursor: &mut Cursor<'_>, at: usize) -> Result<Edge, Rejected> {
    let sign = cursor.sign();
    let mark = cursor.at;
    if let Some(word) = cursor.word() {
        let shape = word.to_ascii_uppercase();
        if shape == "INF" || shape == "INFINITY" {
            return match sign {
                Some(positive) => Ok(Edge::Away(positive)),
                None => Err(grammar(
                    "an infinite bound carries the sign of the side it names",
                    at,
                )),
            };
        }
        cursor.at = mark;
        return Err(grammar("a number was expected here", at));
    }
    let Some(token) = cursor.digits() else {
        return Err(grammar("a number was expected here", at));
    };
    let value = number(token, at)?;
    let value = if sign.unwrap_or(true) {
        value
    } else {
        value.negated()
    };
    representable(&value, "a bound", at)?;
    Ok(Edge::Number(value))
}

/// Counts the relational operators in a statement.
///
/// The shape of a bound statement is decided from this count before any of its
/// pieces are read, so that a name is looked up only inside a statement whose
/// shape is accepted. Resolving a name in a statement nobody can parse is a
/// guess about what its author meant.
fn operators(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut count = 0usize;
    let mut at = 0usize;
    while at < bytes.len() {
        match bytes[at] {
            b'<' | b'>' => {
                count += 1;
                at += 1;
                if bytes.get(at) == Some(&b'=') {
                    at += 1;
                }
            }
            b'=' => {
                count += 1;
                at += 1;
            }
            _ => at += 1,
        }
    }
    count
}

/// Reads one bound statement.
fn bound(
    cursor: &mut Cursor<'_>,
    at: usize,
    columns: &mut [Slot],
    names: &Names,
) -> Result<(), Rejected> {
    let shape = "a bound statement is not one of the accepted forms";
    match operators(cursor.text) {
        0 => {
            let Some(name) = cursor.word() else {
                return Err(grammar(shape, at));
            };
            let free = cursor
                .word()
                .is_some_and(|word| word.eq_ignore_ascii_case("FREE"));
            if !free || !cursor.done() {
                return Err(grammar(shape, at));
            }
            let column = find(name, names, at)?;
            set_low(columns, column, Edge::Away(false), at)?;
            set_high(columns, column, Edge::Away(true), at)
        }
        1 => {
            let Some(name) = cursor.word() else {
                return Err(grammar("a one-sided bound names its variable first", at));
            };
            let Some(relation) = cursor.relation() else {
                return Err(grammar("a one-sided bound names its variable first", at));
            };
            let column = find(name, names, at)?;
            let endpoint = edge(cursor, at)?;
            if !cursor.done() {
                return Err(grammar("content follows the end of a bound", at));
            }
            match relation {
                Rel::Le => set_high(columns, column, endpoint, at),
                Rel::Ge => set_low(columns, column, endpoint, at),
                Rel::Eq => match endpoint {
                    Edge::Away(_) => Err(grammar("a variable cannot be fixed at an infinity", at)),
                    Edge::Number(value) => {
                        set_low(columns, column, Edge::Number(value.clone()), at)?;
                        set_high(columns, column, Edge::Number(value), at)
                    }
                },
            }
        }
        2 => {
            // The endpoints are read after the variable, because the variable
            // is what says whether the statement is about anything at all, and
            // then in the order they were written.
            let near_start = cursor.at;
            let Some((first, first_at)) = skip_to_relation(cursor) else {
                return Err(grammar(shape, at));
            };
            let Some(name) = cursor.word() else {
                return Err(grammar(
                    "a two-sided bound names its variable between the operators",
                    at,
                ));
            };
            let Some(second) = cursor.relation() else {
                return Err(grammar(
                    "a two-sided bound names its variable between the operators",
                    at,
                ));
            };
            if second != first {
                return Err(grammar(
                    "the two operators of a two-sided bound point different ways",
                    at,
                ));
            }
            if first == Rel::Eq {
                return Err(grammar(
                    "a two-sided bound cannot be written with equalities",
                    at,
                ));
            }
            let column = find(name, names, at)?;
            let mut before = Cursor {
                text: cursor.text,
                at: near_start,
            };
            let near = edge(&mut before, at)?;
            before.blanks();
            if before.at != first_at {
                return Err(grammar(
                    "content follows a number that should have stood alone",
                    at,
                ));
            }
            let far = edge(cursor, at)?;
            if !cursor.done() {
                return Err(grammar("content follows the end of a bound", at));
            }
            // The lower side is always set first, so a two-sided form never
            // trips the ambiguity a negative upper bound raises against a
            // default lower one.
            match first {
                Rel::Le => {
                    set_low(columns, column, near, at)?;
                    set_high(columns, column, far, at)
                }
                _ => {
                    set_low(columns, column, far, at)?;
                    set_high(columns, column, near, at)
                }
            }
        }
        _ => Err(grammar(shape, at)),
    }
}

/// Advances to the first relational operator, consumes it, and says where it
/// began, so that the caller can check the endpoint before it stood alone.
fn skip_to_relation(cursor: &mut Cursor<'_>) -> Option<(Rel, usize)> {
    loop {
        cursor.blanks();
        if cursor.at >= cursor.text.len() {
            return None;
        }
        let before = cursor.at;
        if let Some(relation) = cursor.relation() {
            return Some((relation, before));
        }
        cursor.at += 1;
    }
}

/// Reads a line of an integrality section.
///
/// The names are collected before any of them is resolved. A line is lexed and
/// then interpreted, never both at once: a character that begins no name makes
/// the whole line unreadable, and reporting a lookup failure from the first
/// half of a line whose second half is not even a list of names would name a
/// defect that is not the one the file has.
fn whole(
    cursor: &mut Cursor<'_>,
    at: usize,
    columns: &mut [Slot],
    names: &Names,
    zeroone: bool,
) -> Result<(), Rejected> {
    let mut listed = Vec::new();
    while !cursor.done() {
        let Some(name) = cursor.word() else {
            return Err(grammar(
                "an integrality section lists names and nothing else",
                at,
            ));
        };
        listed.push(name);
    }

    for name in listed {
        if reserved(name) {
            return Err(grammar("a reserved word cannot be a variable", at));
        }
        let column = find(name, names, at)?;
        if columns[column].listed {
            return Err(Rejected::new(
                Code::DUPLICATE_COLUMN,
                format!("{name:?} is listed as integer twice"),
            )
            .on_line(at));
        }
        columns[column].listed = true;
        columns[column].whole = true;
        if zeroone {
            if columns[column].fixed_low || columns[column].fixed_high {
                return Err(Rejected::new(
                    Code::DUPLICATE_BOUND,
                    format!("{name:?} is bounded by a statement and by the binary section"),
                )
                .on_line(at));
            }
            columns[column].low = Some(Frac::zero());
            columns[column].high = Some(Frac::one());
            columns[column].low_default = false;
            columns[column].fixed_low = true;
            columns[column].fixed_high = true;
        }
    }
    Ok(())
}

fn find(name: &str, names: &Names, at: usize) -> Result<usize, Rejected> {
    names.find(name).ok_or_else(|| {
        Rejected::new(
            Code::UNKNOWN_COLUMN,
            format!("{name:?} appears in no objective or constraint term"),
        )
        .on_line(at)
    })
}

fn set_low(columns: &mut [Slot], column: usize, edge: Edge, at: usize) -> Result<(), Rejected> {
    if columns[column].fixed_low {
        return Err(Rejected::new(
            Code::DUPLICATE_BOUND,
            format!("the lower bound of {:?} is set twice", columns[column].name),
        )
        .on_line(at));
    }
    columns[column].low = match edge {
        Edge::Away(true) => {
            return Err(grammar(
                "a lower bound of positive infinity names an empty column",
                at,
            ));
        }
        Edge::Away(false) => None,
        Edge::Number(value) => Some(value),
    };
    columns[column].low_default = false;
    columns[column].fixed_low = true;
    Ok(())
}

fn set_high(columns: &mut [Slot], column: usize, edge: Edge, at: usize) -> Result<(), Rejected> {
    if columns[column].fixed_high {
        return Err(Rejected::new(
            Code::DUPLICATE_BOUND,
            format!("the upper bound of {:?} is set twice", columns[column].name),
        )
        .on_line(at));
    }
    columns[column].high = match edge {
        Edge::Away(false) => {
            return Err(grammar(
                "an upper bound of negative infinity names an empty column",
                at,
            ));
        }
        Edge::Away(true) => None,
        Edge::Number(value) => {
            if value.is_minus() && columns[column].low_default {
                // Readers disagree about whether this opens the lower side as
                // well, and the two readings differ on which points are
                // feasible. This crate has no lenient mode in which to guess.
                return Err(Rejected::new(
                    Code::AMBIGUOUS_NEGATIVE_UPPER,
                    format!(
                        "column {:?} takes a negative upper bound while still at the default lower bound",
                        columns[column].name
                    ),
                )
                .on_line(at));
            }
            Some(value)
        }
    };
    columns[column].fixed_high = true;
    Ok(())
}
