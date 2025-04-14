//! The trace of a real run.
//!
//! Every assertion here is about correspondence rather than about content: the
//! events have to be the ones the search recorded and the certificate carries,
//! in the order they happened. A trace that agreed with a fixture but not with
//! its run would be exactly the artifact the no-facade rule forbids.

use hc_cert::mip::assemble_mip;
use hc_cert::trace::{Run, Verdict, of, status_of};
use hc_core::trace::{Event, STATUSES, Trace};
use hc_io::mps::{Form, read};
use hc_mip::search::{Ending, Node, Options, run as search};
use hc_verify::vclose::verify;

const TINY: &[u8] = include_bytes!("../../../spec/v1/vectors/mps/tiny.mps");
const COVER: &[u8] = include_bytes!("../../../instances/local/cover6.mps");
/// An instance whose relaxation is fractional, so the tree has to branch.
const KNAPSACK: &[u8] = b"NAME KNAP\nROWS\n N obj\n L cap\nCOLUMNS\n m1 'MARKER' 'INTORG'\n x obj -5 cap 3\n y obj -4 cap 4\n z obj -3 cap 2\n m1 'MARKER' 'INTEND'\nRHS\n rhs cap 7\nBOUNDS\n UI bnd x 3\n UI bnd y 3\n UI bnd z 3\nENDATA\n";

/// Reads an instance under whichever variant it is written in.
fn parse(raw: &[u8]) -> hc_core::Model {
    read(raw, Form::Fixed)
        .or_else(|_| read(raw, Form::Free))
        .expect("a readable instance")
}

/// Solves an instance and assembles the trace of that solve.
fn traced(raw: &[u8], cuts: usize) -> (Trace, hc_mip::search::Search) {
    let model = parse(raw);
    let found = search(
        &model,
        &Options {
            cut_rounds: cuts,
            ..Options::default()
        },
    );
    let assembled = assemble_mip(&model, raw, &found).ok();
    let bytes = assembled.as_ref().map(|certificate| {
        let encoded = certificate.encode();
        (encoded.len() as u64, encoded)
    });
    let verdict = bytes
        .as_ref()
        .map(|(_, encoded)| match verify(encoded, raw) {
            Ok(_) => Verdict {
                accepted: true,
                rejection: None,
                work: 0,
            },
            Err(refusal) => Verdict {
                accepted: false,
                rejection: Some(refusal.code.text().to_owned()),
                work: 0,
            },
        });
    let trace = of(&Run {
        chain: None,
        search: &found,
        certificate: assembled.as_ref(),
        bytes: bytes.as_ref().map(|(length, _)| *length),
        verdict,
        status: status_of(found.ending, found.incumbent.is_some()),
    });
    (trace, found)
}

#[test]
fn the_trace_of_a_solve_reports_the_nodes_the_search_opened() {
    let (trace, found) = traced(KNAPSACK, 0);
    let opened: Vec<u64> = trace
        .events
        .iter()
        .filter_map(|event| match event {
            Event::NodeOpen { node, .. } => Some(*node),
            _ => None,
        })
        .collect();
    let solved: Vec<u64> = found.spent.iter().map(|record| record.node).collect();
    assert_eq!(
        opened, solved,
        "the trace and the search disagree about nodes"
    );
    assert!(opened.len() > 1, "the instance did not need a tree");

    // And every node the trace says was solved reports the iterations and the
    // objective the search recorded for it, in that order.
    let reported: Vec<(u64, u64, f64)> = trace
        .events
        .iter()
        .filter_map(|event| match event {
            Event::NodeSolved {
                node,
                iterations,
                objective,
            } => Some((*node, *iterations, *objective)),
            _ => None,
        })
        .collect();
    let recorded: Vec<(u64, u64, f64)> = found
        .spent
        .iter()
        .filter_map(|record| {
            record
                .objective
                .map(|objective| (record.node, record.iterations, objective))
        })
        .collect();
    assert_eq!(reported, recorded);
}

#[test]
fn the_trace_of_a_solve_ends_where_the_run_ended() {
    for (name, raw) in [("tiny", TINY), ("cover", COVER), ("knapsack", KNAPSACK)] {
        let (trace, found) = traced(raw, 0);
        let last = trace.events.last().expect("a trace is never empty");
        let Event::Terminal { status } = last else {
            panic!("{name}: the last event is {last:?} rather than the terminal one");
        };
        assert_eq!(
            STATUSES[*status as usize],
            match found.ending {
                Ending::Optimal => "optimal",
                Ending::Infeasible => "infeasible",
                _ => panic!("{name} ended {:?}", found.ending),
            }
        );
        // Exactly one terminal event, and it is the last.
        assert_eq!(
            trace
                .events
                .iter()
                .filter(|event| matches!(event, Event::Terminal { .. }))
                .count(),
            1
        );
    }
}

#[test]
fn a_cut_appears_in_the_trace_with_the_scope_it_is_valid_in() {
    let (trace, found) = traced(KNAPSACK, 4);
    let reported: Vec<(u64, u64, u8, f64)> = trace
        .events
        .iter()
        .filter_map(|event| match event {
            Event::CutAdded {
                identifier,
                scope,
                family,
                violation,
            } => Some((*identifier, *scope, *family, *violation)),
            _ => None,
        })
        .collect();
    let recorded: Vec<(u64, u64, u8, f64)> = found
        .separated
        .iter()
        .map(|cut| (cut.identifier, cut.scope, cut.family, cut.violation))
        .collect();
    assert_eq!(reported, recorded);
    assert!(!reported.is_empty(), "no cut was separated to report");
    for (_, _, _, violation) in &reported {
        assert!(
            violation.is_finite(),
            "a violation that is not finite reached a trace"
        );
    }
}

#[test]
fn the_trace_carries_the_exact_closing_bounds_the_certificate_claims() {
    let (trace, _) = traced(TINY, 0);
    let complete = trace
        .events
        .iter()
        .find_map(|event| match event {
            Event::CertifyComplete { lower, upper, gap } => Some((lower, upper, gap)),
            _ => None,
        })
        .expect("a solved instance closes its frontier");
    assert_eq!(complete.0, "-4");
    assert_eq!(complete.1, "-4");
    assert_eq!(complete.2, "0");
}

#[test]
fn an_open_node_reaches_the_trace_as_a_node_that_blocked_certification() {
    // The honest half. A search that ran out of nodes leaves the frontier
    // open, and the trace has to say which nodes stopped it rather than
    // reporting a certification that did not complete.
    let model = parse(KNAPSACK);
    let found = search(
        &model,
        &Options {
            node_limit: 1,
            ..Options::default()
        },
    );
    let trace = of(&Run {
        chain: None,
        search: &found,
        certificate: None,
        bytes: None,
        verdict: None,
        status: status_of(found.ending, found.incumbent.is_some()),
    });
    let frontier = found
        .nodes
        .iter()
        .filter(|node| matches!(node, Some(Node::Frontier { .. })))
        .count();
    let reported = trace
        .events
        .iter()
        .filter(|event| matches!(event, Event::CertifyNode { .. }))
        .count();
    assert_eq!(reported, frontier);
    assert!(
        !trace
            .events
            .iter()
            .any(|event| matches!(event, Event::CertifyComplete { .. })),
        "a frontier that never closed reported a completed certification"
    );
}

#[test]
fn the_same_solve_traces_the_same_bytes_twice() {
    // The determinism the double-run gate compares. Two runs of one solve
    // produce the same trace, in both encodings, byte for byte.
    for raw in [TINY, COVER, KNAPSACK] {
        let (first, _) = traced(raw, 4);
        let (second, _) = traced(raw, 4);
        assert_eq!(
            first.encode().expect("writable"),
            second.encode().expect("writable")
        );
        assert_eq!(
            first.render().expect("renderable"),
            second.render().expect("renderable")
        );
    }
}

#[test]
fn a_verdict_reaches_the_trace_as_the_verifier_gave_it() {
    let (trace, _) = traced(TINY, 0);
    let verdict = trace
        .events
        .iter()
        .find_map(|event| match event {
            Event::VerifyResult {
                accepted,
                rejection,
                ..
            } => Some((*accepted, rejection.clone())),
            _ => None,
        })
        .expect("the run was verified");
    assert_eq!(verdict, (true, None));
}
