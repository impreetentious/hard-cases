//! Writing a model as free-form MPS.
//!
//! Free form rather than fixed, because the fixed windows cap a name at eight
//! characters and a writer that silently truncated one would produce a file
//! naming a different model. A name carrying whitespace is refused here for
//! the mirror-image reason: free form splits on whitespace, so such a name has
//! no spelling in this variant either.

use core::fmt::Write as _;

use hc_core::{Model, RowId, Sense};

use crate::write::{Unwritable, decimal, row_entries};

/// Writes a model as a free-form MPS file.
///
/// # Errors
///
/// Returns a refusal when the model cannot be spelled in the format.
pub fn write(model: &Model) -> Result<String, Unwritable> {
    for variable in model.variables() {
        admit(&variable.name, "column")?;
    }
    for row in model.rows() {
        admit(&row.name, "row")?;
        if row.lower.is_none() && row.upper.is_none() {
            return Err(Unwritable::new(format!(
                "row {:?} has no finite side, and a retained row is written by its sides",
                row.name
            )));
        }
        if let (Some(low), Some(high)) = (row.lower.as_ref(), row.upper.as_ref())
            && low > high
        {
            return Err(Unwritable::new(format!(
                "row {:?} has sides that cross, which no range in this format produces",
                row.name
            )));
        }
    }
    for name in model.discarded() {
        admit(name, "discarded row")?;
    }
    let objective = label(model);
    let negate = model.original_sense() == Sense::Max;
    let flip = |value: &hc_exact::Rational| {
        if negate { value.neg() } else { value.clone() }
    };

    let mut out = String::from("* Written from a canonical model.\n");
    out.push_str("NAME\n");
    if negate {
        out.push_str("OBJSENSE\n    MAX\n");
    }

    out.push_str("ROWS\n");
    let _ = writeln!(out, " N  {objective}");
    // The discarded free rows come back as free rows, after the objective, so
    // that the reader discards exactly the ones it discarded before.
    for name in model.discarded() {
        let _ = writeln!(out, " N  {name}");
    }
    for row in model.rows() {
        let _ = writeln!(out, " {}  {}", sense(row), row.name);
    }

    out.push_str("COLUMNS\n");
    for (index, variable) in model.variables().iter().enumerate() {
        if variable.integral {
            let _ = writeln!(out, "    MARKER{index}  'MARKER'  'INTORG'");
        }
        let mut wrote = false;
        if !variable.objective.is_zero() {
            let _ = writeln!(
                out,
                "    {}  {objective}  {}",
                variable.name,
                decimal(&flip(&variable.objective))?
            );
            wrote = true;
        }
        for (row, position) in rows_of(model, index) {
            match position {
                Some(value) => {
                    let _ = writeln!(
                        out,
                        "    {}  {}  {}",
                        variable.name,
                        model.rows()[row].name,
                        decimal(value)?
                    );
                }
                None => {
                    // The pair that cancelled, written back as a pair.
                    let _ = writeln!(out, "    {}  {}  1", variable.name, model.rows()[row].name);
                    let _ = writeln!(out, "    {}  {}  -1", variable.name, model.rows()[row].name);
                }
            }
            wrote = true;
        }
        if !wrote {
            // A column with nothing to say still has to be declared, and an
            // explicit zero on the objective says exactly nothing.
            let _ = writeln!(out, "    {}  {objective}  0", variable.name);
        }
        if variable.integral {
            let _ = writeln!(out, "    MARKER{index}  'MARKER'  'INTEND'");
        }
    }

    out.push_str("RHS\n");
    if !model.objective_constant().is_zero() {
        // The reader negates a right-hand side on the objective row, and then
        // negates the whole constant again when the file maximizes.
        let stated = if negate {
            model.objective_constant().clone()
        } else {
            model.objective_constant().neg()
        };
        let _ = writeln!(out, "    rhs  {objective}  {}", decimal(&stated)?);
    }
    for row in model.rows() {
        let _ = writeln!(out, "    rhs  {}  {}", row.name, decimal(&base(row))?);
    }

    let ranged: Vec<&hc_core::Row> = model
        .rows()
        .iter()
        .filter(|row| {
            matches!((row.lower.as_ref(), row.upper.as_ref()), (Some(low), Some(high)) if low != high)
        })
        .collect();
    if !ranged.is_empty() {
        out.push_str("RANGES\n");
        for row in ranged {
            let low = row.lower.as_ref().expect("a ranged row has both sides");
            let high = row.upper.as_ref().expect("a ranged row has both sides");
            let _ = writeln!(out, "    rng  {}  {}", row.name, decimal(&high.sub(low))?);
        }
    }

    out.push_str("BOUNDS\n");
    for variable in model.variables() {
        let name = &variable.name;
        match (variable.lower.as_ref(), variable.upper.as_ref()) {
            (None, None) => {
                let _ = writeln!(out, " FR bnd  {name}");
            }
            (Some(low), None) => {
                let _ = writeln!(out, " LO bnd  {name}  {}", decimal(low)?);
            }
            // The lower side is opened before the upper side is given, so a
            // negative upper bound never meets the default lower bound and the
            // fork this format has there is never reached.
            (None, Some(high)) => {
                let _ = writeln!(out, " MI bnd  {name}");
                let _ = writeln!(out, " UP bnd  {name}  {}", decimal(high)?);
            }
            (Some(low), Some(high)) => {
                let _ = writeln!(out, " LO bnd  {name}  {}", decimal(low)?);
                let _ = writeln!(out, " UP bnd  {name}  {}", decimal(high)?);
            }
        }
    }

    out.push_str("ENDATA\n");
    Ok(out)
}

/// The coefficients of one column, cancellations restored, in row order.
fn rows_of(model: &Model, column: usize) -> Vec<(usize, Option<&hc_exact::Rational>)> {
    let mut out = Vec::new();
    for index in 0..model.rows().len() {
        for (variable, value) in row_entries(model, RowId(index as u32)) {
            if variable.index() == column {
                out.push((index, value));
            }
        }
    }
    out
}

/// The sense a row is written on.
fn sense(row: &hc_core::Row) -> char {
    match (row.lower.as_ref(), row.upper.as_ref()) {
        (Some(low), Some(high)) if low == high => 'E',
        (Some(_), _) => 'G',
        (None, _) => 'L',
    }
}

/// The right-hand side a row is written with, which its range widens from.
fn base(row: &hc_core::Row) -> hc_exact::Rational {
    match (row.lower.as_ref(), row.upper.as_ref()) {
        (Some(low), _) => low.clone(),
        (None, Some(high)) => high.clone(),
        (None, None) => unreachable!("the free row was refused above"),
    }
}

/// Refuses a name this variant cannot spell.
fn admit(name: &str, what: &str) -> Result<(), Unwritable> {
    if name.is_empty() || name.chars().any(char::is_whitespace) {
        return Err(Unwritable::new(format!(
            "the {what} name {name:?} is empty or carries whitespace, which free form splits on"
        )));
    }
    Ok(())
}

/// A name for the objective row that no other row has taken.
fn label(model: &Model) -> String {
    let taken = |candidate: &str| {
        model.rows().iter().any(|row| row.name == candidate)
            || model.discarded().iter().any(|name| name == candidate)
    };
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
