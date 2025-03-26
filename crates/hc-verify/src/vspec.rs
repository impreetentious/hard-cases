//! The verifier's reader for the normative assets.
//!
//! This is the second Rust implementation of the same restricted format, and
//! the reason there are two is the reason there are two of everything here: a
//! specification the verifier can only read through the solver's parser is a
//! specification the verifier does not have. The subset was chosen to be small
//! enough that writing it twice is cheap, and writing it twice is what makes
//! the choice mean anything.
//!
//! The shape is deliberately different. The solver's reader builds nested
//! tables; this one produces a flat list of table-key-value entries in file
//! order and answers questions by scanning it. Flat is slower and it removes a
//! whole category of disagreement — where a nested reader has to decide what a
//! dotted header means, this one never forms an opinion, because the header is
//! just the entry's first field.

use core::fmt;

/// One assignment, tagged with the table it appeared under.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The enclosing table's header, including any dots.
    pub table: String,
    /// The assignment's key.
    pub key: String,
    /// The assigned value.
    pub value: Value,
}

/// The four value shapes the subset admits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// A single-quoted literal string.
    Text(String),
    /// A decimal integer.
    Number(i64),
    /// A boolean.
    Flag(bool),
    /// A single-line list of literal strings.
    Items(Vec<String>),
}

/// A parsed asset.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Asset {
    entries: Vec<Entry>,
    tables: Vec<String>,
}

impl Asset {
    /// Every assignment, in file order.
    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Every table header, in file order, each once.
    #[must_use]
    pub fn tables(&self) -> &[String] {
        &self.tables
    }

    /// The table headers beginning with a prefix followed by a dot.
    #[must_use]
    pub fn tables_under(&self, prefix: &str) -> Vec<&str> {
        self.tables
            .iter()
            .filter(|name| {
                name.len() > prefix.len() + 1
                    && name.starts_with(prefix)
                    && name.as_bytes()[prefix.len()] == b'.'
            })
            .map(String::as_str)
            .collect()
    }

    /// The value of one key in one table.
    #[must_use]
    pub fn value(&self, table: &str, key: &str) -> Option<&Value> {
        self.entries
            .iter()
            .find(|entry| entry.table == table && entry.key == key)
            .map(|entry| &entry.value)
    }

    /// The text of one key in one table.
    #[must_use]
    pub fn text(&self, table: &str, key: &str) -> Option<&str> {
        match self.value(table, key)? {
            Value::Text(text) => Some(text),
            _ => None,
        }
    }

    /// The integer of one key in one table.
    #[must_use]
    pub fn number(&self, table: &str, key: &str) -> Option<i64> {
        match self.value(table, key)? {
            Value::Number(value) => Some(*value),
            _ => None,
        }
    }

    /// The list of one key in one table.
    #[must_use]
    pub fn items(&self, table: &str, key: &str) -> Option<&[String]> {
        match self.value(table, key)? {
            Value::Items(items) => Some(items),
            _ => None,
        }
    }

    /// Reads an asset.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] with the line and the rule that refused it.
    pub fn read(source: &str) -> Result<Self, AssetError> {
        let mut entries: Vec<Entry> = Vec::new();
        let mut tables: Vec<String> = Vec::new();
        let mut current: Option<String> = None;

        for (offset, raw) in source.split('\n').enumerate() {
            let line_number = offset + 1;
            let refuse = |rule: Rule| AssetError {
                line: line_number,
                rule,
            };

            if raw.as_bytes().last() == Some(&b'\r') {
                return Err(refuse(Rule::CarriageReturn));
            }
            let line = raw.trim_matches(|c: char| c == ' ' || c == '\t');
            if line.is_empty() || line.as_bytes()[0] == b'#' {
                continue;
            }

            if line.as_bytes()[0] == b'[' {
                if line.len() > 1 && line.as_bytes()[1] == b'[' {
                    return Err(refuse(Rule::TableList));
                }
                if line.as_bytes()[line.len() - 1] != b']' {
                    return Err(refuse(Rule::BadHeader));
                }
                let name = &line[1..line.len() - 1];
                if name.is_empty() || !name.bytes().all(is_header_byte) {
                    return Err(refuse(Rule::BadHeaderName));
                }
                if tables.iter().any(|seen| seen == name) {
                    return Err(refuse(Rule::RepeatedTable));
                }
                tables.push(name.to_owned());
                current = Some(name.to_owned());
                continue;
            }

            let Some(table) = current.clone() else {
                return Err(refuse(Rule::NoTableYet));
            };
            let Some(at) = line.find('=') else {
                return Err(refuse(Rule::NotAnAssignment));
            };
            let key = line[..at].trim_matches(|c: char| c == ' ' || c == '\t');
            let raw_value = line[at + 1..].trim_matches(|c: char| c == ' ' || c == '\t');
            if key.is_empty() || !key.bytes().all(is_key_byte) {
                return Err(refuse(Rule::BadKey));
            }
            if entries
                .iter()
                .any(|entry| entry.table == table && entry.key == key)
            {
                return Err(refuse(Rule::RepeatedKey));
            }
            let value = read_value(raw_value).map_err(refuse)?;
            entries.push(Entry {
                table,
                key: key.to_owned(),
                value,
            });
        }

        Ok(Self { entries, tables })
    }
}

fn is_header_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.' || byte == b'-'
}

fn is_key_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'
}

/// Reads one value.
fn read_value(text: &str) -> Result<Value, Rule> {
    if text == "true" {
        return Ok(Value::Flag(true));
    }
    if text == "false" {
        return Ok(Value::Flag(false));
    }
    let bytes = text.as_bytes();
    match bytes.first() {
        Some(b'\'') => {
            if bytes.len() < 2 || bytes[bytes.len() - 1] != b'\'' {
                return Err(Rule::UnclosedText);
            }
            let inner = &text[1..text.len() - 1];
            if inner.as_bytes().contains(&b'\'') {
                return Err(Rule::QuoteInText);
            }
            Ok(Value::Text(inner.to_owned()))
        }
        Some(b'[') => {
            if bytes[bytes.len() - 1] != b']' {
                return Err(Rule::UnclosedList);
            }
            read_items(text[1..text.len() - 1].trim())
        }
        Some(b'"') => Err(Rule::QuotedString),
        Some(b'{') => Err(Rule::InlineTable),
        _ => read_number(text),
    }
}

/// Reads a single-line list of literal strings.
fn read_items(mut rest: &str) -> Result<Value, Rule> {
    let mut items = Vec::new();
    while !rest.is_empty() {
        if rest.as_bytes()[0] != b'\'' {
            return Err(Rule::ItemNotText);
        }
        let Some(close) = rest[1..].find('\'') else {
            return Err(Rule::UnclosedText);
        };
        items.push(rest[1..=close].to_owned());
        rest = rest[close + 2..].trim_start();
        if rest.as_bytes().first() == Some(&b',') {
            rest = rest[1..].trim_start();
        } else if !rest.is_empty() {
            return Err(Rule::ItemNotText);
        }
    }
    Ok(Value::Items(items))
}

/// Reads a decimal integer.
fn read_number(text: &str) -> Result<Value, Rule> {
    let digits = text.strip_prefix('-').unwrap_or(text);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Rule::OutsideSubset);
    }
    text.parse()
        .map(Value::Number)
        .map_err(|_| Rule::NumberTooLarge)
}

/// A refused asset line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssetError {
    /// The one-based line number.
    pub line: usize,
    /// The rule that refused it.
    pub rule: Rule,
}

/// Which rule refused a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
    /// A carriage return; assets are line-feed only.
    CarriageReturn,
    /// An array of tables.
    TableList,
    /// A table header that did not close.
    BadHeader,
    /// A table name outside the accepted characters.
    BadHeaderName,
    /// A table header seen twice.
    RepeatedTable,
    /// An assignment before any table header.
    NoTableYet,
    /// A line that is neither comment, header, nor assignment.
    NotAnAssignment,
    /// A key outside the accepted characters.
    BadKey,
    /// A key seen twice in one table.
    RepeatedKey,
    /// A literal string that did not close.
    UnclosedText,
    /// A literal string containing a quote.
    QuoteInText,
    /// A list that did not close on its line.
    UnclosedList,
    /// A list element that was not a literal string.
    ItemNotText,
    /// A double-quoted string.
    QuotedString,
    /// An inline table.
    InlineTable,
    /// An integer too large for a machine word.
    NumberTooLarge,
    /// A value shape the subset does not admit.
    OutsideSubset,
}

impl fmt::Display for AssetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "line {}: {}", self.line, self.rule)
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::CarriageReturn => "carriage return; assets are line-feed only",
            Self::TableList => "arrays of tables are outside the subset",
            Self::BadHeader => "malformed table header",
            Self::BadHeaderName => "malformed table name",
            Self::RepeatedTable => "duplicate table",
            Self::NoTableYet => "assignment before any table header",
            Self::NotAnAssignment => "line is neither a comment, a table header, nor an assignment",
            Self::BadKey => "malformed key",
            Self::RepeatedKey => "duplicate key",
            Self::UnclosedText => "unterminated literal string",
            Self::QuoteInText => "literal string contains a quote",
            Self::UnclosedList => "array must close on the same line",
            Self::ItemNotText => "array elements must be literal strings",
            Self::QuotedString => "basic strings are outside the subset",
            Self::InlineTable => "inline tables are outside the subset",
            Self::NumberTooLarge => "integer does not fit in a machine word",
            Self::OutsideSubset => "value is outside the subset",
        };
        formatter.write_str(text)
    }
}

impl core::error::Error for AssetError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_small_asset_keeps_its_order() {
        let asset = Asset::read(
            "# note\n[meta]\nversion = 1\ntitle = 'Vectors'\n\n[case.one]\nop = 'add'\nflag = true\nlist = ['a', 'b']\n",
        )
        .expect("inside the subset");
        assert_eq!(asset.tables(), &["meta".to_owned(), "case.one".to_owned()]);
        assert_eq!(asset.number("meta", "version"), Some(1));
        assert_eq!(asset.text("meta", "title"), Some("Vectors"));
        assert_eq!(asset.value("case.one", "flag"), Some(&Value::Flag(true)));
        assert_eq!(
            asset.items("case.one", "list"),
            Some(["a".to_owned(), "b".to_owned()].as_slice())
        );
        let keys: Vec<&str> = asset
            .entries()
            .iter()
            .filter(|entry| entry.table == "case.one")
            .map(|entry| entry.key.as_str())
            .collect();
        assert_eq!(keys, vec!["op", "flag", "list"]);
    }

    #[test]
    fn a_prefix_selects_its_group_and_not_the_parent() {
        let asset =
            Asset::read("[reject]\nnote = 'x'\n[reject.one]\nid = 'A'\n[rejected.two]\nid = 'B'\n")
                .expect("inside the subset");
        assert_eq!(asset.tables_under("reject"), vec!["reject.one"]);
    }

    #[test]
    fn negatives_empty_lists_and_hashes_inside_text_are_admitted() {
        let asset =
            Asset::read("[t]\nn = -42\nempty = []\nk = 'a # b'\n").expect("inside the subset");
        assert_eq!(asset.number("t", "n"), Some(-42));
        assert_eq!(asset.items("t", "empty"), Some([].as_slice()));
        assert_eq!(asset.text("t", "k"), Some("a # b"));
    }

    #[test]
    fn everything_outside_the_subset_is_refused_by_name() {
        for (text, rule) in [
            ("[t]\nk = \"basic\"\n", Rule::QuotedString),
            ("[t]\nk = { a = 1 }\n", Rule::InlineTable),
            ("[[t]]\n", Rule::TableList),
            ("[t\n", Rule::BadHeader),
            ("[t ]\n", Rule::BadHeaderName),
            ("[t]\n[t]\n", Rule::RepeatedTable),
            ("k = 1\n", Rule::NoTableYet),
            ("[t]\nnonsense\n", Rule::NotAnAssignment),
            ("[t]\nk k = 1\n", Rule::BadKey),
            ("[t]\nk = 1\nk = 2\n", Rule::RepeatedKey),
            ("[t]\nk = 'open\n", Rule::UnclosedText),
            ("[t]\nk = 'v' # note\n", Rule::UnclosedText),
            ("[t]\nk = ['a'\n", Rule::UnclosedList),
            ("[t]\nk = [1, 2]\n", Rule::ItemNotText),
            ("[t]\nk = 1.5\n", Rule::OutsideSubset),
            ("[t]\nk = 1\r\n", Rule::CarriageReturn),
            ("[t]\nk = 99999999999999999999\n", Rule::NumberTooLarge),
        ] {
            let error = Asset::read(text).expect_err("outside the subset");
            assert_eq!(error.rule, rule, "for {text:?}");
        }
    }

    #[test]
    fn an_error_reports_the_line_it_stopped_on() {
        let error = Asset::read("[t]\na = 1\nb = 1.5\n").expect_err("outside the subset");
        assert_eq!(error.line, 3);
        assert!(error.to_string().starts_with("line 3:"));
    }
}
