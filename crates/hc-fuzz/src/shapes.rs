//! Instances that are almost right, in every format at once.
//!
//! A byte-level fuzzer spends nearly all of its budget being refused by the
//! first fence it meets. The interesting inputs are the ones that get past
//! that: a file inside the grammar, or one character outside it. So the corpus
//! starts from a small abstract model and renders it, with the spelling
//! choices each format leaves open picked from the same seed.
//!
//! Rendering one model into all four accepted readings buys a property nothing
//! else here checks: the four files must produce the same canonical model. The
//! digest differs, because the bytes and the tag differ, and everything below
//! it must not.

use hc_core::Xoshiro256PlusPlus as Rng;

/// One variable of a drafted model.
pub struct Column {
    /// The label, short enough for the fixed-form windows.
    pub name: String,
    /// The lower bound, absent when the draft is unbounded below.
    pub low: Option<String>,
    /// The upper bound, absent when the draft is unbounded above.
    pub high: Option<String>,
    /// Whether the draft declares it integer.
    pub whole: bool,
    /// The objective coefficient, in the direction the draft states.
    pub cost: String,
}

/// One row of a drafted model.
pub struct Row {
    /// The label.
    pub name: String,
    /// The lower side.
    pub low: Option<String>,
    /// The upper side.
    pub high: Option<String>,
    /// The coefficients, by column index, in the order they were drafted.
    pub terms: Vec<(usize, String)>,
}

/// A model before it is written down.
pub struct Draft {
    /// Whether the file states a maximizing objective.
    pub max: bool,
    /// The objective constant, in the direction the draft states.
    pub constant: String,
    /// The columns.
    pub columns: Vec<Column>,
    /// The rows.
    pub rows: Vec<Row>,
}

/// The values a draft is built from.
///
/// Short decimals, halves and tenths among them, so the exact path is
/// exercised rather than only the integers a generator reaches for first.
const VALUES: &[&str] = &[
    "0", "1", "-1", "2", "-3", "4", "10", "0.5", "-0.5", "0.1", "-0.25", "1.5", "2.5", "-7", "100",
];

fn pick<'a>(rng: &mut Rng, from: &[&'a str]) -> &'a str {
    from[rng.next_below(from.len() as u64) as usize]
}

/// Draws a model.
#[must_use]
pub fn draft(seed: u64) -> Draft {
    let mut rng = Rng::from_seed(seed);
    let columns = 1 + rng.next_below(4) as usize;
    let rows = 1 + rng.next_below(4) as usize;

    let mut drawn = Vec::with_capacity(columns);
    for index in 0..columns {
        // The two sides are drawn in order so they can never cross, because a
        // crossed pair is a model two of the three formats cannot write.
        let low = match rng.next_below(4) {
            0 => None,
            _ => Some(pick(&mut rng, &["0", "-1", "-10", "1"]).to_owned()),
        };
        let high = match rng.next_below(3) {
            0 => None,
            _ => Some(pick(&mut rng, &["1", "4", "10", "100"]).to_owned()),
        };
        let (low, high) = match (low, high) {
            (Some(low), Some(high)) if low.starts_with('-') || low == "0" => {
                (Some(low), Some(high))
            }
            // A lower bound above every upper bound this generator draws would
            // cross, so the pair is opened instead.
            (Some(low), Some(_)) => (Some(low), None),
            pair => pair,
        };
        drawn.push(Column {
            name: format!("x{index}"),
            low,
            high,
            whole: rng.next_below(3) == 0,
            cost: pick(&mut rng, VALUES).to_owned(),
        });
    }

    let mut built = Vec::with_capacity(rows);
    for index in 0..rows {
        let mut terms = Vec::new();
        for column in 0..columns {
            match rng.next_below(4) {
                0 => {}
                1 => {
                    // A pair that cancels: the same position twice, opposite
                    // signs. The canonical dump records it, so every format
                    // has to carry it through.
                    terms.push((column, "3".to_owned()));
                    terms.push((column, "-3".to_owned()));
                }
                _ => terms.push((column, pick(&mut rng, VALUES).to_owned())),
            }
        }
        if terms.is_empty() {
            terms.push((0, "1".to_owned()));
        }
        let (low, high) = match rng.next_below(4) {
            0 => (Some("1".to_owned()), None),
            1 => (None, Some("10".to_owned())),
            2 => (Some("2".to_owned()), Some("2".to_owned())),
            _ => (Some("0".to_owned()), Some("5".to_owned())),
        };
        built.push(Row {
            name: format!("r{index}"),
            low,
            high,
            terms,
        });
    }

    Draft {
        max: rng.next_below(2) == 0,
        constant: pick(&mut rng, &["0", "0", "5", "-2.5"]).to_owned(),
        columns: drawn,
        rows: built,
    }
}

/// Which reading a rendering is meant for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// MPS read from its column windows.
    MpsFixed,
    /// MPS with whitespace-delimited fields.
    MpsFree,
    /// The accepted LP-format subset.
    Lp,
    /// The JSON instance schema.
    Json,
}

impl Shape {
    /// Every shape, in tag order.
    pub const ALL: [Self; 4] = [Self::MpsFixed, Self::MpsFree, Self::Lp, Self::Json];

    /// The name a report uses.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MpsFixed => "mps-fixed",
            Self::MpsFree => "mps-free",
            Self::Lp => "lp",
            Self::Json => "json",
        }
    }
}

/// Renders a draft in one reading.
#[must_use]
pub fn render(draft: &Draft, shape: Shape, seed: u64) -> String {
    match shape {
        Shape::MpsFixed => mps(draft, true),
        Shape::MpsFree => mps(draft, false),
        Shape::Lp => lp(draft, seed),
        Shape::Json => json(draft),
    }
}

/// A field padded into its fixed-form window, or separated by a blank.
fn field(out: &mut String, text: &str, width: usize, fixed: bool) {
    out.push_str(text);
    if fixed {
        for _ in text.len()..width {
            out.push(' ');
        }
    } else {
        out.push(' ');
    }
}

fn mps(draft: &Draft, fixed: bool) -> String {
    let mut out = String::from("NAME          DRAFT\n");
    if draft.max {
        out.push_str("OBJSENSE\n    MAX\n");
    }
    out.push_str("ROWS\n");
    out.push_str(" N  obj\n");
    for row in &draft.rows {
        let sense = match (row.low.as_deref(), row.high.as_deref()) {
            (Some(low), Some(high)) if low == high => 'E',
            (Some(_), _) => 'G',
            (None, _) => 'L',
        };
        out.push(' ');
        out.push(sense);
        out.push_str("  ");
        out.push_str(&row.name);
        out.push('\n');
    }

    out.push_str("COLUMNS\n");
    for (index, column) in draft.columns.iter().enumerate() {
        if column.whole {
            let _ = core::fmt::Write::write_fmt(
                &mut out,
                format_args!("    MARK{index}     'MARKER'                 'INTORG'\n"),
            );
        }
        let mut record = |row: &str, value: &str| {
            out.push_str("    ");
            field(&mut out, &column.name, 10, fixed);
            field(&mut out, row, 10, fixed);
            out.push_str(value);
            out.push('\n');
        };
        record("obj", &column.cost);
        for row in &draft.rows {
            for (at, value) in &row.terms {
                if *at == index {
                    record(&row.name, value);
                }
            }
        }
        if column.whole {
            let _ = core::fmt::Write::write_fmt(
                &mut out,
                format_args!("    MARK{index}     'MARKER'                 'INTEND'\n"),
            );
        }
    }

    out.push_str("RHS\n");
    if draft.constant != "0" {
        // A right-hand side on the objective row is the negation of the
        // constant, whichever direction the file states: the reader negates it
        // here, and then negates the whole constant again at the end when the
        // file maximizes, which is the same second negation the other two
        // formats apply to the constant they carry directly.
        let stated = negated(&draft.constant);
        out.push_str("    ");
        field(&mut out, "rhs", 10, fixed);
        field(&mut out, "obj", 10, fixed);
        out.push_str(&stated);
        out.push('\n');
    }
    for row in &draft.rows {
        let base = row.low.as_deref().or(row.high.as_deref()).unwrap_or("0");
        out.push_str("    ");
        field(&mut out, "rhs", 10, fixed);
        field(&mut out, &row.name, 10, fixed);
        out.push_str(base);
        out.push('\n');
    }

    let ranged: Vec<&Row> = draft
        .rows
        .iter()
        .filter(|row| matches!((row.low.as_deref(), row.high.as_deref()), (Some(low), Some(high)) if low != high))
        .collect();
    if !ranged.is_empty() {
        out.push_str("RANGES\n");
        for row in ranged {
            let low: i64 = row.low.as_deref().unwrap_or("0").parse().unwrap_or(0);
            let high: i64 = row.high.as_deref().unwrap_or("0").parse().unwrap_or(0);
            out.push_str("    ");
            field(&mut out, "rng", 10, fixed);
            field(&mut out, &row.name, 10, fixed);
            let _ = core::fmt::Write::write_fmt(&mut out, format_args!("{}\n", high - low));
        }
    }

    out.push_str("BOUNDS\n");
    for column in &draft.columns {
        let mut record = |code: &str, value: Option<&str>| {
            out.push(' ');
            out.push_str(code);
            out.push(' ');
            field(&mut out, "bnd", 10, fixed);
            field(&mut out, &column.name, 10, fixed);
            if let Some(value) = value {
                out.push_str(value);
            }
            out.push('\n');
        };
        match (column.low.as_deref(), column.high.as_deref()) {
            (None, None) => record("FR", None),
            (Some(low), None) => record("LO", Some(low)),
            // The lower side is opened before the upper is given, so a
            // negative upper bound never meets the default lower bound.
            (None, Some(high)) => {
                record("MI", None);
                record("UP", Some(high));
            }
            (Some(low), Some(high)) => {
                record("LO", Some(low));
                record("UP", Some(high));
            }
        }
    }
    out.push_str("ENDATA\n");
    out
}

fn lp(draft: &Draft, seed: u64) -> String {
    let mut rng = Rng::from_seed(seed ^ 0x5bf0_3635);
    let mut out = String::new();
    if rng.next_below(2) == 0 {
        out.push_str("\\ a drafted instance\n");
    }
    out.push_str(if draft.max {
        pick(&mut rng, &["Maximize", "MAXIMIZE", "max", "Maximum"])
    } else {
        pick(&mut rng, &["Minimize", "MINIMISE", "min", "Minimum"])
    });
    out.push('\n');

    out.push_str(" obj:");
    for column in &draft.columns {
        signed(&mut out, &column.cost, &column.name);
    }
    if draft.constant != "0" {
        signed(&mut out, &draft.constant, "");
    }
    out.push('\n');

    out.push_str(pick(&mut rng, &["Subject To", "such that", "ST", "s.t."]));
    out.push('\n');
    for row in &draft.rows {
        let mut body = String::new();
        for (index, value) in &row.terms {
            signed(&mut body, value, &draft.columns[*index].name);
            // A statement continued across lines is the fork this format is
            // famous for, so the corpus writes some that way.
            if rng.next_below(4) == 0 {
                body.push_str("\n   ");
            }
        }
        let _ = core::fmt::Write::write_fmt(&mut out, format_args!(" {}:{body}", row.name));
        match (row.low.as_deref(), row.high.as_deref()) {
            (Some(low), Some(high)) if low == high => {
                let _ = core::fmt::Write::write_fmt(&mut out, format_args!(" = {low}\n"));
            }
            (Some(low), Some(high)) => {
                // Written from whichever end the seed says; the two spellings
                // have to produce the same row.
                if rng.next_below(2) == 0 {
                    out.insert_str(out.len() - body.len(), &format!(" {low} <="));
                    let _ = core::fmt::Write::write_fmt(&mut out, format_args!(" <= {high}\n"));
                } else {
                    out.insert_str(out.len() - body.len(), &format!(" {high} >="));
                    let _ = core::fmt::Write::write_fmt(&mut out, format_args!(" >= {low}\n"));
                }
            }
            (Some(low), None) => {
                let _ = core::fmt::Write::write_fmt(&mut out, format_args!(" >= {low}\n"));
            }
            (None, Some(high)) => {
                let _ = core::fmt::Write::write_fmt(&mut out, format_args!(" <= {high}\n"));
            }
            (None, None) => out.push_str(" >= 0\n"),
        }
    }

    out.push_str("Bounds\n");
    let mut binaries = Vec::new();
    for column in &draft.columns {
        let name = &column.name;
        match (column.low.as_deref(), column.high.as_deref()) {
            (None, None) => {
                let _ = core::fmt::Write::write_fmt(&mut out, format_args!(" {name} free\n"));
            }
            (Some(low), None) => {
                let _ = core::fmt::Write::write_fmt(&mut out, format_args!(" {name} >= {low}\n"));
            }
            (None, Some(high)) => {
                let _ = core::fmt::Write::write_fmt(
                    &mut out,
                    format_args!(" -inf <= {name} <= {high}\n"),
                );
            }
            (Some(low), Some(high)) => {
                if column.whole && low == "0" && high == "1" && rng.next_below(2) == 0 {
                    // The binary section sets both sides itself, so the bound
                    // statement has to be left out when it is used.
                    binaries.push(name.as_str());
                } else {
                    let _ = core::fmt::Write::write_fmt(
                        &mut out,
                        format_args!(" {low} <= {name} <= {high}\n"),
                    );
                }
            }
        }
    }

    let generals: Vec<&str> = draft
        .columns
        .iter()
        .filter(|column| column.whole && !binaries.contains(&column.name.as_str()))
        .map(|column| column.name.as_str())
        .collect();
    if !generals.is_empty() {
        out.push_str("General\n");
        for name in generals {
            let _ = core::fmt::Write::write_fmt(&mut out, format_args!(" {name}\n"));
        }
    }
    if !binaries.is_empty() {
        out.push_str("Binary\n");
        for name in binaries {
            let _ = core::fmt::Write::write_fmt(&mut out, format_args!(" {name}\n"));
        }
    }
    out.push_str("End\n");
    out
}

fn json(draft: &Draft) -> String {
    let mut out = String::from("{\n  \"version\": 1,\n");
    let _ = core::fmt::Write::write_fmt(
        &mut out,
        format_args!(
            "  \"sense\": \"{}\",\n  \"objective_constant\": \"{}\",\n",
            if draft.max { "max" } else { "min" },
            draft.constant
        ),
    );
    out.push_str("  \"vars\": [\n");
    let mut first = true;
    for column in &draft.columns {
        if !first {
            out.push_str(",\n");
        }
        first = false;
        let _ = core::fmt::Write::write_fmt(
            &mut out,
            format_args!(
                "    {{\"name\": \"{}\", \"lb\": {}, \"ub\": {}, \"integer\": {}, \"obj\": \"{}\"}}",
                column.name,
                quoted(column.low.as_deref()),
                quoted(column.high.as_deref()),
                column.whole,
                column.cost
            ),
        );
    }
    out.push_str("\n  ],\n  \"rows\": [\n");
    let mut first = true;
    for row in &draft.rows {
        if !first {
            out.push_str(",\n");
        }
        first = false;
        let (sense, base, range) = match (row.low.as_deref(), row.high.as_deref()) {
            (Some(low), Some(high)) if low == high => ("E", low.to_owned(), None),
            (Some(low), Some(high)) => {
                let low_value: i64 = low.parse().unwrap_or(0);
                let high_value: i64 = high.parse().unwrap_or(0);
                (
                    "G",
                    low.to_owned(),
                    Some((high_value - low_value).to_string()),
                )
            }
            (Some(low), None) => ("G", low.to_owned(), None),
            (None, Some(high)) => ("L", high.to_owned(), None),
            (None, None) => ("G", "0".to_owned(), None),
        };
        let _ = core::fmt::Write::write_fmt(
            &mut out,
            format_args!(
                "    {{\"name\": \"{}\", \"sense\": \"{sense}\", \"rhs\": \"{base}\", \"range\": {}}}",
                row.name,
                quoted(range.as_deref())
            ),
        );
    }
    out.push_str("\n  ],\n  \"nonzeros\": [\n");
    let mut entries = Vec::new();
    for (index, row) in draft.rows.iter().enumerate() {
        for (column, value) in &row.terms {
            entries.push(format!("    [{index}, {column}, \"{value}\"]"));
        }
    }
    out.push_str(&entries.join(",\n"));
    out.push_str("\n  ]\n}\n");
    out
}

/// A value as a JSON string, or the null that spells its absence.
fn quoted(value: Option<&str>) -> String {
    value.map_or_else(|| "null".to_owned(), |text| format!("\"{text}\""))
}

/// A term with its sign written separately, as the LP grammar wants it.
fn signed(out: &mut String, value: &str, name: &str) {
    match value.strip_prefix('-') {
        Some(rest) => {
            out.push_str(" - ");
            out.push_str(rest);
        }
        None => {
            out.push_str(" + ");
            out.push_str(value);
        }
    }
    if !name.is_empty() {
        out.push(' ');
        out.push_str(name);
    }
}

/// A decimal with its sign turned around, textually.
fn negated(value: &str) -> String {
    value
        .strip_prefix('-')
        .map_or_else(|| format!("-{value}"), ToOwned::to_owned)
}
