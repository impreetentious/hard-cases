//! The LP reader: bytes in, canonical exact model out.
//!
//! The shape of the work is set by the format. There is no statement
//! terminator, so the reader carries one pending statement at a time and
//! closes it when a label or a section keyword arrives. Everything else is a
//! single pass: names are declared by their first appearance in a term, rows
//! are complete when their statement closes, and the bound and integrality
//! sections come afterwards and refer back.
//!
//! Where the accepted subset excludes a reading, this refuses rather than
//! choosing. The cost is files, and the alternative is proving a theorem about
//! a model the author did not write.

use std::collections::BTreeMap;

use hc_core::{
    FormatTag, Model, ModelParts, Refusal, Rejection, Row, RowId, Sense, VarId, Variable,
    instance_digest,
};
use hc_exact::Rational;

use crate::lp::token::{self, Relation, Token};
use crate::numeric::Limits;
use crate::text::admit_text;

/// How to read a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// Whether to refuse the one reading this format leaves ambiguous.
    ///
    /// Strict is the default and is what a certificate requires. A lenient
    /// parse records what it forgave by name; the model it produces may be
    /// solved and may not carry a claim unless the verifier's strict parser
    /// independently accepts the same bytes.
    pub strict: bool,
    /// The caps and fences.
    pub limits: Limits,
}

impl Options {
    /// Strict reading.
    #[must_use]
    pub const fn strict() -> Self {
        Self {
            strict: true,
            limits: Limits::SPEC,
        }
    }

    /// Lenient reading.
    #[must_use]
    pub const fn lenient() -> Self {
        Self {
            strict: false,
            limits: Limits::SPEC,
        }
    }
}

impl Default for Options {
    fn default() -> Self {
        Self::strict()
    }
}

/// A model, and whatever a lenient parse had to forgive to produce it.
#[derive(Clone, Debug)]
pub struct Parsed {
    /// The canonical exact model.
    pub model: Model,
    /// One entry per ambiguity a lenient parse resolved rather than refused.
    pub forgiven: Vec<String>,
}

/// Reads a file strictly.
///
/// # Errors
///
/// Returns a refusal naming the rule that refused the file.
pub fn read(raw: &[u8]) -> Result<Model, Refusal> {
    read_with(raw, Options::strict()).map(|parsed| parsed.model)
}

/// Reads a file under explicit options.
///
/// # Errors
///
/// Returns a refusal naming the rule that refused the file.
pub fn read_with(raw: &[u8], options: Options) -> Result<Parsed, Refusal> {
    options.limits.admit_file(raw.len())?;
    let text = admit_text(raw)?;
    let digest = instance_digest(FormatTag::Lp.byte(), raw);
    Builder::new(options, digest).run(text)
}

/// The sections, in the order they must appear.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Objective(Sense),
    Constraints,
    Bounds,
    General,
    Binary,
    End,
}

impl Part {
    const fn place(self) -> usize {
        match self {
            Self::Objective(_) => 0,
            Self::Constraints => 1,
            Self::Bounds => 2,
            Self::General => 3,
            Self::Binary => 4,
            Self::End => 5,
        }
    }

    const fn word(self) -> &'static str {
        match self {
            Self::Objective(Sense::Min) => "the minimizing objective",
            Self::Objective(Sense::Max) => "the maximizing objective",
            Self::Constraints => "the constraint section",
            Self::Bounds => "the bound section",
            Self::General => "the general integrality section",
            Self::Binary => "the binary integrality section",
            Self::End => "the terminator",
        }
    }
}

/// The section keywords, upper-cased and single-spaced as they are matched.
const KEYWORDS: &[(&str, Part)] = &[
    ("MINIMIZE", Part::Objective(Sense::Min)),
    ("MINIMISE", Part::Objective(Sense::Min)),
    ("MINIMUM", Part::Objective(Sense::Min)),
    ("MIN", Part::Objective(Sense::Min)),
    ("MAXIMIZE", Part::Objective(Sense::Max)),
    ("MAXIMISE", Part::Objective(Sense::Max)),
    ("MAXIMUM", Part::Objective(Sense::Max)),
    ("MAX", Part::Objective(Sense::Max)),
    ("SUBJECT TO", Part::Constraints),
    ("SUCH THAT", Part::Constraints),
    ("S.T.", Part::Constraints),
    ("ST.", Part::Constraints),
    ("ST", Part::Constraints),
    ("BOUNDS", Part::Bounds),
    ("BOUND", Part::Bounds),
    ("GENERALS", Part::General),
    ("GENERAL", Part::General),
    ("GEN", Part::General),
    ("BINARIES", Part::Binary),
    ("BINARY", Part::Binary),
    ("BIN", Part::Binary),
    ("END", Part::End),
];

/// Section keywords this subset knows about and does not accept.
///
/// They are named rather than left to fall through to the statement grammar
/// for the reason the MPS reader names QUADOBJ and SOS: a file carrying one of
/// these sections is not a file this subset misread, it is a file this subset
/// does not accept, and the two deserve different identifiers. Anything not on
/// this list and not a keyword is read as a statement and refused by the
/// grammar, which is the honest answer for a word nobody has heard of.
const REFUSED: &[&str] = &[
    "SEMI-CONTINUOUS",
    "SEMIS",
    "SEMI",
    "SOS",
    "SOS1",
    "SOS2",
    "INDICATORS",
    "LAZY CONSTRAINTS",
    "USER CUTS",
    "PWL",
    "QMATRIX",
    "QUADRATIC OBJECTIVE",
];

/// What a line turned out to be.
enum Opening {
    /// A section header on a line of its own.
    Clean(Part),
    /// A section header with content after it on the same line.
    Trailing(Part),
    /// A section this subset knows about and does not accept.
    Refused,
}

/// Recognises a section header.
///
/// Trailing content is reported rather than read. A keyword sharing its line
/// with a statement would need the statement to carry a label the format does
/// not require there, and reading it as one would put the reader back in the
/// business of guessing where statements begin.
fn opening(shape: &str) -> Option<Opening> {
    for word in REFUSED {
        if shape == *word
            || shape
                .strip_prefix(*word)
                .is_some_and(|rest| rest.starts_with(' '))
        {
            return Some(Opening::Refused);
        }
    }
    for (word, part) in KEYWORDS {
        if shape == *word {
            return Some(Opening::Clean(*part));
        }
    }
    for (word, part) in KEYWORDS {
        if let Some(rest) = shape.strip_prefix(*word) {
            if rest.starts_with(' ') {
                return Some(Opening::Trailing(*part));
            }
        }
    }
    None
}

/// A bound endpoint: a number, or the absence of that side.
enum Endpoint {
    Finite(Rational),
    /// True when the spelling carried a plus sign.
    Infinite(bool),
}

/// A declared row and the two sides its statement gave it.
struct RowDecl {
    name: String,
    lower: Option<Rational>,
    upper: Option<Rational>,
}

/// A declared column and whatever has been said about it.
struct ColumnDecl {
    name: String,
    integral: bool,
    objective: Rational,
    lower: Option<Rational>,
    upper: Option<Rational>,
    /// Whether the lower bound is still the default of zero.
    lower_is_default: bool,
    /// Whether a statement has set each side, so a repeat can be refused.
    set_lower: bool,
    set_upper: bool,
    /// Whether an integrality section has already listed the column.
    listed: bool,
}

/// One objective or constraint statement, gathered across its lines.
struct Statement<'a> {
    line: usize,
    tokens: Vec<Token<'a>>,
}

/// The reader's state.
struct Builder {
    options: Options,
    digest: [u8; 32],
    forgiven: Vec<String>,

    part: Option<Part>,
    seen: [bool; 6],
    highest: Option<usize>,
    ended: bool,
    sense: Sense,

    rows: Vec<RowDecl>,
    row_index: BTreeMap<String, usize>,
    columns: Vec<ColumnDecl>,
    column_index: BTreeMap<String, usize>,
    entries: BTreeMap<(usize, usize), (Rational, u32)>,
    objective_constant: Rational,
}

impl Builder {
    fn new(options: Options, digest: [u8; 32]) -> Self {
        Self {
            options,
            digest,
            forgiven: Vec::new(),
            part: None,
            seen: [false; 6],
            highest: None,
            ended: false,
            sense: Sense::Min,
            rows: Vec::new(),
            row_index: BTreeMap::new(),
            columns: Vec::new(),
            column_index: BTreeMap::new(),
            entries: BTreeMap::new(),
            objective_constant: Rational::zero(),
        }
    }

    fn run(mut self, text: &str) -> Result<Parsed, Refusal> {
        let mut pending: Option<Statement<'_>> = None;

        for (offset, raw_line) in text.lines().enumerate() {
            let line = offset + 1;
            let stripped = token::strip(raw_line);
            if stripped.is_empty() {
                continue;
            }

            if let Some(found) = opening(&token::keyword_shape(stripped)) {
                // The statement above is finished before this line is judged,
                // so a refusal always names the earliest line that carries a
                // defect rather than the first check that happens to run.
                if let Some(statement) = pending.take() {
                    self.close(statement)?;
                }
                let part = match found {
                    Opening::Clean(part) => part,
                    Opening::Refused => {
                        return Err(Refusal::new(
                            Rejection::INSTANCE_UNKNOWN_SECTION,
                            format!("{stripped:?} is a section this subset does not accept"),
                        )
                        .at(line));
                    }
                    Opening::Trailing(part) => {
                        return Err(Refusal::new(
                            Rejection::INSTANCE_LP_GRAMMAR,
                            format!("{} shares its line with other content", part.word()),
                        )
                        .at(line));
                    }
                };
                self.open(part, line)?;
                if part == Part::End {
                    self.ended = true;
                    break;
                }
                continue;
            }

            match self.part {
                None => {
                    return Err(Refusal::new(
                        Rejection::INSTANCE_LP_GRAMMAR,
                        "content appears before any section".to_owned(),
                    )
                    .at(line));
                }
                Some(Part::Objective(_) | Part::Constraints) => {
                    let tokens = token::tokens(stripped, line)?;
                    if labelled(&tokens) {
                        if let Some(statement) = pending.take() {
                            self.close(statement)?;
                        }
                        pending = Some(Statement { line, tokens });
                    } else {
                        match pending.as_mut() {
                            Some(statement) => statement.tokens.extend(tokens),
                            None => {
                                return Err(Refusal::new(
                                    Rejection::INSTANCE_LP_GRAMMAR,
                                    "a statement begins without a label".to_owned(),
                                )
                                .at(line));
                            }
                        }
                    }
                }
                Some(Part::Bounds) => {
                    let tokens = token::tokens(stripped, line)?;
                    self.bound(&tokens, line)?;
                }
                Some(part @ (Part::General | Part::Binary)) => {
                    let tokens = token::tokens(stripped, line)?;
                    self.integrality(&tokens, line, part == Part::Binary)?;
                }
                Some(Part::End) => unreachable!("the terminator ends the walk"),
            }
        }

        if let Some(statement) = pending.take() {
            self.close(statement)?;
        }
        self.finish()
    }

    /// Opens a section, checking its order and that it has not appeared before.
    fn open(&mut self, part: Part, line: usize) -> Result<(), Refusal> {
        let place = part.place();
        if self.seen[place] {
            return Err(Refusal::new(
                Rejection::INSTANCE_SECTION_ORDER,
                format!("{} appears twice", part.word()),
            )
            .at(line));
        }
        if let Some(highest) = self.highest {
            if place < highest {
                return Err(Refusal::new(
                    Rejection::INSTANCE_SECTION_ORDER,
                    format!("{} appears out of order", part.word()),
                )
                .at(line));
            }
        }
        self.seen[place] = true;
        self.highest = Some(place);
        if let Part::Objective(sense) = part {
            self.sense = sense;
        }
        self.part = Some(part);
        Ok(())
    }

    /// Reads a completed objective or constraint statement.
    fn close(&mut self, statement: Statement<'_>) -> Result<(), Refusal> {
        let line = statement.line;
        let label = match statement.tokens.as_slice() {
            [Token::Name(name), Token::Colon, ..] => *name,
            _ => {
                return Err(Refusal::new(
                    Rejection::INSTANCE_LP_GRAMMAR,
                    "a statement begins without a label".to_owned(),
                )
                .at(line));
            }
        };
        if token::is_reserved(label) {
            return Err(Refusal::new(
                Rejection::INSTANCE_LP_GRAMMAR,
                format!("{label:?} is a reserved word and cannot label a statement"),
            )
            .at(line));
        }
        self.options.limits.admit_name(label, line)?;
        let rest = &statement.tokens[2..];
        match self.part {
            Some(Part::Objective(_)) => self.objective(rest, line),
            Some(Part::Constraints) => self.constraint(label, rest, line),
            _ => unreachable!("statements are gathered only in those two sections"),
        }
    }

    /// Reads the objective statement.
    fn objective(&mut self, tokens: &[Token<'_>], line: usize) -> Result<(), Refusal> {
        let (terms, constant) = self.terms(tokens, line, true)?;
        for (column, value) in terms {
            let existing =
                core::mem::replace(&mut self.columns[column].objective, Rational::zero());
            self.columns[column].objective = existing.add(&value);
        }
        self.objective_constant = self.objective_constant.add(&constant);
        Ok(())
    }

    /// Reads one constraint statement.
    fn constraint(
        &mut self,
        label: &str,
        tokens: &[Token<'_>],
        line: usize,
    ) -> Result<(), Refusal> {
        let relations: Vec<usize> = tokens
            .iter()
            .enumerate()
            .filter_map(|(at, item)| matches!(item, Token::Relation(_)).then_some(at))
            .collect();
        let relation_at = |at: usize| match tokens[at] {
            Token::Relation(relation) => relation,
            _ => unreachable!("the positions were collected from relations"),
        };

        let (terms, lower, upper) = match relations.as_slice() {
            [only] => {
                let terms = self.terms(&tokens[..*only], line, false)?.0;
                let value = self.number(&tokens[only + 1..], line)?;
                match relation_at(*only) {
                    Relation::AtMost => (terms, None, Some(value)),
                    Relation::AtLeast => (terms, Some(value), None),
                    Relation::Equal => (terms, Some(value.clone()), Some(value)),
                }
            }
            [first, second] => {
                let left = relation_at(*first);
                if left != relation_at(*second) {
                    return Err(Refusal::new(
                        Rejection::INSTANCE_LP_GRAMMAR,
                        "the two operators of a two-sided constraint point different ways"
                            .to_owned(),
                    )
                    .at(line));
                }
                if left == Relation::Equal {
                    return Err(Refusal::new(
                        Rejection::INSTANCE_LP_GRAMMAR,
                        "a two-sided constraint cannot be written with equalities".to_owned(),
                    )
                    .at(line));
                }
                let near = self.number(&tokens[..*first], line)?;
                let terms = self.terms(&tokens[first + 1..*second], line, false)?.0;
                let far = self.number(&tokens[second + 1..], line)?;
                match left {
                    Relation::AtMost => (terms, Some(near), Some(far)),
                    _ => (terms, Some(far), Some(near)),
                }
            }
            _ => {
                return Err(Refusal::new(
                    Rejection::INSTANCE_LP_GRAMMAR,
                    format!(
                        "a constraint carries one or two relational operators, not {}",
                        relations.len()
                    ),
                )
                .at(line));
            }
        };

        if terms.is_empty() {
            return Err(Refusal::new(
                Rejection::INSTANCE_LP_GRAMMAR,
                "a constraint states a fact about no variables".to_owned(),
            )
            .at(line));
        }

        if self.row_index.contains_key(label) {
            return Err(Refusal::new(
                Rejection::INSTANCE_DUPLICATE_ROW,
                format!("two constraints share the label {label:?}"),
            )
            .at(line));
        }
        let row = self.rows.len();
        self.row_index.insert(label.to_owned(), row);
        self.rows.push(RowDecl {
            name: label.to_owned(),
            lower,
            upper,
        });

        for (column, value) in terms {
            match self.entries.get_mut(&(column, row)) {
                Some(slot) => {
                    let existing = core::mem::replace(&mut slot.0, Rational::zero());
                    slot.0 = existing.add(&value);
                    slot.1 += 1;
                }
                None => {
                    self.options
                        .limits
                        .admit_nonzero(self.entries.len(), line)?;
                    self.entries.insert((column, row), (value, 1));
                }
            }
        }
        Ok(())
    }

    /// Reads a linear form.
    ///
    /// Returns the terms in the order they were written, together with the
    /// constant the form carried. A constant is admitted only where the caller
    /// says one belongs, which is the objective and nowhere else.
    fn terms(
        &mut self,
        tokens: &[Token<'_>],
        line: usize,
        constants: bool,
    ) -> Result<(Vec<(usize, Rational)>, Rational), Refusal> {
        let grammar =
            |detail: String| Refusal::new(Rejection::INSTANCE_LP_GRAMMAR, detail).at(line);
        let mut out = Vec::new();
        let mut constant = Rational::zero();
        let mut at = 0usize;
        let mut first = true;

        while at < tokens.len() {
            let mut positive = true;
            if let Some(Token::Sign(sign)) = tokens.get(at) {
                positive = *sign;
                at += 1;
                if matches!(tokens.get(at), Some(Token::Sign(_))) {
                    return Err(grammar("two signs in a row".to_owned()));
                }
            } else if !first {
                return Err(grammar(
                    "the terms of a linear form are joined by a sign".to_owned(),
                ));
            }

            let written = matches!(tokens.get(at), Some(Token::Number(_)));
            let magnitude = match tokens.get(at) {
                Some(Token::Number(text)) => {
                    at += 1;
                    self.options.limits.read_number(text, line)?
                }
                _ => Rational::one(),
            };
            let value = if positive { magnitude } else { magnitude.neg() };

            match tokens.get(at) {
                Some(Token::Name(name)) => {
                    if token::is_reserved(name) {
                        return Err(grammar(format!(
                            "{name:?} is a reserved word and cannot be a variable"
                        )));
                    }
                    at += 1;
                    Limits::admit_value(&value, "a coefficient", line)?;
                    let column = self.declare(name, line)?;
                    out.push((column, value));
                }
                _ if written => {
                    if !constants {
                        return Err(grammar(
                            "a constant inside a constraint belongs on its right-hand side"
                                .to_owned(),
                        ));
                    }
                    Limits::admit_value(&value, "an objective constant", line)?;
                    constant = constant.add(&value);
                }
                _ => return Err(grammar("a sign stands with no term after it".to_owned())),
            }
            first = false;
        }
        Ok((out, constant))
    }

    /// Reads a signed number standing alone.
    fn number(&self, tokens: &[Token<'_>], line: usize) -> Result<Rational, Refusal> {
        let (value, rest) = match tokens {
            [Token::Number(text), rest @ ..] => {
                (self.options.limits.read_number(text, line)?, rest)
            }
            [Token::Sign(sign), Token::Number(text), rest @ ..] => {
                let magnitude = self.options.limits.read_number(text, line)?;
                (if *sign { magnitude } else { magnitude.neg() }, rest)
            }
            _ => {
                return Err(Refusal::new(
                    Rejection::INSTANCE_LP_GRAMMAR,
                    "a number was expected here".to_owned(),
                )
                .at(line));
            }
        };
        if !rest.is_empty() {
            return Err(Refusal::new(
                Rejection::INSTANCE_LP_GRAMMAR,
                "content follows a number that should have stood alone".to_owned(),
            )
            .at(line));
        }
        Limits::admit_value(&value, "a right-hand side", line)?;
        Ok(value)
    }

    /// Reads a bound endpoint, which may be an absence.
    fn endpoint(&self, tokens: &[Token<'_>], line: usize) -> Result<Endpoint, Refusal> {
        if let [Token::Sign(sign), Token::Name(word)] = tokens {
            if token::is_infinity(word) {
                return Ok(Endpoint::Infinite(*sign));
            }
        }
        if let [Token::Name(word)] = tokens {
            if token::is_infinity(word) {
                return Err(Refusal::new(
                    Rejection::INSTANCE_LP_GRAMMAR,
                    "an infinite bound carries the sign of the side it names".to_owned(),
                )
                .at(line));
            }
        }
        self.number(tokens, line).map(Endpoint::Finite)
    }

    /// Reads one bound statement.
    fn bound(&mut self, tokens: &[Token<'_>], line: usize) -> Result<(), Refusal> {
        let grammar =
            |detail: String| Refusal::new(Rejection::INSTANCE_LP_GRAMMAR, detail).at(line);

        if let [Token::Name(name), Token::Name(word)] = tokens {
            if token::is_free(word) {
                let column = self.known(name, line)?;
                self.apply_lower(column, Endpoint::Infinite(false), line)?;
                return self.apply_upper(column, Endpoint::Infinite(true), line);
            }
        }

        let relations: Vec<usize> = tokens
            .iter()
            .enumerate()
            .filter_map(|(at, item)| matches!(item, Token::Relation(_)).then_some(at))
            .collect();
        let relation_at = |at: usize| match tokens[at] {
            Token::Relation(relation) => relation,
            _ => unreachable!("the positions were collected from relations"),
        };

        match relations.as_slice() {
            [only] => {
                let column = match &tokens[..*only] {
                    [Token::Name(name)] => self.known(name, line)?,
                    _ => {
                        return Err(grammar(
                            "a one-sided bound names its variable first".to_owned(),
                        ));
                    }
                };
                let endpoint = self.endpoint(&tokens[only + 1..], line)?;
                match relation_at(*only) {
                    Relation::AtMost => self.apply_upper(column, endpoint, line),
                    Relation::AtLeast => self.apply_lower(column, endpoint, line),
                    Relation::Equal => {
                        let value = match endpoint {
                            Endpoint::Finite(value) => value,
                            Endpoint::Infinite(_) => {
                                return Err(grammar(
                                    "a variable cannot be fixed at an infinity".to_owned(),
                                ));
                            }
                        };
                        self.apply_lower(column, Endpoint::Finite(value.clone()), line)?;
                        self.apply_upper(column, Endpoint::Finite(value), line)
                    }
                }
            }
            [first, second] => {
                let left = relation_at(*first);
                if left != relation_at(*second) {
                    return Err(grammar(
                        "the two operators of a two-sided bound point different ways".to_owned(),
                    ));
                }
                if left == Relation::Equal {
                    return Err(grammar(
                        "a two-sided bound cannot be written with equalities".to_owned(),
                    ));
                }
                let column = match &tokens[first + 1..*second] {
                    [Token::Name(name)] => self.known(name, line)?,
                    _ => {
                        return Err(grammar(
                            "a two-sided bound names its variable between the operators".to_owned(),
                        ));
                    }
                };
                let near = self.endpoint(&tokens[..*first], line)?;
                let far = self.endpoint(&tokens[second + 1..], line)?;
                // The lower side is always applied first, so a two-sided form
                // never trips the ambiguity that a negative upper bound raises
                // against a lower bound nobody has given.
                match left {
                    Relation::AtMost => {
                        self.apply_lower(column, near, line)?;
                        self.apply_upper(column, far, line)
                    }
                    _ => {
                        self.apply_lower(column, far, line)?;
                        self.apply_upper(column, near, line)
                    }
                }
            }
            _ => Err(grammar(
                "a bound statement is not one of the accepted forms".to_owned(),
            )),
        }
    }

    /// Sets the lower side of a bound.
    fn apply_lower(
        &mut self,
        column: usize,
        endpoint: Endpoint,
        line: usize,
    ) -> Result<(), Refusal> {
        if self.columns[column].set_lower {
            return Err(Refusal::new(
                Rejection::INSTANCE_DUPLICATE_BOUND,
                format!(
                    "the lower bound of {:?} is set twice",
                    self.columns[column].name
                ),
            )
            .at(line));
        }
        let value = match endpoint {
            Endpoint::Infinite(true) => {
                return Err(Refusal::new(
                    Rejection::INSTANCE_LP_GRAMMAR,
                    "a lower bound of positive infinity names an empty column".to_owned(),
                )
                .at(line));
            }
            Endpoint::Infinite(false) => None,
            Endpoint::Finite(value) => {
                Limits::admit_value(&value, "a bound", line)?;
                Some(value)
            }
        };
        self.columns[column].lower = value;
        self.columns[column].lower_is_default = false;
        self.columns[column].set_lower = true;
        Ok(())
    }

    /// Sets the upper side of a bound.
    fn apply_upper(
        &mut self,
        column: usize,
        endpoint: Endpoint,
        line: usize,
    ) -> Result<(), Refusal> {
        if self.columns[column].set_upper {
            return Err(Refusal::new(
                Rejection::INSTANCE_DUPLICATE_BOUND,
                format!(
                    "the upper bound of {:?} is set twice",
                    self.columns[column].name
                ),
            )
            .at(line));
        }
        let value = match endpoint {
            Endpoint::Infinite(false) => {
                return Err(Refusal::new(
                    Rejection::INSTANCE_LP_GRAMMAR,
                    "an upper bound of negative infinity names an empty column".to_owned(),
                )
                .at(line));
            }
            Endpoint::Infinite(true) => None,
            Endpoint::Finite(value) => {
                Limits::admit_value(&value, "a bound", line)?;
                if value.is_negative() && self.columns[column].lower_is_default {
                    // Readers disagree about whether this opens the lower side
                    // as well, and the two readings differ on which points are
                    // feasible. Giving the lower bound first accepts the file.
                    if self.options.strict {
                        return Err(Refusal::new(
                            Rejection::INSTANCE_AMBIGUOUS_NEGATIVE_UPPER,
                            format!(
                                "column {:?} takes a negative upper bound while still at the default lower bound",
                                self.columns[column].name
                            ),
                        )
                        .at(line));
                    }
                    self.forgiven.push(format!(
                        "line {line}: negative upper bound on column {:?} read as removing the lower bound",
                        self.columns[column].name
                    ));
                    self.columns[column].lower = None;
                    self.columns[column].lower_is_default = false;
                }
                Some(value)
            }
        };
        self.columns[column].upper = value;
        self.columns[column].set_upper = true;
        Ok(())
    }

    /// Reads a line of an integrality section.
    fn integrality(
        &mut self,
        tokens: &[Token<'_>],
        line: usize,
        binary: bool,
    ) -> Result<(), Refusal> {
        // The names are read before any of them is resolved. A line is lexed
        // and then interpreted, never both at once: reporting a lookup failure
        // from the first half of a line whose second half is not even a list
        // of names would name a defect that is not the one the file has.
        let mut listed = Vec::with_capacity(tokens.len());
        for item in tokens {
            let Token::Name(name) = item else {
                return Err(Refusal::new(
                    Rejection::INSTANCE_LP_GRAMMAR,
                    "an integrality section lists names and nothing else".to_owned(),
                )
                .at(line));
            };
            listed.push(name);
        }

        for name in listed {
            if token::is_reserved(name) {
                return Err(Refusal::new(
                    Rejection::INSTANCE_LP_GRAMMAR,
                    format!("{name:?} is a reserved word and cannot be a variable"),
                )
                .at(line));
            }
            let column = self.known(name, line)?;
            if self.columns[column].listed {
                return Err(Refusal::new(
                    Rejection::INSTANCE_DUPLICATE_COLUMN,
                    format!("{name:?} is listed as integer twice"),
                )
                .at(line));
            }
            self.columns[column].listed = true;
            self.columns[column].integral = true;
            if binary {
                if self.columns[column].set_lower || self.columns[column].set_upper {
                    // The section sets both sides. A bound statement for the
                    // same column says the same thing twice, and a reader
                    // would have to decide which of the two wins.
                    return Err(Refusal::new(
                        Rejection::INSTANCE_DUPLICATE_BOUND,
                        format!("{name:?} is bounded by a statement and by the binary section"),
                    )
                    .at(line));
                }
                self.columns[column].lower = Some(Rational::zero());
                self.columns[column].upper = Some(Rational::one());
                self.columns[column].lower_is_default = false;
                self.columns[column].set_lower = true;
                self.columns[column].set_upper = true;
            }
        }
        Ok(())
    }

    /// Declares a column on its first appearance in a term.
    fn declare(&mut self, name: &str, line: usize) -> Result<usize, Refusal> {
        if let Some(found) = self.column_index.get(name) {
            return Ok(*found);
        }
        self.options.limits.admit_name(name, line)?;
        let index = self.columns.len();
        self.columns.push(ColumnDecl {
            name: name.to_owned(),
            integral: false,
            objective: Rational::zero(),
            lower: Some(Rational::zero()),
            upper: None,
            lower_is_default: true,
            set_lower: false,
            set_upper: false,
            listed: false,
        });
        self.column_index.insert(name.to_owned(), index);
        Ok(index)
    }

    /// Finds a column a term already declared.
    fn known(&self, name: &str, line: usize) -> Result<usize, Refusal> {
        self.column_index.get(name).copied().ok_or_else(|| {
            Refusal::new(
                Rejection::INSTANCE_UNKNOWN_COLUMN,
                format!("{name:?} appears in no objective or constraint term"),
            )
            .at(line)
        })
    }

    /// Assembles the model.
    fn finish(self) -> Result<Parsed, Refusal> {
        if !self.ended {
            return Err(Refusal::new(
                Rejection::INSTANCE_UNEXPECTED_END,
                "input ends before the terminator".to_owned(),
            ));
        }
        for (place, part) in [(0, "the objective section"), (1, "the constraint section")] {
            if !self.seen[place] {
                return Err(Refusal::new(
                    Rejection::INSTANCE_SECTION_MISSING,
                    format!("{part} is missing"),
                ));
            }
        }

        let negate = self.sense == Sense::Max;
        let variables = self
            .columns
            .iter()
            .map(|column| Variable {
                lower: column.lower.clone(),
                upper: column.upper.clone(),
                integral: column.integral,
                objective: if negate {
                    column.objective.neg()
                } else {
                    column.objective.clone()
                },
                name: column.name.clone(),
            })
            .collect();

        let rows = self
            .rows
            .iter()
            .map(|row| Row {
                lower: row.lower.clone(),
                upper: row.upper.clone(),
                name: row.name.clone(),
            })
            .collect();

        let mut entries = Vec::new();
        let mut zero_sums = Vec::new();
        for ((column, row), (value, contributions)) in self.entries {
            let position = (VarId(column as u32), RowId(row as u32));
            if value.is_zero() {
                // A cancellation and an absence are the same matrix and
                // different files.
                if contributions > 1 {
                    zero_sums.push(position);
                }
                continue;
            }
            entries.push((position.0, position.1, value));
        }

        let objective_constant = if negate {
            self.objective_constant.neg()
        } else {
            self.objective_constant
        };

        Ok(Parsed {
            model: Model::new(ModelParts {
                format: FormatTag::Lp,
                digest: self.digest,
                original_sense: self.sense,
                objective_constant,
                variables,
                rows,
                entries,
                zero_sums,
                discarded: Vec::new(),
            }),
            forgiven: self.forgiven,
        })
    }
}

/// Whether a line opens a statement rather than continuing one.
fn labelled(tokens: &[Token<'_>]) -> bool {
    matches!(tokens, [Token::Name(_), Token::Colon, ..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(text: &str) -> Model {
        read(text.as_bytes()).unwrap_or_else(|refusal| panic!("refused: {refusal}"))
    }

    fn refusal(text: &str) -> Refusal {
        read(text.as_bytes()).expect_err("accepted, and must not be")
    }

    fn shown(value: Option<&Rational>) -> String {
        value.map_or_else(|| "*".to_owned(), Rational::to_canonical_string)
    }

    #[test]
    fn trap_lp_statement_continuation() {
        // The fork this format is famous for. A wrapped row has to reach the
        // model as one row, and the three shapes below are the ones the usual
        // heuristics disagree about: a continuation opening with a sign, one
        // opening with a bare coefficient, and one carrying the operator.
        let wrapped =
            model("Minimize\n obj: x\nSubject To\n c1: x\n  + 2 y\n  + 3 z\n  <= 4\nEnd\n");
        let inline = model("Minimize\n obj: x\nSubject To\n c1: x + 2 y + 3 z <= 4\nEnd\n");
        let below = |value: &Model| {
            value
                .canonical_dump()
                .lines()
                .skip(3)
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert_eq!(below(&wrapped), below(&inline));
        assert_eq!(wrapped.rows().len(), 1, "a wrapped row is one row");

        // And the rule is total in the other direction: a line that does begin
        // with a label is a new statement, however much it looks like a
        // continuation of the one above.
        let two = model("Minimize\n obj: x\nSubject To\n c1: x <= 4\n c2: -x >= -9\nEnd\n");
        assert_eq!(two.rows().len(), 2);
        assert_eq!(shown(two.rows()[1].lower.as_ref()), "-9");
    }

    #[test]
    fn trap_lp_reserved_word_as_name() {
        // A variable called `free` would make `x free` two readings, and a
        // constraint called `end` would end the file. Neither is a name.
        for text in [
            "Minimize\n obj: x + free\nSubject To\n c1: x <= 4\nEnd\n",
            "Minimize\n obj: x\nSubject To\n end: x <= 4\nEnd\n",
            "Minimize\n obj: x\nSubject To\n c1: x <= 4\nGeneral\n inf\nEnd\n",
        ] {
            assert_eq!(
                refusal(text).id,
                Rejection::INSTANCE_LP_GRAMMAR,
                "accepted a reserved word: {text:?}"
            );
        }
    }

    #[test]
    fn trap_lp_double_sided_row() {
        // The same row written four ways. Whichever direction the operators
        // point, the bounds land on the same sides.
        let forms = [
            "Minimize\n obj: x\nSubject To\n r: 1 <= x + y <= 5\nEnd\n",
            "Minimize\n obj: x\nSubject To\n r: 5 >= x + y >= 1\nEnd\n",
        ];
        for text in forms {
            let built = model(text);
            assert_eq!(shown(built.rows()[0].lower.as_ref()), "1", "{text:?}");
            assert_eq!(shown(built.rows()[0].upper.as_ref()), "5", "{text:?}");
        }
        // Mixed directions are refused rather than reordered, and so is a
        // two-sided form written with equalities.
        for text in [
            "Minimize\n obj: x\nSubject To\n r: 1 <= x + y >= 5\nEnd\n",
            "Minimize\n obj: x\nSubject To\n r: 1 = x + y = 5\nEnd\n",
        ] {
            assert_eq!(refusal(text).id, Rejection::INSTANCE_LP_GRAMMAR, "{text:?}");
        }
    }

    #[test]
    fn the_default_bounds_are_the_ones_the_format_states() {
        let built = model("Minimize\n obj: x + y\nSubject To\n c: x + y <= 4\nBinary\n y\nEnd\n");
        // A variable nobody bounded: zero below, nothing above, continuous.
        assert_eq!(shown(built.variables()[0].lower.as_ref()), "0");
        assert_eq!(shown(built.variables()[0].upper.as_ref()), "*");
        assert!(!built.variables()[0].integral);
        // A binary variable is bounded by its section.
        assert_eq!(shown(built.variables()[1].lower.as_ref()), "0");
        assert_eq!(shown(built.variables()[1].upper.as_ref()), "1");
        assert!(built.variables()[1].integral);
    }

    #[test]
    fn an_objective_with_a_label_and_no_terms_is_the_zero_objective() {
        // Unlike a missing MPS objective row, the direction is stated here.
        // The emptiness is written down rather than inferred.
        let built = model("Maximize\n obj:\nSubject To\n c: x <= 4\nEnd\n");
        assert_eq!(built.variables().len(), 1);
        assert!(built.variables()[0].objective.is_zero());
        assert_eq!(built.original_sense(), Sense::Max);
    }

    #[test]
    fn a_maximizing_objective_is_negated_where_it_is_read() {
        let built = model("Maximize\n p: 2 x - 3 y + 7\nSubject To\n c: x + y <= 4\nEnd\n");
        assert_eq!(built.variables()[0].objective.to_canonical_string(), "-2");
        assert_eq!(built.variables()[1].objective.to_canonical_string(), "3");
        assert_eq!(built.objective_constant().to_canonical_string(), "-7");
        assert_eq!(built.original_sense(), Sense::Max);
    }

    #[test]
    fn a_section_keyword_may_not_share_its_line() {
        let refused = refusal("Minimize obj: x\nSubject To\n c: x <= 4\nEnd\n");
        assert_eq!(refused.id, Rejection::INSTANCE_LP_GRAMMAR);
        assert_eq!(refused.line, Some(1));
    }

    #[test]
    fn a_comment_removes_only_what_follows_it() {
        let built = model(
            "\\ a whole line\nMinimize\n obj: x \\ and a tail\nSubject To\n c: x <= 4\nEnd\n",
        );
        assert_eq!(built.variables().len(), 1);
        assert_eq!(built.variables()[0].objective.to_canonical_string(), "1");
    }

    #[test]
    fn content_after_the_terminator_is_not_read_and_is_still_hashed() {
        let plain = model("Minimize\n obj: x\nSubject To\n c: x <= 4\nEnd\n");
        let trailing = model("Minimize\n obj: x\nSubject To\n c: x <= 4\nEnd\nwhatever\n");
        let below = |value: &Model| {
            value
                .canonical_dump()
                .lines()
                .skip(3)
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert_eq!(below(&plain), below(&trailing));
        assert_ne!(plain.digest(), trailing.digest(), "the bytes differ");
    }

    #[test]
    fn a_free_variable_loses_both_of_its_sides() {
        let built = model("Minimize\n obj: x\nSubject To\n c: x <= 4\nBounds\n x free\nEnd\n");
        assert_eq!(built.variables()[0].lower, None);
        assert_eq!(built.variables()[0].upper, None);
    }

    #[test]
    fn an_infinity_belongs_to_the_side_its_sign_names() {
        let built = model(
            "Minimize\n obj: x\nSubject To\n c: x <= 4\nBounds\n -inf <= x <= +INFINITY\nEnd\n",
        );
        assert_eq!(built.variables()[0].lower, None);
        assert_eq!(built.variables()[0].upper, None);

        for text in [
            "Minimize\n obj: x\nSubject To\n c: x <= 4\nBounds\n x >= +inf\nEnd\n",
            "Minimize\n obj: x\nSubject To\n c: x <= 4\nBounds\n x <= -inf\nEnd\n",
            "Minimize\n obj: x\nSubject To\n c: x <= 4\nBounds\n x = +inf\nEnd\n",
        ] {
            assert_eq!(refusal(text).id, Rejection::INSTANCE_LP_GRAMMAR, "{text:?}");
        }
        // And an infinity where a number belongs is a malformed number, not a
        // bound word, so there is one numeric grammar and no local exception.
        let refused = refusal("Minimize\n obj: x\nSubject To\n c: x <= inf\nEnd\n");
        assert_eq!(refused.id, Rejection::INSTANCE_LP_GRAMMAR);
    }

    #[test]
    fn terms_naming_one_variable_sum_and_a_cancellation_is_recorded() {
        let built = model("Minimize\n obj: x + x\nSubject To\n c: 3 x - 3 x + y <= 4\nEnd\n");
        assert_eq!(built.variables()[0].objective.to_canonical_string(), "2");
        assert_eq!(built.matrix().nonzeros(), 1, "only y survives");
        assert_eq!(built.zero_sums().len(), 1, "the cancellation is recorded");
    }

    #[test]
    fn a_fixed_variable_sets_both_sides_and_a_second_statement_is_a_repeat() {
        let built = model("Minimize\n obj: x\nSubject To\n c: x <= 4\nBounds\n x = 3\nEnd\n");
        assert_eq!(shown(built.variables()[0].lower.as_ref()), "3");
        assert_eq!(shown(built.variables()[0].upper.as_ref()), "3");
        let refused =
            refusal("Minimize\n obj: x\nSubject To\n c: x <= 4\nBounds\n x = 3\n x >= 1\nEnd\n");
        assert_eq!(refused.id, Rejection::INSTANCE_DUPLICATE_BOUND);
    }

    #[test]
    fn the_required_sections_are_required() {
        let refused = refusal("Minimize\n obj: x\nEnd\n");
        assert_eq!(refused.id, Rejection::INSTANCE_SECTION_MISSING);
        let refused = refusal("Subject To\n c: x <= 4\nEnd\n");
        assert_eq!(refused.id, Rejection::INSTANCE_SECTION_MISSING);
    }

    #[test]
    fn a_variable_may_be_listed_integer_only_once() {
        let refused =
            refusal("Minimize\n obj: x\nSubject To\n c: x <= 4\nGeneral\n x\nBinary\n x\nEnd\n");
        assert_eq!(refused.id, Rejection::INSTANCE_DUPLICATE_COLUMN);
    }
}
