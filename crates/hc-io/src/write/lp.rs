//! Writing a model as LP format.
//!
//! The accepted subset is narrow, so this refuses more often than the other
//! two writers do: a label that is not an identifier, or is a reserved word,
//! has no spelling here. What it does produce is inside the subset by
//! construction — every statement labelled, every bound two-sided, every
//! coefficient explicit — so nothing it writes depends on a default.

use core::fmt::Write as _;

use hc_core::{Model, RowId, Sense};
use hc_exact::Rational;

use crate::lp::token;
use crate::write::{Unwritable, decimal, row_entries};

/// Writes a model as an LP-format file.
///
/// # Errors
///
/// Returns a refusal when the model cannot be spelled in the accepted subset.
pub fn write(model: &Model) -> Result<String, Unwritable> {
    if !model.discarded().is_empty() {
        return Err(Unwritable::new(
            "the model carries discarded free rows, which this format has no section for",
        ));
    }
    for variable in model.variables() {
        admit(&variable.name, "variable")?;
    }
    for row in model.rows() {
        admit(&row.name, "constraint")?;
    }
    let objective = label(model);

    let mut out = String::new();
    out.push_str("\\ Written from a canonical model.\n");

    let negate = model.original_sense() == Sense::Max;
    out.push_str(if negate { "Maximize\n" } else { "Minimize\n" });
    let _ = write!(out, " {objective}:");
    for variable in model.variables() {
        if variable.objective.is_zero() {
            continue;
        }
        term(&mut out, &variable.objective, &variable.name, negate)?;
    }
    if !model.objective_constant().is_zero() {
        signed(&mut out, model.objective_constant(), negate)?;
    }
    out.push('\n');

    out.push_str("Subject To\n");
    for (index, row) in model.rows().iter().enumerate() {
        let entries = row_entries(model, RowId(index as u32));
        let mut body = String::new();
        for (variable, value) in &entries {
            let name = &model.variables()[variable.index()].name;
            match value {
                Some(value) => term(&mut body, value, name, false)?,
                None => {
                    // The pair that cancelled, written back as a pair. The
                    // canonical dump records the cancellation, so a file that
                    // dropped it would be a different file.
                    let _ = write!(body, " + 1 {name} - 1 {name}");
                }
            }
        }
        if entries.is_empty() {
            // A row with no coefficients still has to be a statement about
            // something. An explicit zero says exactly what the model says.
            let Some(first) = model.variables().first() else {
                return Err(Unwritable::new(
                    "a model with no variables cannot carry a row with no coefficients",
                ));
            };
            let _ = write!(body, " + 0 {}", first.name);
        }

        let _ = write!(out, " {}:", row.name);
        match (row.lower.as_ref(), row.upper.as_ref()) {
            (Some(low), Some(high)) if low == high => {
                let _ = writeln!(out, "{body} = {}", decimal(low)?);
            }
            (Some(low), Some(high)) => {
                let _ = writeln!(out, " {} <={body} <= {}", decimal(low)?, decimal(high)?);
            }
            (Some(low), None) => {
                let _ = writeln!(out, "{body} >= {}", decimal(low)?);
            }
            (None, Some(high)) => {
                let _ = writeln!(out, "{body} <= {}", decimal(high)?);
            }
            (None, None) => {
                return Err(Unwritable::new(format!(
                    "row {:?} has no finite side, and this format writes a row by its sides",
                    row.name
                )));
            }
        }
    }

    out.push_str("Bounds\n");
    for variable in model.variables() {
        let name = &variable.name;
        match (variable.lower.as_ref(), variable.upper.as_ref()) {
            (None, None) => {
                let _ = writeln!(out, " {name} free");
            }
            (Some(low), None) => {
                let _ = writeln!(out, " {name} >= {}", decimal(low)?);
            }
            // Both sides are written even where one of them is the default,
            // so that a negative upper bound never meets a lower bound nobody
            // gave and the fork this format has there is never reached.
            (None, Some(high)) => {
                let _ = writeln!(out, " -inf <= {name} <= {}", decimal(high)?);
            }
            (Some(low), Some(high)) => {
                let _ = writeln!(out, " {} <= {name} <= {}", decimal(low)?, decimal(high)?);
            }
        }
    }

    let integers: Vec<&str> = model
        .variables()
        .iter()
        .filter(|variable| variable.integral)
        .map(|variable| variable.name.as_str())
        .collect();
    if !integers.is_empty() {
        // Every integer column goes in the general section, including the ones
        // bounded at zero and one. The binary section would set those bounds
        // itself, and a bound stated twice is refused on the way back in.
        out.push_str("General\n");
        for name in integers {
            let _ = writeln!(out, " {name}");
        }
    }

    out.push_str("End\n");
    Ok(out)
}

/// Refuses a name this format cannot spell.
fn admit(name: &str, what: &str) -> Result<(), Unwritable> {
    let mut characters = name.chars();
    let start = characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_');
    let rest = characters
        .all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '.');
    if !start || !rest {
        return Err(Unwritable::new(format!(
            "the {what} name {name:?} is not spellable as an LP-format name"
        )));
    }
    if token::is_reserved(name) {
        return Err(Unwritable::new(format!(
            "the {what} name {name:?} is a reserved word in this format"
        )));
    }
    Ok(())
}

/// A label for the objective that no row has taken.
fn label(model: &Model) -> String {
    let taken = |candidate: &str| model.rows().iter().any(|row| row.name == candidate);
    if !taken("obj") {
        return "obj".to_owned();
    }
    let mut suffix = 1u32;
    loop {
        let candidate = format!("obj_{suffix}");
        if !taken(&candidate) {
            return candidate;
        }
        suffix += 1;
    }
}

/// Writes one term of a linear form, sign and coefficient and all.
fn term(out: &mut String, value: &Rational, name: &str, negate: bool) -> Result<(), Unwritable> {
    signed(out, value, negate)?;
    let _ = write!(out, " {name}");
    Ok(())
}

/// Writes a signed magnitude, restoring the direction the file was written in.
fn signed(out: &mut String, value: &Rational, negate: bool) -> Result<(), Unwritable> {
    let written = if negate { value.neg() } else { value.clone() };
    let _ = write!(
        out,
        " {} {}",
        if written.is_negative() { '-' } else { '+' },
        decimal(&written.abs())?
    );
    Ok(())
}
