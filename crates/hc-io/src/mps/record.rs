//! Lines and fields.
//!
//! The two variants of the format differ here and nowhere else. In the fixed
//! form a field's extent is positional, which is why a name may contain a
//! space; in the free form fields are whitespace-delimited, which is why a name
//! may not. Everything above this module works in terms of the six roles a
//! record can carry and never has to know which variant produced them.
//!
//! Two refusals live here because they cannot live anywhere else. Content that
//! runs past a fixed-form window is refused rather than truncated, because
//! silently accepting an overlong name is how a file that means one thing in
//! one reader means another thing in the next. And a free-form record with
//! more fields than its type accepts is refused rather than trimmed, for the
//! same reason.

use hc_core::{Refusal, Rejection};

/// Which variant's rules apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    /// Fields are read from their column windows.
    Fixed,
    /// Fields are whitespace-delimited.
    Free,
}

/// The fixed-form column windows, one-based and inclusive.
const WINDOWS: [(usize, usize); 6] = [(2, 3), (5, 12), (15, 22), (25, 36), (40, 47), (50, 61)];

/// The columns that must be blank on a fixed-form record: the leading column,
/// the gaps between windows, and everything past the final window.
const SEPARATORS: [(usize, usize); 6] = [(1, 1), (4, 4), (13, 14), (23, 24), (37, 39), (48, 49)];

/// One line of an instance file, classified.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line<'a> {
    /// A blank line or a comment, carrying nothing.
    Ignorable,
    /// A section header, with whatever followed it on the same line.
    ///
    /// The tail exists for one construction: a free-form file may write the
    /// objective sense on the header line itself.
    Section {
        /// The header word.
        name: &'a str,
        /// The rest of the line, trimmed, empty when there was none.
        tail: &'a str,
    },
    /// A data record.
    Record(Record<'a>),
}

/// A data record, before its fields have been given roles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record<'a> {
    form: Form,
    /// The six windows for a fixed record, or the tokens for a free one.
    parts: Vec<&'a str>,
    line: usize,
}

/// Whether a record's first field is an indicator.
///
/// The sections differ: a row declaration and a bound record lead with a code,
/// and a coefficient, right-hand side, or range record does not. In the fixed
/// form this changes nothing, because the windows are where they are. In the
/// free form it shifts every field by one, which is exactly the kind of
/// difference that is invisible until a file with an unusual name shows up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// The record leads with an indicator code.
    Indicated,
    /// The record's first field is already a name.
    Plain,
}

/// A record's fields, in their roles.
///
/// A field that was absent is the empty string rather than an option, because
/// every consumer treats absent and blank identically and an option here would
/// be unwrapped at every call site.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Fields<'a> {
    /// The row sense, bound type, or marker code.
    pub indicator: &'a str,
    /// The first name: a row name, a column name, or a vector name.
    pub name1: &'a str,
    /// The second name.
    pub name2: &'a str,
    /// The first value.
    pub value1: &'a str,
    /// The third name, used by the optional second pair on one line.
    pub name3: &'a str,
    /// The second value.
    pub value2: &'a str,
}

impl<'a> Record<'a> {
    /// The one-based line the record came from.
    #[must_use]
    pub fn line(&self) -> usize {
        self.line
    }

    /// The variant the record was read under.
    #[must_use]
    pub fn form(&self) -> Form {
        self.form
    }

    /// The record's fields in their roles.
    ///
    /// # Errors
    ///
    /// Returns a refusal when a free-form record carries more fields than its
    /// shape accepts, or when a fixed-form record puts content in the
    /// indicator window of a section that has no indicator.
    pub fn fields(&self, shape: Shape) -> Result<Fields<'a>, Refusal> {
        match self.form {
            Form::Fixed => {
                let indicator = self.parts[0];
                if shape == Shape::Plain && !indicator.is_empty() {
                    return Err(Refusal::new(
                        Rejection::INSTANCE_FIELD_OVERRUN,
                        format!("this section takes no indicator, and one is present: {indicator}"),
                    )
                    .at(self.line));
                }
                Ok(Fields {
                    indicator,
                    name1: self.parts[1],
                    name2: self.parts[2],
                    value1: self.parts[3],
                    name3: self.parts[4],
                    value2: self.parts[5],
                })
            }
            Form::Free => {
                let accepted = match shape {
                    Shape::Indicated => 6,
                    Shape::Plain => 5,
                };
                if self.parts.len() > accepted {
                    return Err(Refusal::new(
                        Rejection::INSTANCE_TOO_MANY_FIELDS,
                        format!(
                            "record carries {} fields and its type accepts {accepted}",
                            self.parts.len()
                        ),
                    )
                    .at(self.line));
                }
                let at = |index: usize| self.parts.get(index).copied().unwrap_or_default();
                let offset = usize::from(shape == Shape::Indicated);
                Ok(Fields {
                    indicator: if offset == 1 { at(0) } else { "" },
                    name1: at(offset),
                    name2: at(offset + 1),
                    value1: at(offset + 2),
                    name3: at(offset + 3),
                    value2: at(offset + 4),
                })
            }
        }
    }

    /// How many fields the record actually carried.
    ///
    /// Only the free form can answer this meaningfully; a fixed record always
    /// has six windows, however many of them are blank.
    #[must_use]
    pub fn arity(&self) -> usize {
        match self.form {
            Form::Fixed => self.parts.iter().filter(|part| !part.is_empty()).count(),
            Form::Free => self.parts.len(),
        }
    }
}

/// Classifies one line.
///
/// # Errors
///
/// Returns a refusal when a fixed-form record has content outside its windows.
pub fn classify(form: Form, line: &str, number: usize) -> Result<Line<'_>, Refusal> {
    if line.is_empty() || line.bytes().all(|byte| byte == b' ' || byte == b'\t') {
        return Ok(Line::Ignorable);
    }
    if line.starts_with('*') {
        return Ok(Line::Ignorable);
    }

    // A line whose first character is not a blank opens a section. This is the
    // convention both variants share and it is the only thing that separates a
    // header from a record.
    let first = line.as_bytes()[0];
    if first != b' ' && first != b'\t' {
        let trimmed = line.trim_end();
        let (name, tail) = match trimmed.find([' ', '\t']) {
            Some(at) => (&trimmed[..at], trimmed[at..].trim()),
            None => (trimmed, ""),
        };
        return Ok(Line::Section { name, tail });
    }

    match form {
        Form::Free => {
            let parts: Vec<&str> = line.split_whitespace().collect();
            Ok(Line::Record(Record {
                form,
                parts,
                line: number,
            }))
        }
        Form::Fixed => {
            check_windows(line, number)?;
            let parts = WINDOWS
                .iter()
                .map(|&(from, to)| window(line, from, to))
                .collect();
            Ok(Line::Record(Record {
                form,
                parts,
                line: number,
            }))
        }
    }
}

/// The content of one column window, trimmed at both ends.
///
/// Trimmed at both ends and not in the middle: a fixed-form name's extent is
/// its window, so a name containing a space is a name containing a space.
fn window(line: &str, from: usize, to: usize) -> &str {
    let bytes = line.as_bytes();
    let start = (from - 1).min(bytes.len());
    let end = to.min(bytes.len());
    if start >= end {
        return "";
    }
    line[start..end].trim_matches(|character| character == ' ' || character == '\t')
}

/// Refuses a fixed-form record whose content escapes its windows.
fn check_windows(line: &str, number: usize) -> Result<(), Refusal> {
    for &(from, to) in &SEPARATORS {
        let gap = window(line, from, to);
        if !gap.is_empty() {
            return Err(Refusal::new(
                Rejection::INSTANCE_FIELD_OVERRUN,
                format!("content in columns {from} to {to}, which lie between fields: {gap:?}"),
            )
            .at(number));
        }
    }
    // Everything past the final window is refused too. Some readers treat the
    // tail as a comment; accepting it would mean a field that overran its
    // window by enough characters became legal again, which is the reverse of
    // the rule.
    let tail = window(line, 62, usize::MAX);
    if !tail.is_empty() {
        return Err(Refusal::new(
            Rejection::INSTANCE_FIELD_OVERRUN,
            format!("content past column 61: {tail:?}"),
        )
        .at(number));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed(line: &str) -> Line<'_> {
        classify(Form::Fixed, line, 1).expect("a well-formed fixed record")
    }

    fn free(line: &str) -> Line<'_> {
        classify(Form::Free, line, 1).expect("a well-formed free record")
    }

    fn record<'a, 'b>(line: &'b Line<'a>) -> &'b Record<'a> {
        match line {
            Line::Record(record) => record,
            other => panic!("expected a record, got {other:?}"),
        }
    }

    #[test]
    fn blank_lines_and_comments_carry_nothing() {
        assert_eq!(fixed(""), Line::Ignorable);
        assert_eq!(fixed("    "), Line::Ignorable);
        assert_eq!(fixed("* a comment"), Line::Ignorable);
        assert_eq!(free("*"), Line::Ignorable);
        // An asterisk in the first column is a comment even when the rest
        // would otherwise read as a section.
        assert_eq!(fixed("*ROWS"), Line::Ignorable);
    }

    #[test]
    fn a_line_starting_in_the_first_column_opens_a_section() {
        assert_eq!(
            fixed("ROWS"),
            Line::Section {
                name: "ROWS",
                tail: ""
            }
        );
        assert_eq!(
            fixed("ENDATA"),
            Line::Section {
                name: "ENDATA",
                tail: ""
            }
        );
        assert_eq!(
            fixed("NAME          TINY"),
            Line::Section {
                name: "NAME",
                tail: "TINY"
            }
        );
        // The free form may write the objective sense on the header line.
        assert_eq!(
            free("OBJSENSE MAX"),
            Line::Section {
                name: "OBJSENSE",
                tail: "MAX"
            }
        );
    }

    #[test]
    fn the_fixed_windows_land_where_the_format_says() {
        let line = fixed("    X         COST      -1.0");
        let fields = record(&line).fields(Shape::Plain).expect("a plain record");
        assert_eq!(fields.indicator, "");
        assert_eq!(fields.name1, "X");
        assert_eq!(fields.name2, "COST");
        assert_eq!(fields.value1, "-1.0");
        assert_eq!(fields.name3, "");
        assert_eq!(fields.value2, "");
    }

    #[test]
    fn the_fifth_and_sixth_windows_carry_a_second_pair() {
        let line = fixed("    X         OBJ       1.0            R1        1.0");
        let fields = record(&line).fields(Shape::Plain).expect("a plain record");
        assert_eq!(fields.name1, "X");
        assert_eq!(fields.name2, "OBJ");
        assert_eq!(fields.value1, "1.0");
        assert_eq!(fields.name3, "R1");
        assert_eq!(fields.value2, "1.0");
    }

    #[test]
    fn an_indicated_fixed_record_reads_its_code() {
        let line = fixed(" UI BND       X         10");
        let fields = record(&line)
            .fields(Shape::Indicated)
            .expect("an indicated record");
        assert_eq!(fields.indicator, "UI");
        assert_eq!(fields.name1, "BND");
        assert_eq!(fields.name2, "X");
        assert_eq!(fields.value1, "10");

        let sense = fixed(" N  COST");
        let fields = record(&sense)
            .fields(Shape::Indicated)
            .expect("an indicated record");
        assert_eq!(fields.indicator, "N");
        assert_eq!(fields.name1, "COST");
    }

    #[test]
    fn a_fixed_name_may_contain_a_space_because_its_extent_is_positional() {
        let line = fixed("    A B       COST      1.0");
        let fields = record(&line).fields(Shape::Plain).expect("a plain record");
        assert_eq!(fields.name1, "A B");
    }

    #[test]
    fn content_between_windows_is_refused_rather_than_absorbed() {
        for line in [
            "    X        Q COST      1.0",
            "    X         COST     Q-1.0",
            "   QX         COST      1.0",
        ] {
            let refusal = classify(Form::Fixed, line, 3).expect_err("content escapes a window");
            assert_eq!(
                refusal.id,
                Rejection::INSTANCE_FIELD_OVERRUN,
                "for {line:?}"
            );
            assert_eq!(refusal.line, Some(3));
        }
    }

    #[test]
    fn content_past_the_final_window_is_refused_too() {
        let mut line = String::from("    X         COST      1.0");
        while line.len() < 63 {
            line.push(' ');
        }
        line.push_str("note");
        let refusal = classify(Form::Fixed, &line, 9).expect_err("content past the last window");
        assert_eq!(refusal.id, Rejection::INSTANCE_FIELD_OVERRUN);
    }

    #[test]
    fn an_indicator_where_a_section_takes_none_is_refused() {
        let line = fixed(" XX X         COST      1.0");
        let refusal = record(&line)
            .fields(Shape::Plain)
            .expect_err("an unexpected indicator");
        assert_eq!(refusal.id, Rejection::INSTANCE_FIELD_OVERRUN);
    }

    #[test]
    fn free_form_fields_shift_with_the_shape() {
        let plain = free("    X  COST  -1.0");
        let fields = record(&plain).fields(Shape::Plain).expect("a plain record");
        assert_eq!(fields.indicator, "");
        assert_eq!(fields.name1, "X");
        assert_eq!(fields.name2, "COST");
        assert_eq!(fields.value1, "-1.0");

        let indicated = free(" UI BND X 10");
        let fields = record(&indicated)
            .fields(Shape::Indicated)
            .expect("an indicated record");
        assert_eq!(fields.indicator, "UI");
        assert_eq!(fields.name1, "BND");
        assert_eq!(fields.name2, "X");
        assert_eq!(fields.value1, "10");
    }

    #[test]
    fn a_free_record_with_too_many_fields_is_refused_rather_than_trimmed() {
        let line = free(" X COST 1.0 R1 1.0 EXTRA");
        let refusal = record(&line)
            .fields(Shape::Plain)
            .expect_err("too many fields");
        assert_eq!(refusal.id, Rejection::INSTANCE_TOO_MANY_FIELDS);
        assert_eq!(record(&line).arity(), 6);

        // The same six fields are exactly what an indicated record accepts.
        assert!(record(&line).fields(Shape::Indicated).is_ok());
        let seven = free(" A B C D E F G");
        assert!(record(&seven).fields(Shape::Indicated).is_err());
    }

    #[test]
    fn a_short_fixed_line_simply_has_empty_windows() {
        let line = fixed("    X");
        let fields = record(&line).fields(Shape::Plain).expect("a plain record");
        assert_eq!(fields.name1, "X");
        assert_eq!(fields.name2, "");
        assert_eq!(fields.value1, "");
        assert_eq!(record(&line).arity(), 1);
    }

    #[test]
    fn a_record_remembers_the_line_it_came_from() {
        let line = classify(Form::Free, " X COST 1.0", 42).expect("a record");
        assert_eq!(record(&line).line(), 42);
        assert_eq!(record(&line).form(), Form::Free);
    }
}
