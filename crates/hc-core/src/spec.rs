//! A reader for the normative assets under `spec/`.
//!
//! The assets are valid TOML restricted to a subset small enough that a
//! dependency-free reader can accept it in a few dozen lines. That constraint
//! is not stylistic. The verifier may take no dependencies at all, and a
//! specification the verifier cannot read is a specification the verifier does
//! not have — so the subset is bounded by what is reasonable to write twice,
//! independently, on both sides of the proof boundary.
//!
//! This is the solver side of that pair. It is strict on purpose: anything
//! outside the subset is a malformed asset and is reported as one rather than
//! guessed at. An asset this reader accepts and the verifier's reader rejects
//! is a defect in one of them, and both being permissive is how that defect
//! stays hidden.
//!
//! Tables and keys keep their file order. Nothing here uses an unordered
//! container, so two runs over the same bytes visit the same entries in the
//! same sequence, and a vector harness that iterates a document reports its
//! failures in the order a reader would find them.

use core::fmt;

/// A parsed asset: tables in file order, each holding keys in file order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpecDocument {
    tables: Vec<SpecTable>,
}

/// One `[table]` and its assignments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpecTable {
    name: String,
    entries: Vec<(String, SpecValue)>,
}

/// The four value shapes the subset admits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpecValue {
    /// A single-quoted literal string.
    Text(String),
    /// A decimal integer, optionally negative.
    Integer(i64),
    /// `true` or `false`.
    Boolean(bool),
    /// A single-line array of literal strings.
    List(Vec<String>),
}

impl SpecValue {
    /// The value as text, when it is text.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            _ => None,
        }
    }

    /// The value as an integer, when it is one.
    #[must_use]
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            Self::Integer(value) => Some(*value),
            _ => None,
        }
    }

    /// The value as a boolean, when it is one.
    #[must_use]
    pub fn as_boolean(&self) -> Option<bool> {
        match self {
            Self::Boolean(value) => Some(*value),
            _ => None,
        }
    }

    /// The value as a list, when it is one.
    #[must_use]
    pub fn as_list(&self) -> Option<&[String]> {
        match self {
            Self::List(items) => Some(items),
            _ => None,
        }
    }
}

impl SpecTable {
    /// The table's header name, including any dots.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The table's assignments in file order.
    #[must_use]
    pub fn entries(&self) -> &[(String, SpecValue)] {
        &self.entries
    }

    /// The value for a key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&SpecValue> {
        self.entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }

    /// The text value for a key.
    #[must_use]
    pub fn text(&self, key: &str) -> Option<&str> {
        self.get(key)?.as_text()
    }

    /// The integer value for a key.
    #[must_use]
    pub fn integer(&self, key: &str) -> Option<i64> {
        self.get(key)?.as_integer()
    }
}

impl SpecDocument {
    /// The tables in file order.
    #[must_use]
    pub fn tables(&self) -> &[SpecTable] {
        &self.tables
    }

    /// The table with an exact name.
    #[must_use]
    pub fn table(&self, name: &str) -> Option<&SpecTable> {
        self.tables.iter().find(|table| table.name == name)
    }

    /// Every table whose name begins with `prefix` followed by a dot.
    ///
    /// The assets group related entries by a dotted prefix — every rejection
    /// identifier under `reject.`, every vector under `biguint.` — so this is
    /// the shape almost every consumer wants.
    pub fn tables_under<'a>(&'a self, prefix: &'a str) -> impl Iterator<Item = &'a SpecTable> {
        self.tables.iter().filter(move |table| {
            table.name.len() > prefix.len() + 1
                && table.name.starts_with(prefix)
                && table.name.as_bytes()[prefix.len()] == b'.'
        })
    }

    /// Reads an asset.
    ///
    /// # Errors
    ///
    /// Returns [`SpecError`] naming the line and the reason, for anything
    /// outside the accepted subset.
    pub fn parse(text: &str) -> Result<Self, SpecError> {
        let mut tables: Vec<SpecTable> = Vec::new();

        for (index, raw) in text.split('\n').enumerate() {
            let line_number = index + 1;
            let fail = |reason: SpecReason| SpecError {
                line: line_number,
                reason,
            };

            if raw.ends_with('\r') {
                return Err(fail(SpecReason::CarriageReturn));
            }
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some(rest) = line.strip_prefix('[') {
                if line.starts_with("[[") {
                    return Err(fail(SpecReason::ArrayOfTables));
                }
                let Some(name) = rest.strip_suffix(']') else {
                    return Err(fail(SpecReason::MalformedTableHeader));
                };
                if name.is_empty()
                    || !name.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-')
                    })
                {
                    return Err(fail(SpecReason::MalformedTableName));
                }
                if tables.iter().any(|table| table.name == name) {
                    return Err(fail(SpecReason::DuplicateTable));
                }
                tables.push(SpecTable {
                    name: name.to_owned(),
                    entries: Vec::new(),
                });
                continue;
            }

            let Some(table) = tables.last_mut() else {
                return Err(fail(SpecReason::AssignmentBeforeTable));
            };
            let Some(split) = line.find('=') else {
                return Err(fail(SpecReason::NotAnAssignment));
            };
            let key = line[..split].trim();
            let value = line[split + 1..].trim();
            if key.is_empty()
                || !key
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
            {
                return Err(fail(SpecReason::MalformedKey));
            }
            if table.entries.iter().any(|(name, _)| name == key) {
                return Err(fail(SpecReason::DuplicateKey));
            }
            let parsed = parse_value(value).map_err(fail)?;
            table.entries.push((key.to_owned(), parsed));
        }

        Ok(Self { tables })
    }
}

/// Reads one value in the accepted subset.
fn parse_value(value: &str) -> Result<SpecValue, SpecReason> {
    if value == "true" {
        return Ok(SpecValue::Boolean(true));
    }
    if value == "false" {
        return Ok(SpecValue::Boolean(false));
    }
    if let Some(rest) = value.strip_prefix('\'') {
        let Some(inner) = rest.strip_suffix('\'') else {
            return Err(SpecReason::UnterminatedText);
        };
        if inner.contains('\'') {
            return Err(SpecReason::QuoteInsideText);
        }
        return Ok(SpecValue::Text(inner.to_owned()));
    }
    if let Some(rest) = value.strip_prefix('[') {
        let Some(inner) = rest.strip_suffix(']') else {
            return Err(SpecReason::UnterminatedList);
        };
        return parse_list(inner.trim());
    }
    if value.starts_with('"') {
        return Err(SpecReason::BasicString);
    }
    if value.starts_with('{') {
        return Err(SpecReason::InlineTable);
    }
    let digits = value.strip_prefix('-').unwrap_or(value);
    if !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return value
            .parse()
            .map(SpecValue::Integer)
            .map_err(|_| SpecReason::IntegerOutOfRange);
    }
    Err(SpecReason::OutsideSubset)
}

/// Reads the interior of a single-line array of literal strings.
fn parse_list(inner: &str) -> Result<SpecValue, SpecReason> {
    let mut items = Vec::new();
    let mut rest = inner;
    while !rest.is_empty() {
        let Some(after_quote) = rest.strip_prefix('\'') else {
            return Err(SpecReason::ListElementNotText);
        };
        let Some(end) = after_quote.find('\'') else {
            return Err(SpecReason::UnterminatedText);
        };
        items.push(after_quote[..end].to_owned());
        rest = after_quote[end + 1..].trim_start();
        if let Some(next) = rest.strip_prefix(',') {
            rest = next.trim_start();
        } else if !rest.is_empty() {
            return Err(SpecReason::ListElementNotText);
        }
    }
    Ok(SpecValue::List(items))
}

/// A malformed asset, with the line that stopped the reader.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpecError {
    /// The one-based line number.
    pub line: usize,
    /// Why the line was refused.
    pub reason: SpecReason,
}

/// Why a line fell outside the accepted subset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecReason {
    /// The line ended with a carriage return; assets are line-feed only.
    CarriageReturn,
    /// An array of tables, which the subset excludes.
    ArrayOfTables,
    /// A table header that did not close.
    MalformedTableHeader,
    /// A table name outside the accepted character set.
    MalformedTableName,
    /// The same table header appeared twice.
    DuplicateTable,
    /// An assignment preceded every table header.
    AssignmentBeforeTable,
    /// A line that is neither comment, header, nor assignment.
    NotAnAssignment,
    /// A key outside the accepted character set.
    MalformedKey,
    /// The same key appeared twice in one table.
    DuplicateKey,
    /// A literal string that did not close.
    UnterminatedText,
    /// A literal string containing a quote, which has no escape here.
    QuoteInsideText,
    /// An array that did not close on its own line.
    UnterminatedList,
    /// An array element that was not a literal string.
    ListElementNotText,
    /// A double-quoted string, which the subset excludes.
    BasicString,
    /// An inline table, which the subset excludes.
    InlineTable,
    /// An integer that did not fit in a machine word.
    IntegerOutOfRange,
    /// A value shape the subset does not admit.
    OutsideSubset,
}

impl fmt::Display for SpecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "line {}: {}", self.line, self.reason)
    }
}

impl fmt::Display for SpecReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::CarriageReturn => "carriage return; assets are line-feed only",
            Self::ArrayOfTables => "arrays of tables are outside the subset",
            Self::MalformedTableHeader => "malformed table header",
            Self::MalformedTableName => "malformed table name",
            Self::DuplicateTable => "duplicate table",
            Self::AssignmentBeforeTable => "assignment before any table header",
            Self::NotAnAssignment => "line is neither a comment, a table header, nor an assignment",
            Self::MalformedKey => "malformed key",
            Self::DuplicateKey => "duplicate key",
            Self::UnterminatedText => "unterminated literal string",
            Self::QuoteInsideText => "literal string contains a quote",
            Self::UnterminatedList => "array must close on the same line",
            Self::ListElementNotText => "array elements must be literal strings",
            Self::BasicString => "basic strings are outside the subset; use a literal string",
            Self::InlineTable => "inline tables are outside the subset",
            Self::IntegerOutOfRange => "integer does not fit in a machine word",
            Self::OutsideSubset => "value is outside the subset",
        };
        formatter.write_str(text)
    }
}

impl core::error::Error for SpecError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_small_document_keeps_file_order() {
        let document = SpecDocument::parse(
            "# a comment\n[meta]\nversion = 1\ntitle = 'Vectors'\n\n[case.one]\nop = 'add'\nflag = true\nlist = ['a', 'b']\n",
        )
        .expect("inside the subset");
        let names: Vec<&str> = document.tables().iter().map(SpecTable::name).collect();
        assert_eq!(names, vec!["meta", "case.one"]);
        assert_eq!(
            document.table("meta").and_then(|t| t.integer("version")),
            Some(1)
        );
        assert_eq!(
            document.table("meta").and_then(|t| t.text("title")),
            Some("Vectors")
        );
        let case = document.table("case.one").expect("present");
        assert_eq!(case.get("flag").and_then(SpecValue::as_boolean), Some(true));
        assert_eq!(
            case.get("list").and_then(SpecValue::as_list),
            Some(["a".to_owned(), "b".to_owned()].as_slice())
        );
        let keys: Vec<&str> = case.entries().iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(keys, vec!["op", "flag", "list"]);
    }

    #[test]
    fn a_dotted_prefix_selects_its_group_and_not_the_parent() {
        let document = SpecDocument::parse(
            "[reject]\nnote = 'x'\n[reject.one]\nid = 'A'\n[rejected.two]\nid = 'B'\n",
        )
        .expect("inside the subset");
        let found: Vec<&str> = document
            .tables_under("reject")
            .map(SpecTable::name)
            .collect();
        assert_eq!(found, vec!["reject.one"]);
    }

    #[test]
    fn a_negative_integer_and_an_empty_list_are_admitted() {
        let document =
            SpecDocument::parse("[t]\nn = -42\nempty = []\n").expect("inside the subset");
        let table = document.table("t").expect("present");
        assert_eq!(table.integer("n"), Some(-42));
        assert_eq!(
            table.get("empty").and_then(SpecValue::as_list),
            Some([].as_slice())
        );
    }

    #[test]
    fn a_hash_inside_a_literal_string_is_part_of_the_value() {
        let document = SpecDocument::parse("[t]\nk = 'a # b'\n").expect("inside the subset");
        assert_eq!(document.table("t").and_then(|t| t.text("k")), Some("a # b"));
    }

    #[test]
    fn everything_outside_the_subset_is_refused_by_name() {
        for (text, reason) in [
            ("[t]\nk = \"basic\"\n", SpecReason::BasicString),
            ("[t]\nk = { a = 1 }\n", SpecReason::InlineTable),
            ("[[t]]\n", SpecReason::ArrayOfTables),
            ("[t\n", SpecReason::MalformedTableHeader),
            ("[t ]\n", SpecReason::MalformedTableName),
            ("[t]\n[t]\n", SpecReason::DuplicateTable),
            ("k = 1\n", SpecReason::AssignmentBeforeTable),
            ("[t]\nnonsense\n", SpecReason::NotAnAssignment),
            ("[t]\nk k = 1\n", SpecReason::MalformedKey),
            ("[t]\nk = 1\nk = 2\n", SpecReason::DuplicateKey),
            ("[t]\nk = 'open\n", SpecReason::UnterminatedText),
            ("[t]\nk = ['a'\n", SpecReason::UnterminatedList),
            ("[t]\nk = [1, 2]\n", SpecReason::ListElementNotText),
            ("[t]\nk = 1.5\n", SpecReason::OutsideSubset),
            ("[t]\nk = 1\r\n", SpecReason::CarriageReturn),
            (
                "[t]\nk = 99999999999999999999\n",
                SpecReason::IntegerOutOfRange,
            ),
        ] {
            let error = SpecDocument::parse(text).expect_err("outside the subset");
            assert_eq!(error.reason, reason, "for {text:?}");
        }
    }

    #[test]
    fn a_trailing_comment_is_not_a_comment() {
        // The subset has no trailing comments, so a hash after a closed
        // literal string is not the start of one: the value simply does not
        // end where it must, and the asset is refused. A reader that silently
        // trimmed the tail would accept an asset the other reader rejects,
        // which is the one outcome the two-reader design exists to prevent.
        let error = SpecDocument::parse("[t]\nk = 'v' # note\n").expect_err("outside the subset");
        assert_eq!(error.reason, SpecReason::UnterminatedText);
        // A hash inside the string is ordinary content, checked above.
    }

    #[test]
    fn an_error_reports_the_line_it_stopped_on() {
        let error = SpecDocument::parse("[t]\na = 1\nb = 1.5\n").expect_err("outside the subset");
        assert_eq!(error.line, 3);
        assert!(error.to_string().starts_with("line 3:"));
    }
}
