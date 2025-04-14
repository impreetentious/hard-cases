//! Rendering a proof as a tree somebody can read.
//!
//! Everything printed here comes out of a certificate's records. That is the
//! whole design constraint: the explanation is a rendering of the proof, not a
//! rendering of what the search remembered, so a line in it is something the
//! verifier will have checked or will refuse. A tree drawn from solver state
//! would be a picture of an intention.
//!
//! The output is ASCII, and deliberately so. It is read in terminals, pasted
//! into issues, and diffed between versions, and every one of those goes better
//! without drawing characters. The shape is the ordinary one: a node's children
//! are indented beneath it, the last child closes its branch, and the vertical
//! bars mark the ancestors that still have siblings to come.

use core::fmt::Write as _;

use hc_exact::Rational;

use crate::hcc::{Certificate, ClaimKind, Record};

/// What one node of the rendered tree says.
struct Line {
    node: u64,
    children: Vec<u64>,
    detail: String,
}

/// Renders a certificate's tree.
///
/// A certificate with no tree renders its claim and nothing else, because
/// there is nothing else in it: a linear program's proof is a bound, a point,
/// and the statement they support.
#[must_use]
pub fn tree(certificate: &Certificate) -> String {
    let mut lines: Vec<Line> = vec![Line {
        node: 0,
        children: Vec::new(),
        detail: String::from("root"),
    }];

    // The inference each record established, so a leaf can be described by
    // what it rests on rather than by the number of the record it cites.
    let mut bounds: Vec<Option<(u64, String)>> = Vec::new();

    for record in &certificate.records {
        match record {
            Record::DualBound { node, terms } => {
                bounds.push(Some((*node, format!("{} multipliers", terms.len()))));
                continue;
            }
            Record::Contradiction { node, terms } => {
                bounds.push(Some((*node, format!("{} multipliers", terms.len()))));
                continue;
            }
            _ => bounds.push(None),
        }

        match record {
            Record::Branch {
                parent,
                variable,
                split,
                left,
                right,
            } => {
                say(
                    &mut lines,
                    *parent,
                    format!("branch on x{variable}: <= {split} | >= {}", plus_one(split)),
                );
                add(
                    &mut lines,
                    *parent,
                    *left,
                    format!("x{variable} <= {split}"),
                );
                add(
                    &mut lines,
                    *parent,
                    *right,
                    format!("x{variable} >= {}", plus_one(split)),
                );
            }
            Record::LeafBound { node, bound } => {
                let cited = describe(&bounds, *bound);
                say(&mut lines, *node, format!("closed by a bound ({cited})"));
            }
            Record::LeafInfeas {
                node,
                contradiction,
            } => {
                let cited = describe(&bounds, *contradiction);
                say(
                    &mut lines,
                    *node,
                    format!("closed by a contradiction ({cited})"),
                );
            }
            Record::LeafIntegral { node, point, bound } => {
                let cited = describe(&bounds, *bound);
                say(
                    &mut lines,
                    *node,
                    format!(
                        "closed by an integral point [{}] attaining its bound ({cited})",
                        join(point)
                    ),
                );
            }
            Record::LeafFrontier { node, bound } => {
                let cited = describe(&bounds, *bound);
                say(&mut lines, *node, format!("OPEN with a bound ({cited})"));
            }
            _ => {}
        }
    }

    let mut out = String::new();
    // The walk is iterative, with an explicit stack, for the same reason the
    // verifier's replay is: a legal tree may be deeper than the call stack.
    let mut stack: Vec<(u64, Vec<bool>)> = vec![(0, Vec::new())];
    while let Some((node, ancestry)) = stack.pop() {
        let Some(line) = lines.iter().find(|line| line.node == node) else {
            continue;
        };
        out.push_str(&prefix(&ancestry));
        let _ = writeln!(out, "node {node}  {}", line.detail);

        let children = line.children.clone();
        for (position, child) in children.iter().enumerate().rev() {
            let mut deeper = ancestry.clone();
            deeper.push(position + 1 == children.len());
            stack.push((*child, deeper));
        }
    }

    let _ = writeln!(out, "{}", claim(certificate));
    out
}

/// The claim line, in the terminal vocabulary.
fn claim(certificate: &Certificate) -> String {
    let Some(Record::Claim {
        kind,
        objective,
        lower,
        gap,
    }) = certificate
        .records
        .iter()
        .find(|record| matches!(record, Record::Claim { .. }))
    else {
        return String::from("claim: none");
    };

    let word = match kind {
        ClaimKind::Optimal => "optimal",
        ClaimKind::Infeasible => "infeasible",
        ClaimKind::Unbounded => "unbounded",
        ClaimKind::FeasibleWithGap => "feasible with a certified gap",
        ClaimKind::FeasibleNoBound => "feasible, no bound claimed",
    };
    let mut out = format!("claim: {word}");
    if let Some(value) = objective {
        let _ = write!(out, "  objective {value}");
    }
    if let Some(value) = lower {
        let _ = write!(out, "  bound {value}");
    }
    if let Some(value) = gap {
        let _ = write!(out, "  gap {value}");
    }
    out
}

/// The indentation for a node at a given ancestry.
///
/// Each entry says whether that ancestor was the last of its siblings; a
/// last-child ancestor contributes blank space and any other contributes a
/// bar, which is what makes the vertical lines stop where the subtree does.
fn prefix(ancestry: &[bool]) -> String {
    let mut out = String::new();
    for (position, last) in ancestry.iter().enumerate() {
        let deepest = position + 1 == ancestry.len();
        out.push_str(match (deepest, last) {
            (true, true) => "`-- ",
            (true, false) => "|-- ",
            (false, true) => "    ",
            (false, false) => "|   ",
        });
    }
    out
}

/// Attaches a description to a node, keeping whatever it already had.
fn say(lines: &mut [Line], node: u64, detail: String) {
    if let Some(line) = lines.iter_mut().find(|line| line.node == node) {
        if line.detail == "root" || line.detail.is_empty() {
            line.detail = detail;
        } else {
            line.detail.push_str("; ");
            line.detail.push_str(&detail);
        }
    }
}

/// Creates a child under a parent.
fn add(lines: &mut Vec<Line>, parent: u64, child: u64, detail: String) {
    if let Some(line) = lines.iter_mut().find(|line| line.node == parent) {
        line.children.push(child);
    }
    lines.push(Line {
        node: child,
        children: Vec::new(),
        detail,
    });
}

/// How a cited inference is described.
fn describe(bounds: &[Option<(u64, String)>], cited: u64) -> String {
    match bounds.get(usize::try_from(cited).unwrap_or(usize::MAX)) {
        Some(Some((_, detail))) => detail.clone(),
        _ => format!("record {cited}"),
    }
}

/// One more than a split value, which is the other child's assumption.
fn plus_one(value: &Rational) -> Rational {
    value.add(&Rational::one())
}

/// A point, as a comma-separated list.
fn join(point: &[Rational]) -> String {
    point
        .iter()
        .map(Rational::to_canonical_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Lists the records a certificate is made of, one to a line.
///
/// The tree above is the argument; this is the material. They are separate
/// because a reader checking that a proof cites what it claims to cite wants
/// the records in the order the verifier will read them, with their identifiers
/// and their sizes, and not a picture.
///
/// Nothing here is summarised away. A record with fifty multipliers says fifty
/// rather than showing three of them, because a listing that quietly elided
/// would be a listing somebody could be misled by.
#[must_use]
pub fn records(certificate: &Certificate) -> String {
    let mut out = String::from("records\n");
    for (position, record) in certificate.records.iter().enumerate() {
        let detail = match record {
            Record::Transform {
                rule, parameters, ..
            } => format!(
                "transform          rule {rule}, {} parameter(s)",
                parameters.len()
            ),
            Record::TransformContradiction { rule, sources } => format!(
                "transform-empty    rule {rule}, from {} inequality(s)",
                sources.len()
            ),
            Record::Root { node, .. } => format!("root               node {node}"),
            Record::DualBound { node, terms } => format!(
                "dual-bound         node {node}, {} multiplier(s)",
                terms.len()
            ),
            Record::Contradiction { node, terms } => format!(
                "contradiction      node {node}, {} multiplier(s)",
                terms.len()
            ),
            Record::GmiCut {
                scope,
                combination,
                isolated,
                coefficients,
                ..
            } => format!(
                "cut-gomory         scope {scope}, from {} row(s), isolating column {isolated}, {} coefficient(s)",
                combination.len(),
                coefficients.len()
            ),
            Record::CoverCut {
                scope,
                source,
                cover,
                coefficients,
                ..
            } => format!(
                "cut-cover          scope {scope}, from inequality {source}, cover of {}, {} coefficient(s)",
                cover.len(),
                coefficients.len()
            ),
            Record::Branch {
                parent,
                variable,
                split,
                ..
            } => format!(
                "branch             node {parent} on column {variable} at {}",
                split.to_canonical_string()
            ),
            Record::LeafBound { node, .. } => format!("leaf-bound         node {node}"),
            Record::LeafInfeas { node, .. } => format!("leaf-infeasible    node {node}"),
            Record::LeafIntegral { node, .. } => format!("leaf-integral      node {node}"),
            Record::LeafFrontier { node, .. } => format!("leaf-frontier      node {node}"),
            Record::Incumbent {
                original,
                objective,
                ..
            } => format!(
                "incumbent          {} component(s), attaining {}",
                original.len(),
                objective.to_canonical_string()
            ),
            Record::FeasiblePoint { point, .. } => {
                format!("feasible-point     {} component(s)", point.len())
            }
            Record::Ray { direction, .. } => {
                format!("ray                {} component(s)", direction.len())
            }
            Record::Claim { kind, .. } => format!("claim              kind {}", kind.code()),
            Record::End { counts } => {
                format!("end                {} record kind(s) counted", counts.len())
            }
        };
        let _ = writeln!(out, "  {position:>4}  {detail}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use hc_exact::BigInt;

    fn whole(value: i64) -> Rational {
        Rational::from_i64(value)
    }

    fn certificate(records: Vec<Record>) -> Certificate {
        Certificate {
            format_tag: 1,
            instance_bytes: 0,
            digest: [0; 32],
            claim: ClaimKind::Optimal,
            records: Certificate::terminated(records),
        }
    }

    #[test]
    fn a_root_only_proof_renders_one_node_and_its_claim() {
        let rendered = tree(&certificate(vec![
            Record::Root {
                node: 0,
                model: [0; 32],
            },
            Record::DualBound {
                node: 0,
                terms: vec![(0, whole(1))],
            },
            Record::LeafIntegral {
                node: 0,
                point: vec![whole(4)],
                bound: 1,
            },
            Record::Claim {
                kind: ClaimKind::Optimal,
                objective: Some(whole(-4)),
                lower: None,
                gap: None,
            },
        ]));

        assert!(rendered.is_ascii());
        assert!(rendered.starts_with("node 0  closed by an integral point [4]"));
        assert!(rendered.contains("claim: optimal  objective -4"));
        assert_eq!(rendered.lines().count(), 2);
    }

    #[test]
    fn a_branched_proof_indents_its_children_and_closes_the_last_branch() {
        let rendered = tree(&certificate(vec![
            Record::Root {
                node: 0,
                model: [0; 32],
            },
            Record::Branch {
                parent: 0,
                variable: 0,
                split: whole(2),
                left: 1,
                right: 2,
            },
            Record::DualBound {
                node: 1,
                terms: vec![(0, whole(1))],
            },
            Record::LeafBound { node: 1, bound: 2 },
            Record::Contradiction {
                node: 2,
                terms: vec![(0, whole(1)), (1, whole(1))],
            },
            Record::LeafInfeas {
                node: 2,
                contradiction: 4,
            },
            Record::Claim {
                kind: ClaimKind::Optimal,
                objective: Some(whole(-4)),
                lower: None,
                gap: None,
            },
        ]));

        let lines: Vec<&str> = rendered.lines().collect();
        assert_eq!(lines.len(), 4);
        assert!(lines[0].starts_with("node 0  branch on x0: <= 2 | >= 3"));
        assert!(lines[1].starts_with("|-- node 1  "));
        assert!(lines[1].contains("closed by a bound"));
        assert!(lines[2].starts_with("`-- node 2  "));
        assert!(lines[2].contains("closed by a contradiction"));
        assert!(lines[3].starts_with("claim: optimal"));
    }

    #[test]
    fn a_deeper_tree_draws_bars_only_where_a_sibling_is_still_coming() {
        let rendered = tree(&certificate(vec![
            Record::Root {
                node: 0,
                model: [0; 32],
            },
            Record::Branch {
                parent: 0,
                variable: 0,
                split: whole(1),
                left: 1,
                right: 2,
            },
            Record::Branch {
                parent: 1,
                variable: 1,
                split: whole(0),
                left: 3,
                right: 4,
            },
            Record::Claim {
                kind: ClaimKind::FeasibleWithGap,
                objective: Some(whole(-1)),
                lower: Some(whole(-5)),
                gap: Some(
                    Rational::new(BigInt::from_i64(4), BigInt::from_i64(1)).expect("an integer"),
                ),
            },
        ]));

        let lines: Vec<&str> = rendered.lines().collect();
        // Node 1 has a sibling still to come, so its subtree carries a bar;
        // node 2 is last, so nothing beneath it would.
        assert!(lines[1].starts_with("|-- node 1"));
        assert!(lines[2].starts_with("|   |-- node 3"));
        assert!(lines[3].starts_with("|   `-- node 4"));
        assert!(lines[4].starts_with("`-- node 2"));
        assert!(rendered.contains("gap 4"));
    }

    #[test]
    fn a_certificate_with_no_claim_says_so_rather_than_inventing_one() {
        let rendered = tree(&certificate(vec![Record::Root {
            node: 0,
            model: [0; 32],
        }]));
        assert!(rendered.contains("claim: none"));
    }
}
