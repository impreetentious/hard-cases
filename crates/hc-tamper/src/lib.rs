#![forbid(unsafe_code)]

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
//!
//! **What the pinned identifier means.** An entry's identifier is the earliest
//! rule that can see the defect *on the models this corpus is run against*.
//! Earliest is a property of the model as well as of the mutation: moving an
//! incumbent a thousand units is caught by the row check on a model with a
//! bound in the way and by the objective check on one without. The pin is held
//! to on the harness's own models, which is where it says something about the
//! order of the checks. What holds on every model, and is the property a
//! visitor is being shown, is that a forgery is refused at all.
//!
//! **Why this is its own crate.** Two callers need the corpus and neither
//! should have to take the other's dependencies to get it: the harness runs
//! every entry against the standalone verifier, and the browser surface hands
//! them to a visitor to try. Putting the forger in either of those would have
//! made the other depend on a benchmark harness or on a WebAssembly module to
//! reach a few hundred lines of encoding. It is deliberately a leaf: it
//! decodes, mutates and encodes, and it depends on nothing that verifies.

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
        name: "unknown_transform_rule",
        expect: "HCC-TRANSFORM-01",
        resealed: true,
    },
    Case {
        name: "inapplicable_transform",
        expect: "HCC-TRANSFORM-02",
        resealed: true,
    },
    Case {
        name: "edited_transform_value",
        expect: "HCC-TRANSFORM-02",
        resealed: true,
    },
    Case {
        name: "forged_stage_hash",
        expect: "HCC-TRANSFORM-03",
        resealed: true,
    },
    Case {
        name: "dropped_transform",
        expect: "HCC-TREE-14",
        resealed: true,
    },
    Case {
        name: "mismatched_witnesses",
        expect: "HCC-POINT-05",
        resealed: true,
    },
    Case {
        name: "forged_gomory_coefficient",
        expect: "HCC-CUT-05",
        resealed: true,
    },
    Case {
        name: "forged_cover_lift",
        expect: "HCC-CUT-11",
        resealed: true,
    },
    Case {
        name: "forged_cut_scope",
        expect: "HCC-SCOPE-04",
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

/// The fixed part of a certificate, in bytes.
///
/// The byte-level mutations reach past it on purpose, so that the header's own
/// checks are not what answers for them.
const HEADER_BYTES: usize = 66;
/// Where the header states how many bytes the stream occupies.
const DECLARED_LENGTH_AT: usize = 58;
/// The trailing checksum, in bytes.
const CHECKSUM_BYTES: usize = 4;

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
    // The byte-level mutations below reach at fixed offsets, so each one first
    // asks whether the offsets it needs exist. A forger that panicked on a
    // certificate too short for its edit would be a forger that could not be
    // handed arbitrary bytes, and it is handed arbitrary bytes: the situation
    // room offers this corpus to a visitor against whatever certificate is on
    // the page.
    let length = certificate.len();
    match name {
        "flipped_byte" => {
            // A byte inside the record stream rather than in the header, so
            // that the header's own checks are not what answers.
            let at = length / 2;
            if at < HEADER_BYTES || at >= length - CHECKSUM_BYTES {
                return Err(NotApplicable);
            }
            let mut bytes = certificate.to_vec();
            bytes[at] ^= 0x01;
            Ok(bytes)
        }
        "truncated_stream" => {
            if length < HEADER_BYTES + 8 {
                return Err(NotApplicable);
            }
            let mut bytes = certificate.to_vec();
            bytes.truncate(bytes.len() - 8);
            Ok(reseal(bytes))
        }
        "trailing_bytes" => {
            if length < HEADER_BYTES + CHECKSUM_BYTES {
                return Err(NotApplicable);
            }
            let mut bytes = certificate.to_vec();
            let tail = bytes.split_off(bytes.len() - CHECKSUM_BYTES);
            bytes.push(0);
            bytes.extend(tail);
            // The declared length is repaired as well as the checksum, so that
            // the header's own arithmetic is not what answers. What is left is
            // one byte after the terminating record and nothing else wrong.
            let declared = (bytes.len() - CHECKSUM_BYTES) as u64;
            bytes[DECLARED_LENGTH_AT..HEADER_BYTES].copy_from_slice(&declared.to_le_bytes());
            Ok(reseal(bytes))
        }
        "unknown_record_type" => {
            if length <= HEADER_BYTES + CHECKSUM_BYTES {
                return Err(NotApplicable);
            }
            let mut bytes = certificate.to_vec();
            // The first record's type byte sits immediately after the header.
            bytes[HEADER_BYTES] = 7;
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
    let split = bytes.len() - CHECKSUM_BYTES;
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
            // An inference can cite nothing at all — a bound that follows from
            // the objective's own sign needs no inequality — and there is then
            // no multiplier to negate.
            let terms = first_terms(records).ok_or(NotApplicable)?;
            let first = terms.first_mut().ok_or(NotApplicable)?;
            first.1 = first.1.neg();
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
            let first = terms.first_mut().ok_or(NotApplicable)?;
            let doubled = first.1.add(&first.1.clone());
            first.1 = doubled;
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
                    && let (Some(first), Some(second)) = (reduced.first_mut(), original.first_mut())
                {
                    let moved = first.add(&Rational::from_i64(1_000));
                    *first = moved.clone();
                    *second = moved;
                    touched = true;
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
        "unknown_transform_rule" => {
            let mut touched = false;
            for record in records.iter_mut() {
                if let Record::Transform { rule, .. } = record
                    && !touched
                {
                    *rule = 9;
                    touched = true;
                }
            }
            if !touched {
                return Err(NotApplicable);
            }
        }
        "inapplicable_transform" => {
            // Point a transform at a different row or column. The rule is
            // real, the model is real, and the condition does not hold there.
            let mut touched = false;
            for record in records.iter_mut() {
                if let Record::Transform { parameters, .. } = record
                    && !touched
                    && let Some(first) = parameters.first_mut()
                {
                    *first = first.add(&Rational::from_i64(1));
                    touched = true;
                }
            }
            if !touched {
                return Err(NotApplicable);
            }
        }
        "edited_transform_value" => {
            // Change the value a transform states rather than the indices. The
            // rule applies; the number it claims to derive is not the number
            // the model implies.
            let mut touched = false;
            for record in records.iter_mut() {
                if let Record::Transform { parameters, .. } = record
                    && !touched
                    && parameters.len() >= 2
                    && let Some(last) = parameters.last_mut()
                {
                    *last = last.add(&Rational::from_i64(1));
                    touched = true;
                }
            }
            if !touched {
                return Err(NotApplicable);
            }
        }
        "forged_stage_hash" => {
            let mut touched = false;
            for record in records.iter_mut() {
                if let Record::Transform { result, .. } = record
                    && !touched
                {
                    result[0] ^= 0xff;
                    touched = true;
                }
            }
            if !touched {
                return Err(NotApplicable);
            }
        }
        "dropped_transform" => {
            // The *last* stage of the chain, so what follows is the root
            // rather than another transform. A chain shortened in the middle
            // is caught one stage later by the next hash; shortened at the end
            // it is caught by the root, which is the case worth pinning
            // because the root is where the tree's model comes from.
            let at = records
                .iter()
                .rposition(|record| matches!(record, Record::Transform { .. }))
                .ok_or(NotApplicable)?;
            excise(records, at);
        }
        "mismatched_witnesses" => {
            // The two coordinate systems pulled apart. Both points may be
            // feasible in their own models; they no longer correspond.
            let mut touched = false;
            for record in records.iter_mut() {
                if let Record::Incumbent { original, .. } = record
                    && !touched
                    && let Some(pair) = distinct_pair(original)
                {
                    original.swap(pair.0, pair.1);
                    touched = true;
                }
            }
            if !touched {
                return Err(NotApplicable);
            }
        }
        "forged_gomory_coefficient" => {
            let mut touched = false;
            for record in records.iter_mut() {
                if let Record::GmiCut { coefficients, .. } = record
                    && !touched
                    && let Some((_, value)) = coefficients.first_mut()
                {
                    *value = value.add(&Rational::one());
                    touched = true;
                }
            }
            if !touched {
                return Err(NotApplicable);
            }
        }
        "forged_cover_lift" => {
            let mut touched = false;
            for record in records.iter_mut() {
                if let Record::CoverCut { coefficients, .. } = record
                    && !touched
                    && let Some((_, value)) = coefficients.first_mut()
                {
                    *value = value.add(&Rational::one());
                    touched = true;
                }
            }
            if !touched {
                return Err(NotApplicable);
            }
        }
        "forged_cut_scope" => {
            // A cut relabelled with a node that does not exist. The subtree it
            // claims to be true of is not in the tree at all.
            let mut touched = false;
            for record in records.iter_mut() {
                match record {
                    Record::GmiCut { scope, .. } | Record::CoverCut { scope, .. } if !touched => {
                        *scope = 1_000_000;
                        touched = true;
                    }
                    _ => {}
                }
            }
            if !touched {
                return Err(NotApplicable);
            }
        }
        "dropped_leaf" => {
            // Only meaningful on a tree that actually branched. Removing the
            // single leaf of a one-node tree leaves a root, a bound and an
            // integral point, which is a sound linear proof of the same thing
            // — accepting it is correct, so it is not a forgery.
            if !records
                .iter()
                .any(|record| matches!(record, Record::Branch { .. }))
            {
                return Err(NotApplicable);
            }
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
                if let Record::Branch { split, .. } = record
                    && !touched
                {
                    *split = split.add(&half());
                    touched = true;
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
                if let Record::LeafBound { node, .. } | Record::LeafFrontier { node, .. } = record
                    && !touched
                    && let Some(other) = nodes.iter().find(|candidate| *candidate != node)
                {
                    *node = *other;
                    touched = true;
                }
            }
            if !touched {
                return Err(NotApplicable);
            }
        }
        "moved_integral_point" => {
            let mut touched = false;
            for record in records.iter_mut() {
                if let Record::LeafIntegral { point, .. } = record
                    && !touched
                    && let Some(first) = point.first_mut()
                {
                    *first = first.add(&Rational::from_i64(1_000));
                    touched = true;
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
                if let Record::LeafBound { node, bound } = record
                    && !touched
                {
                    *record = Record::LeafFrontier {
                        node: *node,
                        bound: *bound,
                    };
                    touched = true;
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

/// The first two positions of a vector holding different values.
///
/// Swapping equal components changes nothing, and a mutation that changes
/// nothing produces a genuine certificate and a test that passes for the wrong
/// reason.
fn distinct_pair(values: &[Rational]) -> Option<(usize, usize)> {
    for left in 0..values.len() {
        for right in (left + 1)..values.len() {
            if values[left] != values[right] {
                return Some((left, right));
            }
        }
    }
    None
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A genuine certificate, frozen, so this file needs no solver to run.
    ///
    /// The asset is annotated hexadecimal — bytes with the derivation written
    /// beside them — and it is read here by the four lines below rather than
    /// through the verifier's reader, because this crate depends on nothing
    /// that verifies and is not about to start.
    const ANNOTATED: &str = include_str!("../../../spec/v1/vectors/hcc/tiny-optimal.hex");
    const INSTANCE: &[u8] = include_bytes!("../../../spec/v1/vectors/mps/tiny.mps");

    fn certificate() -> Vec<u8> {
        let mut out = Vec::new();
        for line in ANNOTATED.lines() {
            let body = line.split('#').next().unwrap_or_default();
            for pair in body.split_whitespace() {
                out.push(u8::from_str_radix(pair, 16).expect("a hexadecimal pair"));
            }
        }
        out
    }

    /// A tiny deterministic stream, so a finding here reproduces from a number.
    struct Stream(u64);

    impl Stream {
        fn next(&mut self) -> u64 {
            // SplitMix64, which is the generator this project already uses to
            // seed everything else.
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }
    }

    #[test]
    fn every_mutation_applies_to_a_genuine_certificate_or_declines() {
        let mut applied = 0usize;
        for case in CASES {
            if let Ok(forged) = forge(case.name, &certificate(), INSTANCE) {
                applied += 1;
                assert!(!forged.is_empty(), "{} produced nothing", case.name);
            }
        }
        assert!(applied > 0, "no mutation applied to the frozen certificate");
    }

    #[test]
    fn no_mutation_faults_on_bytes_that_are_not_a_certificate() {
        // The corpus is reachable from a C interface, and the module that
        // exports it answers rather than traps. A forger that panicked on a
        // short or damaged buffer would take the page down with it, so every
        // entry is handed rubbish of every length up to the header and a good
        // deal past it.
        let mut stream = Stream(0x5EED);
        for length in 0..200usize {
            let bytes: Vec<u8> = (0..length).map(|_| (stream.next() & 0xff) as u8).collect();
            for case in CASES {
                let _ = forge(case.name, &bytes, INSTANCE);
            }
        }
        // And on truncations of a real one, which is the shape that gets past
        // a length check and fails a later one.
        let genuine = certificate();
        for length in 0..genuine.len() {
            for case in CASES {
                let _ = forge(case.name, &genuine[..length], INSTANCE);
            }
        }
        // And on a real one with one byte changed, everywhere.
        for at in 0..genuine.len() {
            let mut damaged = genuine.clone();
            damaged[at] ^= 0xff;
            for case in CASES {
                let _ = forge(case.name, &damaged, INSTANCE);
            }
        }
    }

    #[test]
    fn a_name_the_corpus_does_not_carry_is_declined_rather_than_guessed_at() {
        assert_eq!(
            forge("there_is_no_such_mutation", &certificate(), INSTANCE),
            Err(NotApplicable)
        );
    }
}
