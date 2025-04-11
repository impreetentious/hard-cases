//! The JSON document as a canonical exact model.
//!
//! Every member is required, including the ones that are usually defaulted.
//! Absence is spelled `null` where absence means something; a member that is
//! simply missing is refused, because a default is a second reading of the
//! same document and this format has no history that would justify one.

use std::collections::BTreeMap;

use hc_core::{
    FormatTag, Model, ModelParts, Refusal, Rejection, Row, RowId, Sense, VarId, Variable,
    instance_digest,
};
use hc_exact::Rational;

use crate::json::value::{Json, parse};
use crate::numeric::Limits;
use crate::text::admit_text;

/// How to read a document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// The caps and fences.
    pub limits: Limits,
}

impl Options {
    /// The reading the specification states.
    ///
    /// There is no lenient mode. The other two formats have one because they
    /// have files in the world already written; this one does not, so every
    /// document it will ever see can be written the accepted way.
    #[must_use]
    pub const fn strict() -> Self {
        Self {
            limits: Limits::SPEC,
        }
    }
}

impl Default for Options {
    fn default() -> Self {
        Self::strict()
    }
}

/// The version of the schema this reader accepts.
const VERSION: i64 = 1;

/// The members of the document, and of each kind of element.
const DOCUMENT: &[&str] = &[
    "version",
    "sense",
    "objective_constant",
    "vars",
    "rows",
    "nonzeros",
];
const VAR: &[&str] = &["name", "lb", "ub", "integer", "obj"];
const ROW: &[&str] = &["name", "sense", "rhs", "range"];

/// Reads a document.
///
/// # Errors
///
/// Returns a refusal naming the rule that refused it.
pub fn read(raw: &[u8]) -> Result<Model, Refusal> {
    read_with(raw, Options::strict())
}

/// Reads a document under explicit options.
///
/// # Errors
///
/// Returns a refusal naming the rule that refused it.
pub fn read_with(raw: &[u8], options: Options) -> Result<Model, Refusal> {
    options.limits.admit_file(raw.len())?;
    let text = admit_text(raw)?;
    let digest = instance_digest(FormatTag::Json.byte(), raw);
    let document = parse(text)?;
    build(&document, options, digest)
}

/// Refuses a document with the schema identifier.
fn refuse(detail: impl Into<String>) -> Refusal {
    Refusal::new(Rejection::INSTANCE_JSON_SCHEMA, detail)
}

/// The members of an object, checked to be exactly the ones named.
fn members<'a>(value: &'a Json, expected: &[&str], what: &str) -> Result<&'a Json, Refusal> {
    let Json::Object(present) = value else {
        return Err(refuse(format!("{what} is {}, not an object", value.kind())));
    };
    for name in expected {
        if !present.iter().any(|(key, _)| key == name) {
            return Err(refuse(format!("{what} has no member {name:?}")));
        }
    }
    for (name, _) in present {
        if !expected.contains(&name.as_str()) {
            return Err(refuse(format!(
                "{what} carries the unknown member {name:?}"
            )));
        }
    }
    Ok(value)
}

/// A required string member.
fn text<'a>(value: &'a Json, name: &str, what: &str) -> Result<&'a str, Refusal> {
    match value.member(name) {
        Some(Json::Text(found)) => Ok(found),
        Some(other) => Err(refuse(format!(
            "the {name} of {what} is {}, not a string",
            other.kind()
        ))),
        None => Err(refuse(format!("{what} has no member {name:?}"))),
    }
}

/// A required string member that may be spelled `null`.
fn optional<'a>(value: &'a Json, name: &str, what: &str) -> Result<Option<&'a str>, Refusal> {
    match value.member(name) {
        Some(Json::Text(found)) => Ok(Some(found)),
        Some(Json::Null) => Ok(None),
        Some(other) => Err(refuse(format!(
            "the {name} of {what} is {}, not a string or null",
            other.kind()
        ))),
        None => Err(refuse(format!("{what} has no member {name:?}"))),
    }
}

/// A required array member.
fn list<'a>(value: &'a Json, name: &str) -> Result<&'a [Json], Refusal> {
    match value.member(name) {
        Some(Json::List(items)) => Ok(items),
        Some(other) => Err(refuse(format!(
            "the {name} of the document is {}, not an array",
            other.kind()
        ))),
        None => Err(refuse(format!("the document has no member {name:?}"))),
    }
}

/// Turns an accepted document into the model it describes.
fn build(document: &Json, options: Options, digest: [u8; 32]) -> Result<Model, Refusal> {
    members(document, DOCUMENT, "the document")?;

    match document.member("version") {
        Some(Json::Integer(VERSION)) => {}
        Some(Json::Integer(other)) => {
            return Err(refuse(format!(
                "the document states version {other}, and this reader accepts {VERSION}"
            )));
        }
        _ => return Err(refuse("the version is not an integer")),
    }

    let sense = match text(document, "sense", "the document")? {
        "min" => Sense::Min,
        "max" => Sense::Max,
        other => {
            return Err(refuse(format!(
                "the sense {other:?} is neither min nor max"
            )));
        }
    };
    let negate = sense == Sense::Max;
    let limits = options.limits;

    let written = limits.read_number(text(document, "objective_constant", "the document")?, 0)?;
    Limits::admit_value(&written, "the objective constant", 0)?;
    let objective_constant = if negate { written.neg() } else { written };

    // ---------------------------------------------------------------- columns
    let columns = list(document, "vars")?;
    if columns.is_empty() {
        return Err(refuse(
            "the document declares no variables, so it states nothing to solve",
        ));
    }
    let mut names: BTreeMap<&str, usize> = BTreeMap::new();
    let mut variables = Vec::with_capacity(columns.len());
    for (index, column) in columns.iter().enumerate() {
        let what = format!("variable {index}");
        members(column, VAR, &what)?;
        let name = text(column, "name", &what)?;
        limits.admit_name(name, 0)?;
        if names.insert(name, index).is_some() {
            return Err(Refusal::new(
                Rejection::INSTANCE_DUPLICATE_COLUMN,
                format!("two variables share the name {name:?}"),
            ));
        }
        // The members are read in the order the schema declares them, so that
        // a document with two defects is refused for the earlier one whichever
        // reader is looking at it.
        let bound = |member: &str| -> Result<Option<Rational>, Refusal> {
            match optional(column, member, &what)? {
                None => Ok(None),
                Some(written) => {
                    let value = limits.read_number(written, 0)?;
                    Limits::admit_value(&value, "a bound", 0)?;
                    Ok(Some(value))
                }
            }
        };
        let lower = bound("lb")?;
        let upper = bound("ub")?;
        let integral = match column.member("integer") {
            Some(Json::Bool(flag)) => *flag,
            _ => {
                return Err(refuse(format!(
                    "the integer member of {what} is not a boolean"
                )));
            }
        };
        let written = limits.read_number(text(column, "obj", &what)?, 0)?;
        Limits::admit_value(&written, "an objective coefficient", 0)?;
        variables.push(Variable {
            lower,
            upper,
            integral,
            objective: if negate { written.neg() } else { written },
            name: name.to_owned(),
        });
    }

    // ------------------------------------------------------------------- rows
    let declared = list(document, "rows")?;
    let mut labels: BTreeMap<&str, usize> = BTreeMap::new();
    let mut rows = Vec::with_capacity(declared.len());
    for (index, row) in declared.iter().enumerate() {
        let what = format!("row {index}");
        members(row, ROW, &what)?;
        let name = text(row, "name", &what)?;
        limits.admit_name(name, 0)?;
        if labels.insert(name, index).is_some() {
            return Err(Refusal::new(
                Rejection::INSTANCE_DUPLICATE_ROW,
                format!("two rows share the name {name:?}"),
            ));
        }
        let sense = shape(text(row, "sense", &what)?)?;
        let base = limits.read_number(text(row, "rhs", &what)?, 0)?;
        Limits::admit_value(&base, "a right-hand side", 0)?;
        let range = match optional(row, "range", &what)? {
            None => None,
            Some(written) => {
                let value = limits.read_number(written, 0)?;
                Limits::admit_value(&value, "a range", 0)?;
                Some(value)
            }
        };
        let (lower, upper) = sided(sense, &base, range.as_ref());
        rows.push(Row {
            lower,
            upper,
            name: name.to_owned(),
        });
    }

    // -------------------------------------------------------------- the matrix
    let mut totals: BTreeMap<(usize, usize), (Rational, u32)> = BTreeMap::new();
    for (index, entry) in list(document, "nonzeros")?.iter().enumerate() {
        let Json::List(triple) = entry else {
            return Err(refuse(format!(
                "coefficient {index} is {}, not an array",
                entry.kind()
            )));
        };
        let [
            Json::Integer(row),
            Json::Integer(column),
            Json::Text(written),
        ] = triple.as_slice()
        else {
            return Err(refuse(format!(
                "coefficient {index} is not a row index, a column index, and a decimal string"
            )));
        };
        let row = index_within(*row, rows.len(), "row", index)?;
        let column = index_within(*column, variables.len(), "column", index)?;
        let value = limits.read_number(written, 0)?;
        Limits::admit_value(&value, "a coefficient", 0)?;
        match totals.get_mut(&(column, row)) {
            Some(slot) => {
                let existing = core::mem::replace(&mut slot.0, Rational::zero());
                slot.0 = existing.add(&value);
                slot.1 += 1;
            }
            None => {
                limits.admit_nonzero(totals.len(), 0)?;
                totals.insert((column, row), (value, 1));
            }
        }
    }

    let mut entries = Vec::new();
    let mut zero_sums = Vec::new();
    for ((column, row), (value, contributions)) in totals {
        let position = (VarId(column as u32), RowId(row as u32));
        if value.is_zero() {
            // A cancellation and an absence are the same matrix and different
            // documents.
            if contributions > 1 {
                zero_sums.push(position);
            }
            continue;
        }
        entries.push((position.0, position.1, value));
    }

    Ok(Model::new(ModelParts {
        format: FormatTag::Json,
        digest,
        original_sense: sense,
        objective_constant,
        variables,
        rows,
        entries,
        zero_sums,
        discarded: Vec::new(),
    }))
}

/// How a row was written.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    AtMost,
    AtLeast,
    Exactly,
}

/// Reads a row sense.
fn shape(sense: &str) -> Result<Shape, Refusal> {
    match sense {
        "L" => Ok(Shape::AtMost),
        "G" => Ok(Shape::AtLeast),
        "E" => Ok(Shape::Exactly),
        other => Err(refuse(format!(
            "the row sense {other:?} is outside the accepted set"
        ))),
    }
}

/// Turns a row sense, a right-hand side, and a range into the two sides.
///
/// The table is the one the MPS reader applies, deliberately and to the
/// character. Two formats that widen a row differently would make a model
/// depend on which of them a caller happened to write it in.
fn sided(
    sense: Shape,
    base: &Rational,
    range: Option<&Rational>,
) -> (Option<Rational>, Option<Rational>) {
    let Some(range) = range else {
        return match sense {
            Shape::AtMost => (None, Some(base.clone())),
            Shape::AtLeast => (Some(base.clone()), None),
            Shape::Exactly => (Some(base.clone()), Some(base.clone())),
        };
    };
    let magnitude = range.abs();
    match sense {
        Shape::AtMost => (Some(base.sub(&magnitude)), Some(base.clone())),
        Shape::AtLeast => (Some(base.clone()), Some(base.add(&magnitude))),
        Shape::Exactly => {
            if range.is_negative() {
                (Some(base.add(range)), Some(base.clone()))
            } else {
                // A zero range on an equality row is a no-op rather than an
                // error. It says nothing and is permitted to say nothing.
                (Some(base.clone()), Some(base.add(range)))
            }
        }
    }
}

/// Checks one index against the array it points into.
fn index_within(value: i64, count: usize, what: &str, at: usize) -> Result<usize, Refusal> {
    let index = usize::try_from(value)
        .ok()
        .filter(|found| *found < count)
        .ok_or_else(|| {
            refuse(format!(
                "coefficient {at} names {what} {value}, and there are {count}"
            ))
        })?;
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TINY: &str = r#"{
  "version": 1,
  "sense": "min",
  "objective_constant": "0",
  "vars": [{"name": "x", "lb": "0", "ub": "10", "integer": true, "obj": "-1"}],
  "rows": [{"name": "c1", "sense": "L", "rhs": "4", "range": null}],
  "nonzeros": [[0, 0, "1"]]
}
"#;

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
    fn trap_json_unknown_member() {
        // Forward compatibility by ignoring what is not understood is a
        // reasonable policy for a configuration file and a disastrous one for
        // a model somebody is about to prove a theorem about.
        let extra = TINY.replace(
            "\"version\": 1,",
            "\"version\": 1,\n  \"tolerance\": \"0\",",
        );
        assert_eq!(refusal(&extra).id, Rejection::INSTANCE_JSON_SCHEMA);

        // Inside an element as well as at the top, and a missing member is
        // refused rather than defaulted.
        let inner = TINY.replace("\"integer\": true", "\"integer\": true, \"scale\": \"2\"");
        assert_eq!(refusal(&inner).id, Rejection::INSTANCE_JSON_SCHEMA);
        let absent = TINY.replace("\"range\": null", "\"unused\": null");
        assert_eq!(refusal(&absent).id, Rejection::INSTANCE_JSON_SCHEMA);
    }

    #[test]
    fn trap_json_range_matches_mps() {
        // The classic disagreement, reached through a different grammar. Every
        // sense crossed with every range sign, against hand-computed bounds.
        let cases = [
            ("G", "4", "2", "2", "6"),
            ("G", "-4", "2", "2", "6"),
            ("L", "4", "10", "6", "10"),
            ("L", "-4", "10", "6", "10"),
            ("E", "4", "10", "10", "14"),
            ("E", "-4", "10", "6", "10"),
            ("E", "0", "10", "10", "10"),
        ];
        for (sense, range, rhs, lower, upper) in cases {
            let text = TINY.replace(
                "\"sense\": \"L\", \"rhs\": \"4\", \"range\": null",
                &format!("\"sense\": \"{sense}\", \"rhs\": \"{rhs}\", \"range\": \"{range}\""),
            );
            let built = model(&text);
            let row = &built.rows()[0];
            assert_eq!(shown(row.lower.as_ref()), lower, "{sense} {rhs} {range}");
            assert_eq!(shown(row.upper.as_ref()), upper, "{sense} {rhs} {range}");
        }
    }

    #[test]
    fn the_absent_side_of_a_bound_is_a_null_and_not_a_missing_member() {
        let both = TINY.replace(
            "\"lb\": \"0\", \"ub\": \"10\"",
            "\"lb\": null, \"ub\": null",
        );
        let built = model(&both);
        assert_eq!(built.variables()[0].lower, None);
        assert_eq!(built.variables()[0].upper, None);

        let gone = TINY.replace("\"lb\": \"0\", ", "");
        assert_eq!(refusal(&gone).id, Rejection::INSTANCE_JSON_SCHEMA);
    }

    #[test]
    fn a_maximizing_document_is_negated_where_it_is_read() {
        let text = TINY
            .replace("\"sense\": \"min\"", "\"sense\": \"max\"")
            .replace(
                "\"objective_constant\": \"0\"",
                "\"objective_constant\": \"7\"",
            );
        let built = model(&text);
        assert_eq!(built.variables()[0].objective.to_canonical_string(), "1");
        assert_eq!(built.objective_constant().to_canonical_string(), "-7");
        assert_eq!(built.original_sense(), Sense::Max);
    }

    #[test]
    fn a_model_with_no_rows_is_an_ordinary_box() {
        // Empty rows are accepted; empty variables are not. A model
        // constrained only by its bounds is a real model.
        let text = TINY
            .replace(
                "\"rows\": [{\"name\": \"c1\", \"sense\": \"L\", \"rhs\": \"4\", \"range\": null}]",
                "\"rows\": []",
            )
            .replace("\"nonzeros\": [[0, 0, \"1\"]]", "\"nonzeros\": []");
        let built = model(&text);
        assert_eq!(built.rows().len(), 0);
        assert_eq!(built.variables().len(), 1);
    }

    #[test]
    fn coefficients_may_arrive_in_any_order_and_still_sum() {
        let text = TINY.replace(
            "\"nonzeros\": [[0, 0, \"1\"]]",
            "\"nonzeros\": [[0, 0, \"3\"], [0, 0, \"-2\"]]",
        );
        let built = model(&text);
        assert_eq!(built.matrix().nonzeros(), 1);
        let (_, _, value) = built.matrix().entries().next().expect("one coefficient");
        assert_eq!(value.to_canonical_string(), "1");
    }

    #[test]
    fn the_element_shapes_are_checked_before_their_contents() {
        for text in [
            TINY.replace("[[0, 0, \"1\"]]", "[[0, 0]]"),
            TINY.replace("[[0, 0, \"1\"]]", "[[0, 0, 1]]"),
            TINY.replace("[[0, 0, \"1\"]]", "[[\"0\", 0, \"1\"]]"),
            TINY.replace("\"vars\": [", "\"vars\": [\"x\", "),
        ] {
            assert_eq!(refusal(&text).id, Rejection::INSTANCE_JSON_SCHEMA, "{text}");
        }
    }
}
