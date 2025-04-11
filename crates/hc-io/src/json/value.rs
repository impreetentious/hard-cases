//! The restricted JSON reader.
//!
//! Objects, arrays, strings, integers, booleans, and null. There is no
//! floating-point literal in the accepted grammar, so this reader never needs
//! a decimal-to-binary conversion and can never silently perform one.
//!
//! The recursion is bounded before it is entered rather than after, and the
//! bound is a constant rather than a function of the input. A schema this
//! shallow needs no more, and a parser whose stack depth is chosen by the
//! document it is reading is a parser an input can end.

use hc_core::{Refusal, Rejection};

/// The deepest nesting the accepted grammar reaches, with room to spare.
///
/// The document is an object holding arrays holding objects holding values,
/// which is four. The limit is generous rather than tight because its job is
/// to bound the stack, not to enforce the schema; the schema does that itself
/// and reports a better reason when it does.
const MAX_DEPTH: usize = 8;

/// One JSON value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Json {
    /// The absence a nullable member spells.
    Null,
    /// A boolean.
    Bool(bool),
    /// An integer literal. Never a model value.
    Integer(i64),
    /// A string, with its escapes resolved.
    Text(String),
    /// An array.
    List(Vec<Json>),
    /// An object, in the order its members were written.
    Object(Vec<(String, Json)>),
}

impl Json {
    /// The member of an object with this name.
    #[must_use]
    pub fn member(&self, name: &str) -> Option<&Self> {
        match self {
            Self::Object(members) => members
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    /// The word a diagnostic uses for this kind of value.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "a boolean",
            Self::Integer(_) => "an integer",
            Self::Text(_) => "a string",
            Self::List(_) => "an array",
            Self::Object(_) => "an object",
        }
    }
}

/// Refuses a document with the schema identifier.
fn refuse(detail: impl Into<String>) -> Refusal {
    Refusal::new(Rejection::INSTANCE_JSON_SCHEMA, detail)
}

/// Reads one complete document.
///
/// # Errors
///
/// Returns a refusal naming the schema rule when the text is not accepted
/// JSON, or when content follows the value.
pub fn parse(text: &str) -> Result<Json, Refusal> {
    let mut reader = Reader {
        bytes: text.as_bytes(),
        at: 0,
    };
    reader.blanks();
    let value = reader.value(0)?;
    reader.blanks();
    if reader.at != reader.bytes.len() {
        return Err(refuse(format!(
            "content follows the document at byte {}",
            reader.at
        )));
    }
    Ok(value)
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn blanks(&mut self) {
        while let Some(byte) = self.peek() {
            if byte == b' ' || byte == b'\t' || byte == b'\n' || byte == b'\r' {
                self.at += 1;
            } else {
                break;
            }
        }
    }

    fn literal(&mut self, word: &str) -> bool {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            return true;
        }
        false
    }

    fn value(&mut self, depth: usize) -> Result<Json, Refusal> {
        if depth >= MAX_DEPTH {
            return Err(refuse(format!(
                "the document nests deeper than {MAX_DEPTH} levels"
            )));
        }
        match self.peek() {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.list(depth),
            Some(b'"') => self.text().map(Json::Text),
            Some(b't') if self.literal("true") => Ok(Json::Bool(true)),
            Some(b'f') if self.literal("false") => Ok(Json::Bool(false)),
            Some(b'n') if self.literal("null") => Ok(Json::Null),
            Some(byte) if byte == b'-' || byte.is_ascii_digit() => self.integer(),
            Some(byte) => Err(refuse(format!(
                "byte {:?} at {} begins no value",
                byte as char, self.at
            ))),
            None => Err(refuse("the document ends where a value was expected")),
        }
    }

    fn object(&mut self, depth: usize) -> Result<Json, Refusal> {
        self.at += 1;
        let mut members: Vec<(String, Json)> = Vec::new();
        self.blanks();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(Json::Object(members));
        }
        loop {
            self.blanks();
            if self.peek() != Some(b'"') {
                return Err(refuse(format!("a member name was expected at {}", self.at)));
            }
            let name = self.text()?;
            if members.iter().any(|(known, _)| *known == name) {
                return Err(refuse(format!(
                    "the member {name:?} appears more than once in one object"
                )));
            }
            self.blanks();
            if self.peek() != Some(b':') {
                return Err(refuse(format!("a colon was expected at {}", self.at)));
            }
            self.at += 1;
            self.blanks();
            let value = self.value(depth + 1)?;
            members.push((name, value));
            self.blanks();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(Json::Object(members));
                }
                _ => {
                    return Err(refuse(format!(
                        "a comma or a closing brace was expected at {}",
                        self.at
                    )));
                }
            }
        }
    }

    fn list(&mut self, depth: usize) -> Result<Json, Refusal> {
        self.at += 1;
        let mut items = Vec::new();
        self.blanks();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(Json::List(items));
        }
        loop {
            self.blanks();
            items.push(self.value(depth + 1)?);
            self.blanks();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Ok(Json::List(items));
                }
                _ => {
                    return Err(refuse(format!(
                        "a comma or a closing bracket was expected at {}",
                        self.at
                    )));
                }
            }
        }
    }

    fn text(&mut self) -> Result<String, Refusal> {
        self.at += 1;
        let mut out = String::new();
        loop {
            let Some(byte) = self.peek() else {
                return Err(refuse("the document ends inside a string"));
            };
            self.at += 1;
            match byte {
                b'"' => return Ok(out),
                b'\\' => {
                    let Some(escape) = self.peek() else {
                        return Err(refuse("the document ends inside an escape"));
                    };
                    self.at += 1;
                    let resolved = match escape {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        // Every accepted input is ASCII, so a unicode escape
                        // either repeats a character that could be written
                        // plainly or names one outside the accepted set.
                        _ => {
                            return Err(refuse(format!(
                                "the escape {:?} is outside the accepted set",
                                escape as char
                            )));
                        }
                    };
                    out.push(resolved);
                }
                0x20..=0x7e => out.push(byte as char),
                _ => {
                    return Err(refuse(format!(
                        "byte {byte:#04x} is not accepted inside a string"
                    )));
                }
            }
        }
    }

    fn integer(&mut self) -> Result<Json, Refusal> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        let digits = self.at;
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.at += 1;
        }
        if self.at == digits {
            return Err(refuse(format!("a number with no digits at {start}")));
        }
        if self.bytes[digits] == b'0' && self.at - digits > 1 {
            return Err(refuse(format!("a leading zero at {digits}")));
        }
        // A fractional part or an exponent would make this a floating-point
        // literal, and there are none in the accepted grammar. Refusing here
        // rather than reading and rejecting later is what keeps every model
        // value on the exact path.
        if self
            .peek()
            .is_some_and(|byte| byte == b'.' || byte == b'e' || byte == b'E')
        {
            return Err(refuse(
                "a floating-point literal; every model value is a string".to_owned(),
            ));
        }
        let text = core::str::from_utf8(&self.bytes[start..self.at])
            .map_err(|_| refuse("a number is not text"))?;
        text.parse::<i64>()
            .map(Json::Integer)
            .map_err(|_| refuse(format!("the integer {text} does not fit a machine word")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_accepted_kinds_read_the_way_they_are_written() {
        let value = parse(r#"{"a": 1, "b": [true, false, null], "c": "x\ty"}"#).expect("accepted");
        assert_eq!(value.member("a"), Some(&Json::Integer(1)));
        assert_eq!(
            value.member("b"),
            Some(&Json::List(vec![
                Json::Bool(true),
                Json::Bool(false),
                Json::Null
            ]))
        );
        assert_eq!(value.member("c"), Some(&Json::Text("x\ty".to_owned())));
    }

    #[test]
    fn trap_json_value_is_never_a_number() {
        // The rule the whole schema rests on, enforced by the grammar rather
        // than by a check somewhere later.
        for text in ["1.5", "{\"v\": 0.1}", "[1e3]", "-2.0"] {
            let refusal = parse(text).expect_err("a floating-point literal");
            assert_eq!(refusal.id, Rejection::INSTANCE_JSON_SCHEMA, "{text}");
        }
        // An integer is still an integer, because counts and indices are not
        // model values.
        assert_eq!(parse("-12").expect("accepted"), Json::Integer(-12));
    }

    #[test]
    fn the_shapes_a_forgiving_reader_would_repair_are_refused() {
        for text in [
            "{\"a\": 1,}",
            "[1, 2,]",
            "{\"a\": 1} trailing",
            "{a: 1}",
            "{\"a\" 1}",
            "{\"a\": 1, \"a\": 2}",
            "01",
            "{\"a\": \"\\u0041\"}",
            "",
        ] {
            assert!(parse(text).is_err(), "accepted {text:?}");
        }
    }

    #[test]
    fn the_empty_containers_are_ordinary() {
        assert_eq!(parse("{}").expect("accepted"), Json::Object(Vec::new()));
        assert_eq!(parse(" [ ] ").expect("accepted"), Json::List(Vec::new()));
    }

    #[test]
    fn nesting_is_bounded_before_it_is_entered() {
        let deep = "[".repeat(64) + &"]".repeat(64);
        let refusal = parse(&deep).expect_err("too deep");
        assert!(refusal.detail.contains("nests deeper"), "{refusal}");
    }
}
