//! Lines into tokens, and the words that are never names.
//!
//! The tokenizer is deliberately tiny and total: every byte of an accepted
//! line belongs to exactly one token, and a byte that begins no token refuses
//! the file rather than being skipped. Skipping an unrecognised character is
//! how a reader silently drops a term.

use hc_core::{Refusal, Rejection};

/// A relational operator, after the one-character spellings are folded in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relation {
    /// `<=`, or the bare `<`.
    AtMost,
    /// `>=`, or the bare `>`.
    AtLeast,
    /// `=`.
    Equal,
}

impl Relation {
    /// The spelling this operator is written back out as.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AtMost => "<=",
            Self::AtLeast => ">=",
            Self::Equal => "=",
        }
    }
}

/// One lexical item of a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token<'a> {
    /// A name, which may or may not turn out to be a reserved word.
    Name(&'a str),
    /// A numeric token, exactly as written. It is not interpreted here.
    Number(&'a str),
    /// The colon that closes a label.
    Colon,
    /// A leading sign: true for plus.
    Sign(bool),
    /// A relational operator.
    Relation(Relation),
}

/// The words that are never variable or constraint labels.
///
/// Every word of every section keyword is here, not only the keywords
/// themselves, so that a two-word keyword cannot be assembled out of names.
/// The comparison is on the upper-cased word, because keywords are matched
/// without regard to case and names are matched exactly.
const RESERVED: &[&str] = &[
    "MINIMIZE", "MINIMISE", "MIN", "MINIMUM", "MAXIMIZE", "MAXIMISE", "MAX", "MAXIMUM", "SUBJECT",
    "SUCH", "THAT", "TO", "ST", "ST.", "S.T.", "BOUNDS", "BOUND", "GENERAL", "GENERALS", "GEN",
    "BINARY", "BINARIES", "BIN", "END", "FREE", "INF", "INFINITY",
];

/// Whether a word is reserved and therefore never a name.
#[must_use]
pub fn is_reserved(word: &str) -> bool {
    let upper = word.to_ascii_uppercase();
    RESERVED.contains(&upper.as_str())
}

/// Whether a word is one of the two infinity spellings.
#[must_use]
pub fn is_infinity(word: &str) -> bool {
    let upper = word.to_ascii_uppercase();
    upper == "INF" || upper == "INFINITY"
}

/// Whether a word is the one that removes both sides of a bound.
#[must_use]
pub fn is_free(word: &str) -> bool {
    word.eq_ignore_ascii_case("FREE")
}

/// Removes a comment and the surrounding blanks from one physical line.
#[must_use]
pub fn strip(line: &str) -> &str {
    match line.find('\\') {
        Some(at) => line[..at].trim(),
        None => line.trim(),
    }
}

/// Collapses a line to the form section keywords are matched against.
#[must_use]
pub fn keyword_shape(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    for word in line.split_ascii_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&word.to_ascii_uppercase());
    }
    out
}

/// Whether a character may begin a name.
const fn name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

/// Whether a character may continue a name.
const fn name_rest(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.'
}

/// Splits one prepared line into tokens.
///
/// # Errors
///
/// Returns a refusal naming the grammar when a byte begins no token.
pub fn tokens(line: &str, at: usize) -> Result<Vec<Token<'_>>, Refusal> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b' ' | b'\t' => index += 1,
            b':' => {
                out.push(Token::Colon);
                index += 1;
            }
            b'+' => {
                out.push(Token::Sign(true));
                index += 1;
            }
            b'-' => {
                out.push(Token::Sign(false));
                index += 1;
            }
            b'<' | b'>' => {
                let relation = if byte == b'<' {
                    Relation::AtMost
                } else {
                    Relation::AtLeast
                };
                index += 1;
                if bytes.get(index) == Some(&b'=') {
                    index += 1;
                }
                out.push(Token::Relation(relation));
            }
            b'=' => {
                out.push(Token::Relation(Relation::Equal));
                index += 1;
            }
            _ if name_start(byte) => {
                let start = index;
                while index < bytes.len() && name_rest(bytes[index]) {
                    index += 1;
                }
                out.push(Token::Name(&line[start..index]));
            }
            _ if byte.is_ascii_digit() || byte == b'.' => {
                let start = index;
                index = number_end(bytes, index);
                out.push(Token::Number(&line[start..index]));
            }
            _ => {
                return Err(Refusal::new(
                    Rejection::INSTANCE_LP_GRAMMAR,
                    format!("character {:?} begins no token", byte as char),
                )
                .at(at));
            }
        }
    }
    Ok(out)
}

/// Finds the end of a numeric token.
///
/// The exponent is taken greedily when an `e` is followed by digits, with or
/// without a sign. That is the reading every LP tool uses and it is the reason
/// the accepted subset says a coefficient and a name may be adjacent: a number
/// ends on a digit or a full stop, and a name may begin with neither. A file
/// that means a variable called `e1` after a bare `2` writes the blank.
fn number_end(bytes: &[u8], from: usize) -> usize {
    let mut index = from;
    while index < bytes.len() && (bytes[index].is_ascii_digit() || bytes[index] == b'.') {
        index += 1;
    }
    if index < bytes.len() && (bytes[index] == b'e' || bytes[index] == b'E') {
        let mut lookahead = index + 1;
        if lookahead < bytes.len() && (bytes[lookahead] == b'+' || bytes[lookahead] == b'-') {
            lookahead += 1;
        }
        if lookahead < bytes.len() && bytes[lookahead].is_ascii_digit() {
            index = lookahead;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
        }
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_comment_runs_to_the_end_of_its_line() {
        assert_eq!(strip("  c1: x + y <= 4  \\ the budget "), "c1: x + y <= 4");
        assert_eq!(strip("\\ whole line"), "");
        assert_eq!(strip("  "), "");
    }

    #[test]
    fn a_two_word_keyword_is_one_keyword_however_it_is_spaced() {
        assert_eq!(keyword_shape("subject    to"), "SUBJECT TO");
        assert_eq!(keyword_shape("Bounds"), "BOUNDS");
    }

    #[test]
    fn a_term_splits_the_same_way_whether_or_not_it_carries_a_blank() {
        let spaced = tokens("3 x", 1).expect("tokens");
        let tight = tokens("3x", 1).expect("tokens");
        assert_eq!(spaced, tight);
        assert_eq!(spaced, vec![Token::Number("3"), Token::Name("x")]);
    }

    #[test]
    fn an_exponent_is_part_of_the_number_it_follows() {
        assert_eq!(
            tokens("1.5e2 y", 1).expect("tokens"),
            vec![Token::Number("1.5e2"), Token::Name("y")]
        );
        // An `e` with no digits after it is not an exponent, so the name
        // beginning there is a name.
        assert_eq!(
            tokens("2 e", 1).expect("tokens"),
            vec![Token::Number("2"), Token::Name("e")]
        );
    }

    #[test]
    fn the_one_character_operators_fold_into_their_two_character_forms() {
        assert_eq!(
            tokens("a < 1", 1).expect("tokens"),
            tokens("a <= 1", 1).expect("tokens")
        );
        assert_eq!(
            tokens("a > 1", 1).expect("tokens"),
            tokens("a >= 1", 1).expect("tokens")
        );
    }

    #[test]
    fn the_reversed_operator_spellings_are_two_tokens_and_not_one() {
        // `=<` is a spelling this subset excludes. It has to arrive as two
        // relations so that the grammar refuses it rather than reading it.
        assert_eq!(
            tokens("a =< 1", 1).expect("tokens"),
            vec![
                Token::Name("a"),
                Token::Relation(Relation::Equal),
                Token::Relation(Relation::AtMost),
                Token::Number("1")
            ]
        );
    }

    #[test]
    fn a_character_that_begins_no_token_refuses_the_line() {
        let refusal = tokens("x ? y", 3).expect_err("no token");
        assert_eq!(refusal.id, Rejection::INSTANCE_LP_GRAMMAR);
        assert_eq!(refusal.line, Some(3));
        assert!(tokens("x , y", 1).is_err(), "a comma begins no token");
    }

    #[test]
    fn every_word_of_every_keyword_is_reserved() {
        for word in ["subject", "to", "Bounds", "GEN", "free", "inf", "End"] {
            assert!(is_reserved(word), "{word} should be reserved");
        }
        assert!(!is_reserved("x"), "an ordinary name is not reserved");
        assert!(!is_reserved("infinite"), "only the two spellings are");
        assert!(is_infinity("Infinity") && is_infinity("INF"));
        assert!(is_free("Free"));
    }
}
