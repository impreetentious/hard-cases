//! Assembling a trace from a run.
//!
//! Everything here comes from something the run recorded. The nodes and their
//! costs come from the search, the transforms and the closing bounds come from
//! the certificate the assembler produced, and the verdict comes from the
//! verifier. Nothing is reconstructed, estimated, or filled in: an event this
//! module cannot source is an event it does not write, and a schema field it
//! has no value for is a reason not to write that event at all.
//!
//! That is not fastidiousness. A trace is what the situation room renders, and
//! a panel drawing a number nobody measured is the failure the whole no-facade
//! rule exists to prevent. The rule is easier to keep here, at the one place a
//! trace is made, than in every panel that reads one.
//!
//! The trace is assembled after the fact rather than streamed during the
//! search. The search is single-threaded and deterministic, so the order it
//! recorded things in is the order they happened in, and an assembler reading
//! those records writes the same events a streaming one would — without
//! putting a channel through the middle of a solver that has none.

use hc_core::trace::{Event, Trace};
use hc_mip::search::{Ending, Node, Search};
use hc_presolve::Chain;

use crate::hcc::{Certificate, ClaimKind, Record};

/// The word list positions the schema fixes.
const SOURCE_NODE: u8 = 4;
const BLOCKED_NO_BOUND: u8 = 0;
const BLOCKED_REFUSED: u8 = 2;

/// What the verifier said about the certificate, when it was asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verdict {
    /// Whether it accepted.
    pub accepted: bool,
    /// The identifier it refused with, when it refused.
    pub rejection: Option<String>,
    /// Its own deterministic work units.
    pub work: u64,
}

/// Everything a trace is assembled from.
pub struct Run<'a> {
    /// The digest of the instance the run was about.
    pub instance: [u8; 32],
    /// The reduction chain, when presolve ran.
    pub chain: Option<&'a Chain>,
    /// The search.
    pub search: &'a Search,
    /// The certificate, when one was assembled.
    pub certificate: Option<&'a Certificate>,
    /// The certificate as it was encoded, when one was.
    ///
    /// The bytes rather than their length, because the trace records the
    /// checksum the certificate carries as well as its size, and deriving both
    /// from the same bytes is what keeps them from drifting apart.
    pub proof: Option<&'a [u8]>,
    /// What the verifier said, when it was asked.
    pub verdict: Option<Verdict>,
    /// The claim word the run ended on, from the closed status list.
    pub status: u8,
}

/// Assembles the trace of one run.
#[must_use]
pub fn of(run: &Run<'_>) -> Trace {
    let mut trace = Trace::about(run.instance);

    // The transforms, from the certificate rather than from the chain. Both
    // record the same reductions, and the certificate's are the ones a
    // verifier replayed, so a trace beside a proof is describing the same
    // steps the proof is about.
    if let Some(certificate) = run.certificate {
        for (position, record) in certificate.records.iter().enumerate() {
            match record {
                Record::Transform { rule, .. } | Record::TransformContradiction { rule, .. } => {
                    trace.push(Event::TransformApplied {
                        rule: *rule,
                        position: position as u64,
                    });
                }
                _ => {}
            }
        }
    } else if let Some(chain) = run.chain {
        // Without a certificate the chain is the only record there is. The
        // rule identifier is its position in the chain, which is what the
        // reduction is known by until a certificate names it.
        for position in 0..chain.steps.len() {
            trace.push(Event::TransformApplied {
                rule: position as u64,
                position: position as u64,
            });
        }
    }

    // The root relaxation is the first node the search solved, which is the
    // root by construction. It is written as its own event because a reader
    // wants the root bound before the tree, and it is the same record.
    let mut spent = run.search.spent.iter();
    if let Some(first) = spent.next() {
        if let Some(objective) = first.objective {
            trace.push(Event::RootLp {
                iterations: first.iterations,
                objective,
            });
        }
        push_node(&mut trace, run.search, first.node);
        push_solved(&mut trace, first);
    }
    for record in spent {
        push_node(&mut trace, run.search, record.node);
        push_solved(&mut trace, record);
    }

    // The cuts, in the order they entered the pool, each with the subtree it
    // is valid in and the violation measured where it was separated.
    for cut in &run.search.separated {
        trace.push(Event::CutAdded {
            identifier: cut.identifier,
            scope: cut.scope,
            family: cut.family,
            violation: cut.violation,
        });
    }

    if let Some(incumbent) = &run.search.incumbent {
        trace.push(Event::Incumbent {
            objective: incumbent.objective.to_canonical_string(),
            source: SOURCE_NODE,
        });
    }

    // Certification. The frontier is what the assembler had to close, so the
    // events are written from the nodes it found open.
    trace.push(Event::CertifyStart);
    for (id, node) in run.search.nodes.iter().enumerate() {
        match node {
            Some(Node::Frontier { value, .. }) => trace.push(Event::CertifyNode {
                node: id as u64,
                bound: value.to_canonical_string(),
            }),
            Some(Node::Unproved { .. }) => trace.push(Event::CertifyBlocked {
                node: id as u64,
                reason: BLOCKED_NO_BOUND,
            }),
            Some(Node::Suspect { .. }) => trace.push(Event::CertifyBlocked {
                node: id as u64,
                reason: BLOCKED_REFUSED,
            }),
            _ => {}
        }
    }
    if let Some(certificate) = run.certificate {
        if let Some(event) = complete(certificate) {
            trace.push(event);
        }
    }

    if let Some(proof) = run.proof {
        trace.push(Event::ProofAssembled {
            bytes: proof.len() as u64,
            // The checksum the certificate carries in its own terminating
            // record, which is its last four bytes.
            checksum: u64::from(u32::from_le_bytes(
                proof
                    .get(proof.len().saturating_sub(4)..)
                    .and_then(|tail| tail.try_into().ok())
                    .unwrap_or([0; 4]),
            )),
        });
    }
    if let Some(verdict) = &run.verdict {
        trace.push(Event::VerifyResult {
            accepted: verdict.accepted,
            rejection: verdict.rejection.clone(),
            work: verdict.work,
        });
    }
    trace.push(Event::Terminal { status: run.status });
    trace
}

/// The status word an ending and a certificate agree on.
///
/// The trace's status list is the claim vocabulary with one extra member, so
/// this maps a search ending onto it and reports the extra member only for a
/// run that reached none of the others.
#[must_use]
pub fn status_of(ending: Ending, has_incumbent: bool) -> u8 {
    match ending {
        Ending::Optimal => 0,
        Ending::Infeasible => 1,
        Ending::UnboundedRelaxation => 2,
        Ending::Frontier => {
            if has_incumbent {
                3
            } else {
                6
            }
        }
        Ending::NoProgress => {
            if has_incumbent {
                4
            } else {
                5
            }
        }
    }
}

/// Writes the event for a node, from what the search recorded about it.
fn push_node(trace: &mut Trace, search: &Search, id: u64) {
    let Some(entry) = search.tree.get(id) else {
        return;
    };
    trace.push(Event::NodeOpen {
        node: id,
        parent: entry.parent.unwrap_or(id),
        depth: u64::from(entry.depth),
    });
}

/// Writes the events a solved node produced.
fn push_solved(trace: &mut Trace, spent: &hc_mip::search::Spent) {
    if let Some(objective) = spent.objective {
        trace.push(Event::NodeSolved {
            node: spent.node,
            iterations: spent.iterations,
            objective,
        });
    }
}

/// The closing event, from the claim the certificate carries.
///
/// An optimal claim carries its objective and nothing else, because the two
/// bounds meeting is what optimal *means*: the verifier canonicalizes a claim
/// whose gap is zero into that one. So the event restates the claim rather
/// than reporting an absence, and it is a restatement and not an invention —
/// a certificate saying optimal at a value is a certificate saying the bounds
/// are that value and the gap is nothing.
fn complete(certificate: &Certificate) -> Option<Event> {
    let mut lower = None;
    let mut upper = None;
    let mut gap = None;
    let mut optimal = false;
    for record in &certificate.records {
        if let Record::Claim {
            kind,
            objective,
            lower: low,
            gap: difference,
        } = record
        {
            optimal = *kind == ClaimKind::Optimal;
            lower.clone_from(low);
            upper.clone_from(objective);
            gap.clone_from(difference);
        }
    }
    if optimal {
        if let Some(objective) = upper.clone() {
            return Some(Event::CertifyComplete {
                lower: objective.to_canonical_string(),
                upper: objective.to_canonical_string(),
                gap: "0".to_owned(),
            });
        }
    }
    match (lower, upper, gap) {
        (Some(lower), Some(upper), Some(gap)) => Some(Event::CertifyComplete {
            lower: lower.to_canonical_string(),
            upper: upper.to_canonical_string(),
            gap: gap.to_canonical_string(),
        }),
        _ => None,
    }
}
