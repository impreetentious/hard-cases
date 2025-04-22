//! The proof tree, drawn.
//!
//! An SVG rather than a screenshot, generated from a real solve rather than
//! captured from one. A screenshot on a front page is a photograph of a
//! program that may since have changed; this is regenerated and diffed by the
//! same test that diffs the scoreboard, so a change to the search that changes
//! the tree is a change that redraws the picture in the same commit.
//!
//! Text rather than a binary blob, for the same reason everything else here is
//! text: a reader can open it and check that the node it says was closed by a
//! contradiction is the node the certificate closes by a contradiction.
//!
//! Colours are chosen to read on a light background and on a dark one, because
//! the page it appears on has both and an SVG cannot reliably ask which.
//! Nothing in it is filled with a background.

use core::fmt::Write as _;

use hc_cert::mip::assemble_presolved;
use hc_core::Model;
use hc_io::mps::{Form, read};
use hc_mip::search::{CUT_ROUNDS, Node, Options, run};
use hc_presolve::reduce;
use hc_verify::vclose::verify;

/// The instance the figure is drawn from.
///
/// Chosen because its five nodes between them show every way this solver knows
/// how to close one: a split, a contradiction, a bound, and an integral point
/// attaining its own bound. A larger tree would be a nicer picture and a worse
/// explanation.
const INSTANCE: &[u8] = include_bytes!("../../../instances/integer/fixcharge6.mps");
/// Its name, for the caption.
const NAME: &str = "fixcharge6.mps";

const COLUMN: f64 = 190.0;
const ROW: f64 = 108.0;
const RADIUS: f64 = 15.0;
const MARGIN: f64 = 34.0;
/// The neutral this figure writes its text in.
///
/// A fixed colour rather than `currentColor`, because the front page renders
/// this through an `img` element, which gives the document no colour to
/// inherit and falls back to black — invisible on the dark theme half the
/// readers are using. This one clears four and a half to one on white and
/// three and a half to one on the dark background, which is the best a single
/// value does for text this size on both.
const INK: &str = "#6e7781";

/// How a node ended, as the figure labels it.
struct Drawn {
    id: u64,
    parent: Option<u64>,
    depth: u32,
    /// The assumption this node adds to its parent's region.
    assumption: Option<String>,
    /// The word under the node.
    outcome: &'static str,
    /// The exact quantity beside it, where it has one.
    exact: Option<String>,
    /// The stroke and fill this outcome is drawn with.
    ink: (&'static str, &'static str),
    x: f64,
    y: f64,
}

/// Renders the figure.
///
/// # Panics
///
/// Panics when the committed instance does not solve and certify, which is a
/// defect rather than a drawing problem and is caught by the corpus gate long
/// before this runs.
#[must_use]
pub fn render() -> String {
    let model: Model = read(INSTANCE, Form::Free).expect("the committed instance parses");
    let chain = reduce(&model);
    let search = run(
        &chain.reduced,
        &Options {
            cut_rounds: CUT_ROUNDS,
            ..Options::default()
        },
    );
    let certificate =
        assemble_presolved(&model, &chain, INSTANCE, &search).expect("a certificate assembles");
    let bytes = certificate.encode();
    let verdict = verify(&bytes, INSTANCE).expect("the standalone verifier accepts it");

    let mut drawn: Vec<Drawn> = Vec::new();
    for entry in search.tree.entries() {
        let (outcome, exact, ink) =
            match search.nodes.get(entry.id as usize).and_then(Option::as_ref) {
                // A split node carries no second line. The two edges leaving it
                // are already labelled with the assumption each child adds, and
                // repeating the threshold under the node puts the same fact in two
                // places a centimetre apart.
                Some(Node::Split { .. }) => ("split", None, ("#b07d2b", "none")),
                Some(Node::Infeasible { .. }) => ("no feasible point", None, ("#8a8f98", "none")),
                Some(Node::Bound { value, .. }) => (
                    "closed by a bound",
                    Some(value.to_canonical_string()),
                    ("#b07d2b", "#b07d2b33"),
                ),
                Some(Node::Integral { value, .. }) => (
                    "integral, attains its bound",
                    Some(value.to_canonical_string()),
                    ("#1f9c92", "#1f9c9255"),
                ),
                Some(Node::Frontier { value, .. }) => (
                    "open, with an exact bound",
                    Some(value.to_canonical_string()),
                    ("#1f9c92", "none"),
                ),
                Some(Node::Suspect { .. } | Node::Unproved { .. }) | None => {
                    ("open", None, ("#8a8f98", "none"))
                }
            };
        drawn.push(Drawn {
            id: entry.id,
            parent: entry.parent,
            depth: entry.depth,
            assumption: entry.assumption.as_ref().map(|assumption| {
                // The column the file named, not the position it sits at. A
                // branch cites a column of the *reduced* model, so the name is
                // taken from that model rather than from the original bytes,
                // which presolve may have removed columns from — the situation
                // room names it the same way and has a test forbidding the
                // index. The figure printed the index, which sent a reader who
                // opened the instance looking for a column that is not in it.
                let named = chain
                    .reduced
                    .variables()
                    .get(assumption.column)
                    .map_or_else(
                        || format!("x{}", assumption.column),
                        |variable| variable.name.clone(),
                    );
                format!(
                    "{named} {} {}",
                    if assumption.at_most { "≤" } else { "≥" },
                    assumption.value.to_canonical_string()
                )
            }),
            outcome,
            exact,
            ink,
            x: 0.0,
            y: 0.0,
        });
    }
    place(&mut drawn);

    let caption = format!(
        "{NAME} · claim {} · objective {} · {} bytes rechecked in {} work units by hc-verify, which links none of the solver",
        verdict.claim.word(),
        verdict
            .objective
            .as_ref()
            .map_or_else(|| "—".to_owned(), ToString::to_string),
        verdict.bytes,
        verdict.work
    );
    // The caption is monospace at twelve points, so its width is its length
    // times the advance of that face. Measured rather than guessed at, because
    // an SVG that clips its own caption is a picture that says less than the
    // text it was supposed to be carrying.
    let caption_width = caption.chars().count() as f64 * 7.24 + MARGIN * 2.0;
    let spread = drawn.iter().map(|node| node.x).fold(0.0_f64, f64::max) + MARGIN + COLUMN / 2.0;
    let width = spread.max(caption_width);
    let height = drawn.iter().map(|node| node.y).fold(0.0_f64, f64::max) + MARGIN + 74.0;

    let mut out = String::new();
    let _ = writeln!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {width:.0} {height:.0}\" width=\"{width:.0}\" height=\"{height:.0}\" font-family=\"ui-monospace, SFMono-Regular, Menlo, monospace\" role=\"img\" aria-label=\"the proof tree of {NAME}\">"
    );
    out.push_str("  <title>The certified proof tree of ");
    out.push_str(NAME);
    out.push_str("</title>\n");

    for node in &drawn {
        let Some(parent) = node.parent.and_then(|id| drawn.iter().find(|d| d.id == id)) else {
            continue;
        };
        let _ = writeln!(
            out,
            "  <path d=\"M {:.1} {:.1} C {:.1} {:.1}, {:.1} {:.1}, {:.1} {:.1}\" fill=\"none\" stroke=\"#8a8f98\" stroke-width=\"1.2\"/>",
            parent.x,
            parent.y + RADIUS,
            parent.x,
            parent.y + ROW / 2.0,
            node.x,
            node.y - ROW / 2.0,
            node.x,
            node.y - RADIUS
        );
        if let Some(assumption) = &node.assumption {
            // Two thirds of the way down the curve rather than beside either
            // end. A label under the parent collides with the parent's own
            // caption, one over the child collides with the child's, and one
            // at the midpoint sits exactly where the caption of a node that
            // was split lands.
            let _ = writeln!(
                out,
                "  <text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"11\" fill=\"#8a8f98\">{}</text>",
                parent.x + (node.x - parent.x) * 0.72,
                parent.y + (node.y - parent.y) * 0.70,
                escaped(assumption)
            );
        }
    }

    for node in &drawn {
        let _ = writeln!(
            out,
            "  <circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"{RADIUS}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"2\"/>",
            node.x, node.y, node.ink.1, node.ink.0
        );
        let _ = writeln!(
            out,
            "  <text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"12\" fill=\"{INK}\">{}</text>",
            node.x,
            node.y + 4.0,
            node.id
        );
        let _ = writeln!(
            out,
            "  <text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"11\" fill=\"{INK}\">{}</text>",
            node.x,
            node.y + RADIUS + 16.0,
            escaped(node.outcome)
        );
        if let Some(exact) = &node.exact {
            let _ = writeln!(
                out,
                "  <text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"11\" fill=\"{}\">{}</text>",
                node.x,
                node.y + RADIUS + 30.0,
                node.ink.0,
                escaped(exact)
            );
        }
    }

    let _ = writeln!(
        out,
        "  <text x=\"{MARGIN:.0}\" y=\"{:.1}\" font-size=\"12\" fill=\"{INK}\">{}</text>",
        height - 16.0,
        escaped(&caption)
    );
    out.push_str("</svg>\n");
    out
}

/// Places every node: depth downwards, and across by leaves.
fn place(drawn: &mut [Drawn]) {
    let ids: Vec<u64> = drawn.iter().map(|node| node.id).collect();
    let mut column = 0.0_f64;
    let mut spots: Vec<(u64, f64)> = Vec::new();

    fn walk(id: u64, drawn: &[Drawn], column: &mut f64, spots: &mut Vec<(u64, f64)>) -> f64 {
        let children: Vec<u64> = drawn
            .iter()
            .filter(|node| node.parent == Some(id))
            .map(|node| node.id)
            .collect();
        let at = if children.is_empty() {
            let here = *column;
            *column += 1.0;
            here
        } else {
            let places: Vec<f64> = children
                .iter()
                .map(|child| walk(*child, drawn, column, spots))
                .collect();
            (places.first().copied().unwrap_or(0.0) + places.last().copied().unwrap_or(0.0)) / 2.0
        };
        spots.push((id, at));
        at
    }

    for id in &ids {
        if drawn
            .iter()
            .any(|node| node.id == *id && node.parent.is_none())
        {
            walk(*id, drawn, &mut column, &mut spots);
        }
    }
    for node in drawn.iter_mut() {
        let at = spots
            .iter()
            .find(|(id, _)| *id == node.id)
            .map_or(0.0, |(_, at)| *at);
        node.x = MARGIN + COLUMN / 2.0 + at * COLUMN;
        node.y = MARGIN + f64::from(node.depth) * ROW;
    }
}

/// The five characters XML gives meaning to.
fn escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out
}
