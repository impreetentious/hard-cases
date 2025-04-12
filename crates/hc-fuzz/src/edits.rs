//! Mutations: what a file looks like just outside its grammar.
//!
//! Deliberately coarse. A mutation that produced only unreadable bytes would
//! test the first fence over and over, and one that produced only valid files
//! would test nothing at all. These sit on the boundary: a character replaced
//! by another character the grammar uses, a line dropped, a token widened past
//! a cap, a section header cut in half.

use hc_core::Xoshiro256PlusPlus as Rng;

/// The characters a replacement is drawn from.
///
/// All of them mean something in at least one of the three grammars, which is
/// what makes a replacement likely to produce a file that parses differently
/// rather than one that does not parse at all.
const ALPHABET: &[u8] = b" \t\n:,-+<>=\\.0139eE\"[]{}xXrNGLEBIFP*'";

/// How a case was made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    /// The rendering, untouched.
    None,
    /// One byte replaced by another the grammars use.
    Byte,
    /// One line removed.
    DropLine,
    /// One line repeated.
    RepeatLine,
    /// Two lines exchanged.
    SwapLines,
    /// Everything after a point removed.
    Truncate,
    /// A digit run extended until it is over a cap.
    WidenToken,
    /// A name extended until it is over its cap.
    WidenName,
    /// A run of bytes repeated in place.
    Repeat,
}

impl Edit {
    /// Every mutation, so a campaign can walk them rather than sample them.
    pub const ALL: [Self; 9] = [
        Self::None,
        Self::Byte,
        Self::DropLine,
        Self::RepeatLine,
        Self::SwapLines,
        Self::Truncate,
        Self::WidenToken,
        Self::WidenName,
        Self::Repeat,
    ];

    /// The name a report uses.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "untouched",
            Self::Byte => "byte",
            Self::DropLine => "dropped line",
            Self::RepeatLine => "repeated line",
            Self::SwapLines => "swapped lines",
            Self::Truncate => "truncated",
            Self::WidenToken => "widened token",
            Self::WidenName => "widened name",
            Self::Repeat => "repeated run",
        }
    }
}

/// Applies one mutation to a rendering.
#[must_use]
pub fn apply(text: &str, edit: Edit, seed: u64) -> String {
    let mut rng = Rng::from_seed(seed);
    if text.is_empty() {
        return text.to_owned();
    }
    match edit {
        Edit::None => text.to_owned(),
        Edit::Byte => {
            let mut bytes = text.as_bytes().to_vec();
            let at = rng.next_below(bytes.len() as u64) as usize;
            bytes[at] = ALPHABET[rng.next_below(ALPHABET.len() as u64) as usize];
            String::from_utf8_lossy(&bytes).into_owned()
        }
        Edit::DropLine => {
            let lines: Vec<&str> = text.lines().collect();
            let at = rng.next_below(lines.len() as u64) as usize;
            join(
                lines
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| *index != at)
                    .map(|(_, line)| *line),
            )
        }
        Edit::RepeatLine => {
            let lines: Vec<&str> = text.lines().collect();
            let at = rng.next_below(lines.len() as u64) as usize;
            let mut out: Vec<&str> = lines.clone();
            out.insert(at, lines[at]);
            join(out.into_iter())
        }
        Edit::SwapLines => {
            let mut lines: Vec<&str> = text.lines().collect();
            if lines.len() < 2 {
                return text.to_owned();
            }
            let first = rng.next_below(lines.len() as u64) as usize;
            let second = rng.next_below(lines.len() as u64) as usize;
            lines.swap(first, second);
            join(lines.into_iter())
        }
        Edit::Truncate => {
            let at = rng.next_below(text.len() as u64) as usize;
            text.get(..at).unwrap_or_default().to_owned()
        }
        Edit::WidenToken => widen(text, &mut rng, |byte| byte.is_ascii_digit(), '9', 5000),
        Edit::WidenName => widen(text, &mut rng, u8::is_ascii_alphabetic, 'q', 300),
        Edit::Repeat => {
            let at = rng.next_below(text.len() as u64) as usize;
            let run = 1 + rng.next_below(8) as usize;
            let end = (at + run).min(text.len());
            let Some(piece) = text.get(at..end) else {
                return text.to_owned();
            };
            let mut out = String::with_capacity(text.len() + piece.len());
            out.push_str(text.get(..end).unwrap_or_default());
            out.push_str(piece);
            out.push_str(text.get(end..).unwrap_or_default());
            out
        }
    }
}

/// Extends the first run of a kind of character past a cap.
///
/// The point is the caps: a token or a name that is one character over its
/// limit has to be refused by the check that owns that limit, on both sides,
/// before anything is allocated for it.
fn widen(text: &str, rng: &mut Rng, wanted: fn(&u8) -> bool, filler: char, extra: usize) -> String {
    let bytes = text.as_bytes();
    let start = rng.next_below(bytes.len() as u64) as usize;
    let Some(at) = (start..bytes.len()).find(|index| wanted(&bytes[*index])) else {
        return text.to_owned();
    };
    let mut out = String::with_capacity(text.len() + extra);
    out.push_str(text.get(..=at).unwrap_or_default());
    for _ in 0..extra {
        out.push(filler);
    }
    out.push_str(text.get(at + 1..).unwrap_or_default());
    out
}

/// Rejoins lines, keeping the trailing line feed every accepted file has.
fn join<'a>(lines: impl Iterator<Item = &'a str>) -> String {
    let mut out = String::new();
    for line in lines {
        out.push_str(line);
        out.push('\n');
    }
    out
}
