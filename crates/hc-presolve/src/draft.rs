//! A model under construction.
//!
//! A transform produces a new model, not a patch to an old one. That is what
//! makes the chain replayable: the verifier holds a model, applies a rule, and
//! compares the hash of what it built against what the record states. A patch
//! would need the two sides to agree about how patches compose.
//!
//! Deleting a column renumbers the ones after it. That is not an inconvenience
//! to be worked around — it is what the reduced model *is*, and the identifiers
//! a proof cites are the reduced model's, assigned by the same canonical rule
//! as any other model's.

use hc_core::{FormatTag, Model, ModelParts, RowId, Sense, VarId, Variable};
use hc_exact::Rational;

/// A model being rebuilt, with rows and columns addressed by position.
#[derive(Clone, Debug)]
pub struct Draft {
    /// Which rules produced the original file.
    pub format: FormatTag,
    /// The digest of the original file, carried through unchanged.
    ///
    /// A reduced model is still about that file. The digest names the bytes a
    /// proof is ultimately checked against, and losing it partway down the
    /// chain would sever the reduced model from the thing it is a reduction
    /// of.
    pub digest: [u8; 32],
    /// The sense the file was written in.
    pub sense: Sense,
    /// The objective constant, in the minimizing sense.
    pub constant: Rational,
    /// The columns, in index order.
    pub variables: Vec<Variable>,
    /// The rows, in index order.
    pub rows: Vec<hc_core::Row>,
    /// The coefficients, as `(column, row, value)`, unsorted.
    pub entries: Vec<(usize, usize, Rational)>,
}

impl Draft {
    /// Takes a model apart.
    #[must_use]
    pub fn of(model: &Model) -> Self {
        let entries = model
            .matrix()
            .entries()
            .map(|(column, row, value)| (column.index(), row.index(), value.clone()))
            .collect();
        Self {
            format: model.format(),
            digest: *model.digest(),
            sense: model.original_sense(),
            constant: model.objective_constant().clone(),
            variables: model.variables().to_vec(),
            rows: model.rows().to_vec(),
            entries,
        }
    }

    /// The coefficient at a position, when one is stored there.
    #[must_use]
    pub fn get(&self, column: usize, row: usize) -> Option<&Rational> {
        self.entries
            .iter()
            .find(|(at_column, at_row, _)| *at_column == column && *at_row == row)
            .map(|(_, _, value)| value)
    }

    /// Every stored coefficient of one row, ascending by column.
    #[must_use]
    pub fn row_entries(&self, row: usize) -> Vec<(usize, Rational)> {
        let mut out: Vec<(usize, Rational)> = self
            .entries
            .iter()
            .filter(|(_, at, _)| *at == row)
            .map(|(column, _, value)| (*column, value.clone()))
            .collect();
        out.sort_by_key(|(column, _)| *column);
        out
    }

    /// Every stored coefficient of one column, ascending by row.
    #[must_use]
    pub fn column_entries(&self, column: usize) -> Vec<(usize, Rational)> {
        let mut out: Vec<(usize, Rational)> = self
            .entries
            .iter()
            .filter(|(at, _, _)| *at == column)
            .map(|(_, row, value)| (*row, value.clone()))
            .collect();
        out.sort_by_key(|(row, _)| *row);
        out
    }

    /// Removes a column and renumbers the ones after it.
    pub fn drop_column(&mut self, column: usize) {
        self.variables.remove(column);
        self.entries.retain(|(at, _, _)| *at != column);
        for (at, _, _) in &mut self.entries {
            if *at > column {
                *at -= 1;
            }
        }
    }

    /// Removes a row and renumbers the ones after it.
    pub fn drop_row(&mut self, row: usize) {
        self.rows.remove(row);
        self.entries.retain(|(_, at, _)| *at != row);
        for (_, at, _) in &mut self.entries {
            if *at > row {
                *at -= 1;
            }
        }
    }

    /// Puts the model back together.
    ///
    /// The two notes a parser leaves on a model — where duplicate entries
    /// cancelled, and which free rows were discarded — describe how the file
    /// was read, not what the model contains. A reduced model is not the file,
    /// so they are not carried forward and the reduced dump does not mention
    /// them.
    #[must_use]
    pub fn finish(mut self) -> Model {
        self.entries.retain(|(_, _, value)| !value.is_zero());
        self.entries.sort_by_key(|(column, row, _)| (*column, *row));
        let entries = self
            .entries
            .into_iter()
            .map(|(column, row, value)| (VarId(column as u32), RowId(row as u32), value))
            .collect();
        Model::new(ModelParts {
            format: self.format,
            digest: self.digest,
            original_sense: self.sense,
            objective_constant: self.constant,
            variables: self.variables,
            rows: self.rows,
            entries,
            zero_sums: Vec::new(),
            discarded: Vec::new(),
        })
    }
}

/// The tighter of two optional lower bounds.
#[must_use]
pub fn tighter_lower(current: Option<&Rational>, candidate: &Rational) -> Rational {
    match current {
        Some(value) if value.cmp_value(candidate).is_ge() => value.clone(),
        _ => candidate.clone(),
    }
}

/// The tighter of two optional upper bounds.
#[must_use]
pub fn tighter_upper(current: Option<&Rational>, candidate: &Rational) -> Rational {
    match current {
        Some(value) if value.cmp_value(candidate).is_le() => value.clone(),
        _ => candidate.clone(),
    }
}
