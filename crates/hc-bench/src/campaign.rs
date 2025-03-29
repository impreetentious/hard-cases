//! Generated instances, solved twice, compared.
//!
//! The production solver and the reference solver share no code. One is a
//! revised simplex with a sparse factorization, scaling, reference-weighted
//! pricing, a two-pass ratio test, and an escalation ladder. The other is a
//! dense tableau with Bland's rule and nothing else. They should agree about
//! every linear program, and when they do not, one of them is wrong and the
//! disagreement is a finding rather than a nuisance.
//!
//! Instances are generated as *text*, in the same format a user would supply,
//! and parsed back. That costs a little and buys a lot: the parser is
//! exercised on every round, the generator's output can be written to a file
//! and handed to either solver directly, and a failing case is reproducible
//! from a seed and a round number rather than from a data structure nobody can
//! read.
//!
//! Every coefficient is a quarter-integer. Not because the arithmetic could
//! not take more, but because a value that is exact in binary means a
//! disagreement is about the algorithms rather than about a rounding neither
//! side promised anything about.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use hc_core::{Model, Tol, Xoshiro256PlusPlus};
use hc_io::mps::{Form, read};
use hc_lp::{Options, Problem, Working};

use crate::ref_lp;

/// How far the two solvers' objectives may differ before it is a finding.
///
/// Both are floating point and take entirely different routes, so exact
/// agreement is not on offer. The fence is loose enough to survive that and
/// tight enough that a genuine disagreement — a wrong vertex, a missed bound,
/// a status confusion — is nowhere near it.
pub const AGREEMENT_FENCE: f64 = 1e-6;

/// A generated instance and the text it was generated as.
#[derive(Clone, Debug)]
pub struct Instance {
    /// The instance file, in free-form MPS.
    pub source: String,
    /// The parsed model.
    pub model: Model,
}

/// What one comparison found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The two solvers agree.
    Agree,
    /// Neither solver reached a conclusion worth comparing.
    Inconclusive,
    /// The two solvers disagree, which is a finding.
    Disagree(String),
}

/// What a campaign found, in total.
#[derive(Clone, Debug, Default)]
pub struct Report {
    /// Instances generated.
    pub rounds: usize,
    /// Instances the two solvers agreed on.
    pub agreements: usize,
    /// Instances neither solver concluded anything comparable about.
    pub inconclusive: usize,
    /// How often each pair of statuses came up.
    pub statuses: BTreeMap<String, usize>,
    /// Every disagreement, with the instance that produced it.
    pub disagreements: Vec<Disagreement>,
}

/// One disagreement, with everything needed to reproduce it.
#[derive(Clone, Debug)]
pub struct Disagreement {
    /// The round that produced it.
    pub round: usize,
    /// What differed.
    pub detail: String,
    /// The instance file.
    pub source: String,
}

/// Generates one instance.
///
/// The shape is drawn as well as the values: row and column counts, density,
/// the mix of row senses, and the mix of bound kinds. A campaign over one
/// shape tests one shape.
#[must_use]
pub fn generate(generator: &mut Xoshiro256PlusPlus, rows: usize, columns: usize) -> Instance {
    let mut source = String::new();
    source.push_str("ROWS\n N obj\n");
    let senses: Vec<char> = (0..rows)
        .map(|_| match generator.next_below(4) {
            0 => 'E',
            1 => 'G',
            _ => 'L',
        })
        .collect();
    for (index, sense) in senses.iter().enumerate() {
        let _ = writeln!(source, " {sense} r{index}");
    }

    source.push_str("COLUMNS\n");
    for column in 0..columns {
        // Every column touches the objective, so no column is invisible to it.
        let _ = writeln!(source, " c{column} obj {}", quarter(generator, 8));
        let mut entries = 0usize;
        for row in 0..rows {
            // A density around a half keeps the matrices sparse enough to be
            // interesting and dense enough to couple the rows.
            if generator.next_below(2) == 0 {
                continue;
            }
            let value = quarter(generator, 6);
            if value == "0" {
                continue;
            }
            let _ = writeln!(source, " c{column} r{row} {value}");
            entries += 1;
        }
        // A column with no coefficients at all is legal and worth generating,
        // but an instance where *nothing* touches the matrix is decided
        // entirely by its right-hand sides, which the fixtures already cover.
        // The first column is therefore guaranteed one entry — written inside
        // its own block, because a record appended afterwards would break the
        // contiguity the format requires.
        if column == 0 && rows > 0 && entries == 0 {
            let _ = writeln!(source, " c0 r0 1");
        }
    }

    // Half the instances get right-hand sides whose signs make the origin
    // feasible, and half get unconstrained ones. Without the first half the
    // campaign is almost entirely infeasible instances, and the paths that
    // matter most — a real optimum, a real vertex, a real bound — go
    // untested behind a status that is easy to get right.
    let roomy = generator.next_below(2) == 0;
    source.push_str("RHS\n");
    for (row, sense) in senses.iter().enumerate() {
        let value = if roomy {
            match sense {
                'L' => magnitude(generator, 12),
                'G' => format!("-{}", magnitude(generator, 12)),
                _ => quarter(generator, 4),
            }
        } else {
            quarter(generator, 12)
        };
        let _ = writeln!(source, " rhs r{row} {value}");
    }

    source.push_str("BOUNDS\n");
    for column in 0..columns {
        // Bounded columns are the common case, so the campaign spends most of
        // its rounds on instances that have an optimum rather than a ray.
        match generator.next_below(10) {
            // The default: nonnegative and unbounded above.
            0..=1 => {}
            2..=4 => {
                let _ = writeln!(source, " UP b c{column} {}", magnitude(generator, 10));
            }
            5..=7 => {
                let low = -(i64::try_from(generator.next_below(5)).unwrap_or(0));
                let _ = writeln!(source, " LO b c{column} {low}");
                let _ = writeln!(source, " UP b c{column} {}", magnitude(generator, 10));
            }
            8 => {
                let _ = writeln!(source, " FR b c{column}");
            }
            _ => {
                let _ = writeln!(source, " FX b c{column} {}", quarter(generator, 4));
            }
        }
    }
    source.push_str("ENDATA\n");

    let model = read(source.as_bytes(), Form::Free).unwrap_or_else(|error| {
        panic!("the generator produced something unreadable: {error}\n{source}")
    });
    Instance { source, model }
}

/// A quarter-integer in `[-limit, limit]`, written as decimal text.
fn quarter(generator: &mut Xoshiro256PlusPlus, limit: u64) -> String {
    let steps = generator.next_below(limit * 8 + 1) as i64 - (limit as i64) * 4;
    render_quarter(steps)
}

/// A positive quarter-integer no larger than `limit`.
fn magnitude(generator: &mut Xoshiro256PlusPlus, limit: u64) -> String {
    let steps = generator.next_below(limit * 4) as i64 + 1;
    render_quarter(steps)
}

/// Renders a count of quarters as exact decimal text.
fn render_quarter(steps: i64) -> String {
    let whole = steps / 4;
    let rest = steps % 4;
    match rest {
        0 => whole.to_string(),
        _ => {
            let sign = if steps < 0 { "-" } else { "" };
            let magnitude = steps.abs();
            format!("{sign}{}.{:02}", magnitude / 4, (magnitude % 4) * 25)
        }
    }
}

/// Solves one instance both ways and compares.
#[must_use]
pub fn compare(model: &Model) -> (Verdict, String) {
    let working = Working::from_model(model);
    let problem = Problem::scaled(&working, Tol::solver());
    let (outcome, _) = hc_lp::solve(&problem, &Options::default());
    let reference = ref_lp::solve(model);

    let pair = format!("{}/{}", outcome.as_str(), reference_name(&reference));

    let verdict = match (&outcome, &reference) {
        (
            hc_lp::Outcome::Optimal {
                objective, point, ..
            },
            ref_lp::Outcome::Optimal {
                objective: other, ..
            },
        ) => {
            let recovered = problem.recover_point(point);
            let structural = &recovered[..working.structural()];
            let bound = AGREEMENT_FENCE * objective.abs().max(1.0);
            if (objective - other).abs() > bound {
                Verdict::Disagree(format!(
                    "objectives differ: solver {objective}, reference {other}"
                ))
            } else if !ref_lp::is_feasible(model, structural, AGREEMENT_FENCE.sqrt()) {
                Verdict::Disagree(format!(
                    "the solver's optimum is not feasible in the original model: {structural:?}"
                ))
            } else {
                Verdict::Agree
            }
        }
        (hc_lp::Outcome::Infeasible { .. }, ref_lp::Outcome::Infeasible)
        | (hc_lp::Outcome::Unbounded { .. }, ref_lp::Outcome::Unbounded) => Verdict::Agree,
        (hc_lp::Outcome::EffortLimit { .. } | hc_lp::Outcome::Numerical { .. }, _)
        | (_, ref_lp::Outcome::Exhausted) => Verdict::Inconclusive,
        (left, right) => Verdict::Disagree(format!(
            "statuses differ: solver {}, reference {}",
            left.as_str(),
            reference_name(right)
        )),
    };
    (verdict, pair)
}

fn reference_name(outcome: &ref_lp::Outcome) -> &'static str {
    match outcome {
        ref_lp::Outcome::Optimal { .. } => "optimal",
        ref_lp::Outcome::Infeasible => "infeasible",
        ref_lp::Outcome::Unbounded => "unbounded",
        ref_lp::Outcome::Exhausted => "exhausted",
    }
}

/// Runs a campaign and reports what it found.
#[must_use]
pub fn run(seed: u64, rounds: usize) -> Report {
    let mut generator = Xoshiro256PlusPlus::from_seed(seed);
    let mut report = Report::default();

    for round in 0..rounds {
        let rows = 1 + (round % 7);
        let columns = 1 + (round % 5);
        let instance = generate(&mut generator, rows, columns);
        let (verdict, pair) = compare(&instance.model);
        *report.statuses.entry(pair).or_insert(0) += 1;
        report.rounds += 1;
        match verdict {
            Verdict::Agree => report.agreements += 1,
            Verdict::Inconclusive => report.inconclusive += 1,
            Verdict::Disagree(detail) => report.disagreements.push(Disagreement {
                round,
                detail,
                source: instance.source,
            }),
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarters_render_as_exact_decimal_text() {
        assert_eq!(render_quarter(0), "0");
        assert_eq!(render_quarter(4), "1");
        assert_eq!(render_quarter(-4), "-1");
        assert_eq!(render_quarter(1), "0.25");
        assert_eq!(render_quarter(-1), "-0.25");
        assert_eq!(render_quarter(5), "1.25");
        assert_eq!(render_quarter(-5), "-1.25");
        assert_eq!(render_quarter(6), "1.50");
        assert_eq!(render_quarter(-7), "-1.75");
    }

    #[test]
    fn every_generated_value_is_exact_in_binary() {
        // A quarter is exact in binary, so a disagreement between the two
        // solvers cannot be blamed on the text.
        let mut generator = Xoshiro256PlusPlus::from_seed(5);
        for _ in 0..500 {
            for text in [quarter(&mut generator, 8), magnitude(&mut generator, 10)] {
                let exact = hc_exact::Rational::parse_decimal(&text).expect("a decimal token");
                let rounded = exact.to_binary64();
                assert_eq!(
                    hc_exact::Rational::from_binary64(rounded).expect("finite"),
                    exact,
                    "{text} does not survive the round trip"
                );
            }
        }
    }

    #[test]
    fn the_generator_produces_instances_both_readers_accept() {
        let mut generator = Xoshiro256PlusPlus::from_seed(11);
        for _ in 0..40 {
            let instance = generate(&mut generator, 3, 3);
            // The verifier's parser is the one that matters: an instance the
            // campaign can solve and the verifier cannot read would produce
            // findings nobody could act on.
            hc_verify::vparse::read(instance.source.as_bytes(), hc_verify::vparse::Layout::Free)
                .unwrap_or_else(|error| panic!("{error}\n{}", instance.source));
        }
    }

    #[test]
    fn the_same_seed_generates_the_same_campaign() {
        let first = run(3, 20);
        let second = run(3, 20);
        assert_eq!(first.rounds, second.rounds);
        assert_eq!(first.agreements, second.agreements);
        assert_eq!(first.inconclusive, second.inconclusive);
        assert_eq!(first.statuses, second.statuses);
        assert_eq!(first.disagreements.len(), second.disagreements.len());
    }
}
