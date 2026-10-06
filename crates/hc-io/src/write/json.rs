//! Writing a model as a JSON instance document.
//!
//! The most faithful of the three, because it is the one designed for this
//! rather than adapted to it. Every member is written, including the ones a
//! reader could have defaulted, and every model value is a decimal string.

use core::fmt::Write as _;

use hc_core::{Model, RowId, Sense};

use crate::write::{Unwritable, decimal, row_entries};

/// Writes a model as a JSON instance document.
///
/// # Errors
///
/// Returns a refusal when the model cannot be spelled in the schema.
pub fn write(model: &Model) -> Result<String, Unwritable> {
    if !model.discarded().is_empty() {
        return Err(Unwritable::new(
            "the model carries discarded free rows, which this schema has no member for",
        ));
    }
    if model.variables().is_empty() {
        return Err(Unwritable::new(
            "the schema refuses a document with no variables",
        ));
    }
    for row in model.rows() {
        if row.lower.is_none() && row.upper.is_none() {
            return Err(Unwritable::new(format!(
                "row {:?} has no finite side, and this schema has no free row",
                row.name
            )));
        }
        if let (Some(low), Some(high)) = (row.lower.as_ref(), row.upper.as_ref())
            && low > high
        {
            return Err(Unwritable::new(format!(
                "row {:?} has sides that cross, which no range in this schema produces",
                row.name
            )));
        }
    }

    let negate = model.original_sense() == Sense::Max;
    let flip = |value: &hc_exact::Rational| {
        if negate { value.neg() } else { value.clone() }
    };

    let mut out = String::from("{\n");
    let _ = writeln!(out, "  \"version\": 1,");
    let _ = writeln!(
        out,
        "  \"sense\": \"{}\",",
        if negate { "max" } else { "min" }
    );
    let _ = writeln!(
        out,
        "  \"objective_constant\": \"{}\",",
        decimal(&flip(model.objective_constant()))?
    );

    out.push_str("  \"vars\": [\n");
    for (index, variable) in model.variables().iter().enumerate() {
        let comma = if index + 1 == model.variables().len() {
            ""
        } else {
            ","
        };
        let _ = writeln!(
            out,
            "    {{\"name\": {}, \"lb\": {}, \"ub\": {}, \"integer\": {}, \"obj\": \"{}\"}}{comma}",
            quoted(&variable.name),
            side(variable.lower.as_ref())?,
            side(variable.upper.as_ref())?,
            variable.integral,
            decimal(&flip(&variable.objective))?
        );
    }
    out.push_str("  ],\n");

    out.push_str("  \"rows\": [\n");
    for (index, row) in model.rows().iter().enumerate() {
        let comma = if index + 1 == model.rows().len() {
            ""
        } else {
            ","
        };
        // Every row is written on the sense whose range is a widening, so the
        // range member is never asked to do something the table cannot.
        let (sense, base, range) = match (row.lower.as_ref(), row.upper.as_ref()) {
            (Some(low), Some(high)) if low == high => ("E", low.clone(), None),
            (Some(low), Some(high)) => ("G", low.clone(), Some(high.sub(low))),
            (Some(low), None) => ("G", low.clone(), None),
            (None, Some(high)) => ("L", high.clone(), None),
            (None, None) => unreachable!("the free row was refused above"),
        };
        let _ = writeln!(
            out,
            "    {{\"name\": {}, \"sense\": \"{sense}\", \"rhs\": \"{}\", \"range\": {}}}{comma}",
            quoted(&row.name),
            decimal(&base)?,
            match range.as_ref() {
                None => "null".to_owned(),
                Some(value) => format!("\"{}\"", decimal(value)?),
            }
        );
    }
    out.push_str("  ],\n");

    let mut entries: Vec<String> = Vec::new();
    for index in 0..model.rows().len() {
        let row = RowId(index as u32);
        for (variable, value) in row_entries(model, row) {
            match value {
                Some(value) => entries.push(format!(
                    "    [{index}, {}, \"{}\"]",
                    variable.index(),
                    decimal(value)?
                )),
                // The pair that cancelled, written back as a pair.
                None => {
                    entries.push(format!("    [{index}, {}, \"1\"]", variable.index()));
                    entries.push(format!("    [{index}, {}, \"-1\"]", variable.index()));
                }
            }
        }
    }
    out.push_str("  \"nonzeros\": [\n");
    out.push_str(&entries.join(",\n"));
    if !entries.is_empty() {
        out.push('\n');
    }
    out.push_str("  ]\n}\n");
    Ok(out)
}

/// A bound, or the null that spells its absence.
fn side(value: Option<&hc_exact::Rational>) -> Result<String, Unwritable> {
    match value {
        None => Ok("null".to_owned()),
        Some(value) => Ok(format!("\"{}\"", decimal(value)?)),
    }
}

/// A name as a JSON string, with the two escapes a name can need.
fn quoted(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 2);
    out.push('"');
    for character in name.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}
