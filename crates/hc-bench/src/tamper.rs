//! The tamper corpus: forged certificates and the rule that must catch each.
//!
//! Every entry is a named mutation of a certificate the solver genuinely
//! produced, paired with the exact rejection identifier the verifier must
//! answer with. The pairing is the point. A corpus that only asserted "these
//! must all be rejected" would pass while every one of them was caught by the
//! checksum, which would establish nothing about the proof rules and would
//! silently stop establishing anything the day a rule was deleted.
//!
//! Most mutations are performed on the decoded records and re-encoded, so the
//! checksum is correct and the mathematics has to be what refuses them. That
//! is the case that matters: anyone holding the format specification can
//! repair a checksum. The handful of byte-level entries exist to pin the
//! reader's own rules, and they say so.
//!
//! The corpus is a product surface rather than a test fixture. It is what the
//! tamper lab in the situation room will edit, it is what a reader who
//! disbelieves the whole arrangement should reach for first, and it grows
//! whenever an incident finds a mutation nobody had thought of.

use hc_cert::hcc::{Certificate, ClaimKind, Record};
use hc_cert::read::decode;
use hc_exact::{BigInt, Rational};

/// One entry: a mutation, and the rule that must catch it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Case {
    /// The mutation's name, which the manifest repeats.
    pub name: &'static str,
    /// The rejection identifier the verifier must answer with.
    pub expect: &'static str,
    /// Whether the checksum is repaired after the edit.
    ///
    /// False only where the entry exists to pin a reader rule that fires
    /// before the checksum is even consulted, or the checksum rule itself.
    pub resealed: bool,
}

/// Every mutation, in the order the manifest lists them.
pub const CASES: &[Case] = &[
    Case {
        name: "negated_multiplier",
        expect: "HCC-DUAL-01",
        resealed: true,
    },
    Case {
        name: "zero_multiplier",
        expect: "HCC-DUAL-02",
        resealed: true,
    },
    Case {
        name: "inflated_bound_multiplier",
        expect: "HCC-DUAL-04",
        resealed: true,
    },
    Case {
        name: "dropped_bound_multiplier",
        expect: "HCC-DUAL-05",
        resealed: true,
    },
    Case {
        name: "phantom_inequality",
        expect: "HCC-DUAL-07",
        resealed: true,
    },
    Case {
        name: "misstated_objective",
        expect: "HCC-POINT-04",
        resealed: true,
    },
    Case {
        name: "moved_incumbent",
        expect: "HCC-POINT-01",
        resealed: true,
    },
    Case {
        name: "dropped_incumbent",
        expect: "HCC-CLAIM-03",
        resealed: true,
    },
    Case {
        name: "forged_root_model",
        expect: "HCC-TREE-14",
        resealed: true,
    },
    Case {
        name: "swapped_claim_kind",
        expect: "HCC-CLAIM-01",
        resealed: true,
    },
    Case {
        name: "invented_lower_bound",
        expect: "HCC-CLAIM-10",
        resealed: true,
    },
    Case {
        name: "dropped_leaf",
        expect: "HCC-TREE-06",
        resealed: true,
    },
    Case {
        name: "dropped_branch",
        expect: "HCC-TREE-05",
        resealed: true,
    },
    Case {
        name: "fractional_split",
        expect: "HCC-TREE-02",
        resealed: true,
    },
    Case {
        name: "leaf_cites_another_node",
        expect: "HCC-TREE-08",
        resealed: true,
    },
    Case {
        name: "moved_integral_point",
        expect: "HCC-TREE-09",
        resealed: true,
    },
    Case {
        name: "forged_frontier",
        expect: "HCC-TREE-12",
        resealed: true,
    },
    Case {
        name: "flipped_byte",
        expect: "HCC-PARSE-09",
        resealed: false,
    },
    Case {
        name: "truncated_stream",
        expect: "HCC-PARSE-07",
        resealed: true,
    },
    Case {
        name: "trailing_bytes",
        expect: "HCC-PARSE-08",
        resealed: true,
    },
    Case {
        name: "unknown_record_type",
        expect: "HCC-PARSE-10",
        resealed: true,
    },
    Case {
        name: "reattached_certificate",
        expect: "HCC-PARSE-03",
        resealed: true,
    },
];

/// Why a mutation could not be applied to a particular certificate.
///
/// Not every mutation is meaningful on every claim: there is no multiplier to
/// negate in an unboundedness proof. A mutation that does not apply is skipped
/// by name, and the harness requires that every mutation applies somewhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotApplicable;

/// Applies a named mutation, returning the forged bytes.
///
/// # Errors
///
/// Returns [`NotApplicable`] when the certificate has nothing of the shape the
/// mutation edits.
pub fn forge(name: &str, certificate: &[u8], instance: &[u8]) -> Result<Vec<u8>, NotApplicable> {
    match name {
        "flipped_byte" => {
            let mut bytes = certificate.to_vec();
            // A byte inside the record stream rather than in the header, so
            // that the header's own checks are not what answers.
            let at = bytes.len() / 2;
            bytes[at] ^= 0x01;
            Ok(bytes)
        }
        "truncated_stream" => {
            let mut bytes = certificate.to_vec();
            bytes.truncate(bytes.len() - 8);
            Ok(reseal(bytes))
        }
        "trailing_bytes" => {
            let mut bytes = certificate.to_vec();
            let tail = bytes.split_off(bytes.len() - 4);
            bytes.push(0);
            bytes.extend(tail);
            // The declared length is repaired as well as the checksum, so that
            // the header's own arithmetic is not what answers. What is left is
            // one byte after the terminating record and nothing else wrong.
            let declared = (bytes.len() - 4) as u64;
            bytes[58..66].copy_from_slice(&declared.to_le_bytes());
            Ok(reseal(bytes))
        }
        "unknown_record_type" => {
            let mut bytes = certificate.to_vec();
            // The first record's type byte sits immediately after the header.
            bytes[66] = 7;
            Ok(reseal(bytes))
        }
        "reattached_certificate" => {
            // The certificate is untouched; the instance beside it is not.
            let _ = instance;
            Ok(certificate.to_vec())
        }
        _ => {
            let decoded = decode(certificate).map_err(|_| NotApplicable)?;
            let forged = edit(name, decoded)?;
            Ok(forged.encode())
        }
    }
}

/// The instance a case is verified against.
///
/// Every case but one is checked against the file the certificate was built
/// from. The reattachment case is the exception, and it is the whole reason
/// this function exists rather than the caller assuming.
#[must_use]
pub fn instance_for<'a>(name: &str, original: &'a [u8], other: &'a [u8]) -> &'a [u8] {
    if name == "reattached_certificate" {
        other
    } else {
        original
    }
}

/// Recomputes the trailing checksum over whatever the bytes now are.
fn reseal(mut bytes: Vec<u8>) -> Vec<u8> {
    let split = bytes.len() - 4;
    let mut sum = hc_core::Crc32c::new();
    sum.update(&bytes[..split]);
    let repaired = sum.finish().to_le_bytes();
    bytes[split..].copy_from_slice(&repaired);
    bytes
}

/// The record-level mutations.
fn edit(name: &str, mut certificate: Certificate) -> Result<Certificate, NotApplicable> {
    let records = &mut certificate.records;
    match name {
        "negated_multiplier" => {
            let terms = first_terms(records).ok_or(NotApplicable)?;
            terms[0].1 = terms[0].1.neg();
        }
        "zero_multiplier" => {
            let terms = first_terms(records).ok_or(NotApplicable)?;
            let id = terms.last().ok_or(NotApplicable)?.0;
            terms.push((id + 1, Rational::zero()));
        }
        "inflated_bound_multiplier" => {
            // Only meaningful on a bound: doubling a contradiction's
            // multiplier breaks the zero sum, which is a different rule, and
            // that case has its own entry.
            let terms = terms_of(records, Kind::Bound).ok_or(NotApplicable)?;
            terms[0].1 = terms[0].1.add(&terms[0].1.clone());
        }
        "dropped_bound_multiplier" => {
            // The classic omission: a Farkas witness over rows only, with the
            // variable bounds assumed available by convention. Here there is no
            // convention to lean on and the coefficients fail to cancel.
            let terms = terms_of(records, Kind::Contradiction).ok_or(NotApplicable)?;
            if terms.len() < 2 {
                return Err(NotApplicable);
            }
            terms.pop();
        }
        "phantom_inequality" => {
            let terms = first_terms(records).ok_or(NotApplicable)?;
            let id = terms.last().ok_or(NotApplicable)?.0;
            // An identifier far past the end of the model's inequality list.
            terms.push((id + 1_000_000, Rational::one()));
        }
        "misstated_objective" => {
            let mut touched = false;
            for record in records.iter_mut() {
                if let Record::Incumbent { objective, .. }
                | Record::FeasiblePoint { objective, .. } = record
                {
                    *objective = objective.add(&Rational::one());
                    touched = true;
                }
            }
            if !touched {
                return Err(NotApplicable);
            }
        }
        "moved_incumbent" => {
            let mut touched = false;
            for record in records.iter_mut() {
                if let Record::Incumbent {
                    reduced, original, ..
                } = record
                {
                    if let (Some(first), Some(second)) = (reduced.first_mut(), original.first_mut())
                    {
                        let moved = first.add(&Rational::from_i64(1_000));
                        *first = moved.clone();
                        *second = moved;
                        touched = true;
                    }
                }
            }
            if !touched {
                return Err(NotApplicable);
            }
        }
        "forged_root_model" => {
            let mut touched = false;
            for record in records.iter_mut() {
                if let Record::Root { model, .. } = record {
                    model[0] ^= 0xff;
                    touched = true;
                }
            }
            if !touched {
                return Err(NotApplicable);
            }
        }
        "swapped_claim_kind" => {
            // The header keeps its kind and the claim record changes, which is
            // the shape an edit made with a hex editor takes.
            let mut touched = false;
            for record in records.iter_mut() {
                if let Record::Claim { kind, .. } = record {
                    *kind = match kind {
                        ClaimKind::Optimal => ClaimKind::FeasibleWithGap,
                        _ => ClaimKind::Optimal,
                    };
                    touched = true;
                }
            }
            if !touched {
                return Err(NotApplicable);
            }
        }
        "invented_lower_bound" => {
            let mut touched = false;
            for record in records.iter_mut() {
                if let Record::Claim {
                    kind: ClaimKind::Optimal,
                    lower,
                    ..
                } = record
                {
                    *lower = Some(
                        Rational::new(BigInt::from_i64(-1), BigInt::from_i64(1))
                            .expect("a denominator of one"),
                    );
                    touched = true;
                }
            }
            if !touched {
                return Err(NotApplicable);
            }
        }
        "dropped_leaf" => {
            // A node the certificate stops accounting for. The cover is
            // incomplete and the region that leaf covered could hold anything.
            let at = records
                .iter()
                .position(|record| {
                    matches!(
                        record,
                        Record::LeafBound { .. }
                            | Record::LeafInfeas { .. }
                            | Record::LeafIntegral { .. }
                            | Record::LeafFrontier { .. }
                    )
                })
                .ok_or(NotApplicable)?;
            excise(records, at);
        }
        "dropped_branch" => {
            // The split that created two nodes, deleted. Their leaves now name
            // nodes that were never created.
            let at = records
                .iter()
                .position(|record| matches!(record, Record::Branch { .. }))
                .ok_or(NotApplicable)?;
            excise(records, at);
        }
        "fractional_split" => {
            let mut touched = false;
            for record in records.iter_mut() {
                if let Record::Branch { split, .. } = record {
                    if !touched {
                        *split = split.add(&half());
                        touched = true;
                    }
                }
            }
            if !touched {
                return Err(NotApplicable);
            }
        }
        "leaf_cites_another_node" => {
            // A leaf pointed at an inference made somewhere else in the tree.
            // The inference is sound; it is about a different region.
            let nodes: Vec<u64> = records
                .iter()
                .filter_map(|record| match record {
                    Record::DualBound { node, .. } => Some(*node),
                    _ => None,
                })
                .collect();
            if nodes.len() < 2 {
                return Err(NotApplicable);
            }
            let mut touched = false;
            for record in records.iter_mut() {
                if let Record::LeafBound { node, .. } | Record::LeafFrontier { node, .. } = record {
                    if !touched {
                        if let Some(other) = nodes.iter().find(|candidate| *candidate != node) {
                            *node = *other;
                            touched = true;
                        }
                    }
                }
            }
            if !touched {
                return Err(NotApplicable);
            }
        }
        "moved_integral_point" => {
            let mut touched = false;
            for record in records.iter_mut() {
                if let Record::LeafIntegral { point, .. } = record {
                    if !touched {
                        if let Some(first) = point.first_mut() {
                            *first = first.add(&Rational::from_i64(1_000));
                            touched = true;
                        }
                    }
                }
            }
            if !touched {
                return Err(NotApplicable);
            }
        }
        "forged_frontier" => {
            // A closed node presented as still open, which would turn a
            // complete proof of optimality into a manufactured gap.
            let mut touched = false;
            for record in records.iter_mut() {
                if let Record::LeafBound { node, bound } = record {
                    if !touched {
                        *record = Record::LeafFrontier {
                            node: *node,
                            bound: *bound,
                        };
                        touched = true;
                    }
                }
            }
            if !touched {
                return Err(NotApplicable);
            }
        }
        "dropped_incumbent" => {
            let before = records.len();
            records.retain(|record| !matches!(record, Record::Incumbent { .. }));
            if records.len() == before {
                return Err(NotApplicable);
            }
        }
        _ => return Err(NotApplicable),
    }

    // The terminator is rebuilt so that its counts match the edited stream. A
    // forger who left it stale would be caught by a reader rule rather than by
    // the proof rule the case is about, which would make the case worthless.
    let without_end: Vec<Record> = certificate
        .records
        .iter()
        .filter(|record| !matches!(record, Record::End { .. }))
        .cloned()
        .collect();
    certificate.records = Certificate::terminated(without_end);
    Ok(certificate)
}

/// Removes a record and renumbers every citation that pointed past it.
///
/// A record's identifier is its position, so deleting one shifts everything
/// after it. A forger who deleted without renumbering would produce a stream
/// the reader refuses for a citation error, which would test the reader rather
/// than the rule the case is about. This is the mutation done properly.
fn excise(records: &mut Vec<Record>, at: usize) {
    records.remove(at);
    let boundary = at as u64;
    for record in records.iter_mut() {
        let cited = match record {
            Record::LeafBound { bound, .. } | Record::LeafFrontier { bound, .. } => bound,
            Record::LeafIntegral { bound, .. } => bound,
            Record::LeafInfeas { contradiction, .. } => contradiction,
            _ => continue,
        };
        if *cited > boundary {
            *cited -= 1;
        }
    }
}

/// One half, for the fractional-split mutation.
fn half() -> Rational {
    Rational::new(BigInt::from_i64(1), BigInt::from_i64(2)).expect("a nonzero denominator")
}

/// Which inference kind a mutation is about.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// A lower bound, whose coefficients must reach the objective.
    Bound,
    /// A contradiction, whose coefficients must cancel to zero.
    Contradiction,
    /// Either; the mutation means the same thing to both.
    Either,
}

/// The multipliers of the first inference of a kind.
fn terms_of(records: &mut [Record], kind: Kind) -> Option<&mut Vec<(u64, Rational)>> {
    records.iter_mut().find_map(|record| match record {
        Record::DualBound { terms, .. } if kind != Kind::Contradiction => Some(terms),
        Record::Contradiction { terms, .. } if kind != Kind::Bound => Some(terms),
        _ => None,
    })
}

/// The multipliers of the first inference in a certificate, of either kind.
fn first_terms(records: &mut [Record]) -> Option<&mut Vec<(u64, Rational)>> {
    terms_of(records, Kind::Either)
}
