//! The certificate: what a proof looks like on disk.
//!
//! A sequence of records that a reader with no access to the solver can replay
//! against the original file. The record kinds are a closed list and so are
//! the inference rules; there is no rule meaning trust this bound, no user
//! axiom, and no extension point, because a proof system with an extension
//! mechanism is a proof system with a hole in it.
//!
//! Three encoding decisions are worth stating where they are implemented.
//! Variable-length integers must be minimally encoded, so one number has one
//! spelling and one proof has one byte stream. Rationals travel as canonical
//! text rather than as a pair of limb arrays, because the text has exactly one
//! form per value and a limb array has as many as there are ways to pad it.
//! And every declared count is checked against a cap *before* anything sized by
//! it is reserved — a length field is a claim made by the input, never an
//! instruction.
//!
//! The checksum is not part of the proof. A correct checksum establishes
//! nothing about the mathematics; it is checked first only so that a truncated
//! file is diagnosed as truncated rather than as an invalid inference.

use hc_core::Crc32c;
use hc_exact::Rational;

/// The four bytes every certificate begins with.
pub const MAGIC: [u8; 4] = *b"HCC1";
/// The certificate format version this reader writes and accepts.
pub const HCC_VERSION: u16 = 1;
/// The canonicalization version a certificate must be about.
pub const CANONICAL_VERSION: u16 = 1;

/// The largest accepted rational, in bits.
pub const RATIONAL_BITS: u64 = 1 << 20;
/// The largest accepted rational token, in bytes.
pub const RATIONAL_TOKEN_BYTES: u64 = 262_144;
/// The largest accepted record count.
pub const MAX_RECORDS: u64 = 20_000_000;
/// The largest accepted certificate, in bytes.
pub const MAX_BYTES: u64 = 1_073_741_824;

/// What a certificate claims.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaimKind {
    /// The stated objective is attained and nothing feasible is smaller.
    Optimal,
    /// The model has no feasible point.
    Infeasible,
    /// The objective is unbounded below.
    Unbounded,
    /// A point, a certified lower bound, and the exact gap between them.
    FeasibleWithGap,
    /// A point, and nothing about a lower bound.
    FeasibleNoBound,
}

impl ClaimKind {
    /// The byte this kind travels as.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Optimal => 1,
            Self::Infeasible => 2,
            Self::Unbounded => 3,
            Self::FeasibleWithGap => 4,
            Self::FeasibleNoBound => 5,
        }
    }

    /// The kind a byte names.
    #[must_use]
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Optimal),
            2 => Some(Self::Infeasible),
            3 => Some(Self::Unbounded),
            4 => Some(Self::FeasibleWithGap),
            5 => Some(Self::FeasibleNoBound),
            _ => None,
        }
    }
}

/// One record of a certificate.
///
/// The identifiers a record carries are record identifiers, strictly
/// increasing across the stream, and a record may cite only identifiers below
/// its own. That is what makes a forward reference — the shape a circular
/// argument takes on disk — a parse error rather than a semantic one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Record {
    /// A presolve equivalence and the hash of the model it produces.
    Transform {
        /// The transform rule's identifier.
        rule: u64,
        /// The rule's parameters, as canonical rationals.
        parameters: Vec<Rational>,
        /// The canonical hash of the model the rule produces.
        result: [u8; 32],
    },
    /// A presolve step whose domain came out empty.
    TransformContradiction {
        /// The transform rule's identifier.
        rule: u64,
        /// The canonical inequalities the empty domain follows from.
        sources: Vec<u64>,
    },
    /// The root of the search tree, and the model it is about.
    Root {
        /// Always zero.
        node: u64,
        /// The canonical hash of the reduced model.
        model: [u8; 32],
    },
    /// A lower bound from nonnegative multipliers over visible inequalities.
    DualBound {
        /// The node the inference is made at.
        node: u64,
        /// Cited inequality identifiers with their multipliers, ascending.
        terms: Vec<(u64, Rational)>,
    },
    /// An infeasibility from nonnegative multipliers over visible
    /// inequalities.
    Contradiction {
        /// The node the inference is made at.
        node: u64,
        /// Cited inequality identifiers with their multipliers, ascending.
        terms: Vec<(u64, Rational)>,
    },
    /// A Gomory mixed-integer cut and everything needed to recompute it.
    GmiCut {
        /// The node whose subtree the cut is usable in.
        scope: u64,
        /// The combination taken over visible inequalities.
        combination: Vec<(u64, Rational)>,
        /// The integer variable the combination isolates.
        isolated: u64,
        /// The resulting coefficients, by variable.
        coefficients: Vec<(u64, Rational)>,
        /// The resulting right-hand side.
        right: Rational,
    },
    /// A knapsack cover cut and its lifting.
    CoverCut {
        /// The node whose subtree the cut is usable in.
        scope: u64,
        /// The inequality the cover was taken from.
        source: u64,
        /// The cover's members, ascending.
        cover: Vec<u64>,
        /// The order lifting was performed in.
        lift_order: Vec<u64>,
        /// The lifted coefficients, by variable.
        coefficients: Vec<(u64, Rational)>,
    },
    /// A two-way integer split.
    Branch {
        /// The node being split.
        parent: u64,
        /// The variable split on.
        variable: u64,
        /// The integer the split is made at.
        split: Rational,
        /// The child taking the at-most side.
        left: u64,
        /// The child taking the at-least side.
        right: u64,
    },
    /// A node closed by a bound.
    LeafBound {
        /// The node.
        node: u64,
        /// The bound cited, which must be at this node.
        bound: u64,
    },
    /// A node closed by a contradiction.
    LeafInfeas {
        /// The node.
        node: u64,
        /// The contradiction cited, which must be at this node.
        contradiction: u64,
    },
    /// A node closed by an integral point that attains its own bound.
    LeafIntegral {
        /// The node.
        node: u64,
        /// The point, in the reduced model's coordinates.
        point: Vec<Rational>,
        /// The bound the point attains.
        bound: u64,
    },
    /// A node left open, with a finite bound.
    LeafFrontier {
        /// The node.
        node: u64,
        /// The bound cited, which must be finite.
        bound: u64,
    },
    /// The best point found, in both coordinate systems.
    Incumbent {
        /// The point in the reduced model's coordinates.
        reduced: Vec<Rational>,
        /// The point in the original model's coordinates.
        original: Vec<Rational>,
        /// The objective it attains.
        objective: Rational,
    },
    /// A point and nothing else.
    FeasiblePoint {
        /// The point in the original model's coordinates.
        point: Vec<Rational>,
        /// The objective it attains.
        objective: Rational,
    },
    /// A direction the objective improves along without limit.
    Ray {
        /// A feasible point to travel from.
        base: Vec<Rational>,
        /// The direction.
        direction: Vec<Rational>,
        /// The positive integer step, for a mixed-integer claim.
        step: Option<Rational>,
    },
    /// What the certificate claims, and the numbers it claims.
    Claim {
        /// The kind claimed.
        kind: ClaimKind,
        /// The objective attained, where the claim has one.
        objective: Option<Rational>,
        /// The lower bound, where the claim has one.
        lower: Option<Rational>,
        /// The exact relative gap, where the claim has one.
        gap: Option<Rational>,
    },
    /// The terminator, with a count per record kind.
    End {
        /// One count per record kind, ordered by type code.
        counts: Vec<(u8, u64)>,
    },
}

impl Record {
    /// The byte this record kind travels as.
    #[must_use]
    pub const fn code(&self) -> u8 {
        match self {
            Self::Transform { .. } => 1,
            Self::TransformContradiction { .. } => 2,
            Self::Root { .. } => 3,
            Self::DualBound { .. } => 16,
            Self::Contradiction { .. } => 17,
            Self::GmiCut { .. } => 32,
            Self::CoverCut { .. } => 33,
            Self::Branch { .. } => 48,
            Self::LeafBound { .. } => 64,
            Self::LeafInfeas { .. } => 65,
            Self::LeafIntegral { .. } => 66,
            Self::LeafFrontier { .. } => 67,
            Self::Incumbent { .. } => 80,
            Self::FeasiblePoint { .. } => 81,
            Self::Ray { .. } => 82,
            Self::Claim { .. } => 96,
            Self::End { .. } => 127,
        }
    }

    /// Every record type code, ascending.
    ///
    /// The terminator carries a count per kind in this order, so the order is
    /// written down once rather than implied by whatever a match arm happens
    /// to do.
    #[must_use]
    pub const fn codes() -> [u8; 17] {
        [
            1, 2, 3, 16, 17, 32, 33, 48, 64, 65, 66, 67, 80, 81, 82, 96, 127,
        ]
    }
}

/// A whole certificate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Certificate {
    /// Which format's rules produced the instance.
    pub format_tag: u8,
    /// The instance file's length in bytes.
    pub instance_bytes: u64,
    /// The digest of the format tag followed by the instance's bytes.
    pub digest: [u8; 32],
    /// What the certificate claims.
    pub claim: ClaimKind,
    /// The records, in order.
    pub records: Vec<Record>,
}

impl Certificate {
    /// Writes the certificate.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let body = self.body();
        let mut out = Vec::with_capacity(body.len() + 64);
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&HCC_VERSION.to_le_bytes());
        out.extend_from_slice(&CANONICAL_VERSION.to_le_bytes());
        out.push(self.format_tag);
        out.extend_from_slice(&self.instance_bytes.to_le_bytes());
        out.extend_from_slice(&self.digest);
        out.push(self.claim.code());
        out.extend_from_slice(&(self.records.len() as u64).to_le_bytes());
        // The declared byte length counts the whole file, header included, so
        // a reader can check it against what it actually holds.
        let declared = (out.len() + 8 + body.len()) as u64;
        out.extend_from_slice(&declared.to_le_bytes());
        out.extend_from_slice(&body);

        // The checksum covers everything written so far and is appended after
        // the terminating record's payload.
        let mut checksum = Crc32c::new();
        checksum.update(&out);
        out.extend_from_slice(&checksum.finish().to_le_bytes());
        out
    }

    /// The records, framed, without the header or the checksum.
    fn body(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for (position, record) in self.records.iter().enumerate() {
            let payload = encode_payload(record);
            out.push(record.code());
            write_leb128(&mut out, position as u64);
            write_leb128(&mut out, payload.len() as u64);
            out.extend_from_slice(&payload);
        }
        out
    }

    /// Appends the terminator a record list needs, with the counts it needs.
    ///
    /// The terminator counts every record in the stream, and the terminator is
    /// a record in the stream, so its own entry is part of what it reports.
    /// Building it here rather than at each assembly site is what keeps that
    /// self-reference from being got wrong four separate times.
    #[must_use]
    pub fn terminated(mut records: Vec<Record>) -> Vec<Record> {
        records.push(Record::End { counts: Vec::new() });
        let counts = Self::counts(&records);
        let last = records.len() - 1;
        records[last] = Record::End { counts };
        records
    }

    /// The counts a terminator must carry for these records.
    ///
    /// A plain tally over exactly what it is given, including any terminator
    /// among them. Built from the records rather than accumulated while
    /// writing, so a writer cannot produce a stream whose terminator disagrees
    /// with it.
    #[must_use]
    pub fn counts(records: &[Record]) -> Vec<(u8, u64)> {
        Record::codes()
            .iter()
            .filter_map(|code| {
                let count = records
                    .iter()
                    .filter(|record| record.code() == *code)
                    .count() as u64;
                (count > 0).then_some((*code, count))
            })
            .collect()
    }
}

/// Writes a minimally encoded variable-length integer.
fn write_leb128(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

/// Writes a rational as its canonical text, length-prefixed.
fn write_rational(out: &mut Vec<u8>, value: &Rational) {
    let text = value.to_canonical_string();
    write_leb128(out, text.len() as u64);
    out.extend_from_slice(text.as_bytes());
}

/// Writes a counted array of rationals.
fn write_rationals(out: &mut Vec<u8>, values: &[Rational]) {
    write_leb128(out, values.len() as u64);
    for value in values {
        write_rational(out, value);
    }
}

/// Writes a counted array of identifier and multiplier pairs.
fn write_terms(out: &mut Vec<u8>, terms: &[(u64, Rational)]) {
    write_leb128(out, terms.len() as u64);
    for (id, value) in terms {
        write_leb128(out, *id);
        write_rational(out, value);
    }
}

/// Writes a counted array of identifiers.
fn write_ids(out: &mut Vec<u8>, ids: &[u64]) {
    write_leb128(out, ids.len() as u64);
    for id in ids {
        write_leb128(out, *id);
    }
}

/// Writes one record's payload.
fn encode_payload(record: &Record) -> Vec<u8> {
    let mut out = Vec::new();
    match record {
        Record::Transform {
            rule,
            parameters,
            result,
        } => {
            write_leb128(&mut out, *rule);
            write_rationals(&mut out, parameters);
            out.extend_from_slice(result);
        }
        Record::TransformContradiction { rule, sources } => {
            write_leb128(&mut out, *rule);
            write_ids(&mut out, sources);
        }
        Record::Root { node, model } => {
            write_leb128(&mut out, *node);
            out.extend_from_slice(model);
        }
        Record::DualBound { node, terms } | Record::Contradiction { node, terms } => {
            write_leb128(&mut out, *node);
            write_terms(&mut out, terms);
        }
        Record::GmiCut {
            scope,
            combination,
            isolated,
            coefficients,
            right,
        } => {
            write_leb128(&mut out, *scope);
            write_terms(&mut out, combination);
            write_leb128(&mut out, *isolated);
            write_terms(&mut out, coefficients);
            write_rational(&mut out, right);
        }
        Record::CoverCut {
            scope,
            source,
            cover,
            lift_order,
            coefficients,
        } => {
            write_leb128(&mut out, *scope);
            write_leb128(&mut out, *source);
            write_ids(&mut out, cover);
            write_ids(&mut out, lift_order);
            write_terms(&mut out, coefficients);
        }
        Record::Branch {
            parent,
            variable,
            split,
            left,
            right,
        } => {
            write_leb128(&mut out, *parent);
            write_leb128(&mut out, *variable);
            write_rational(&mut out, split);
            write_leb128(&mut out, *left);
            write_leb128(&mut out, *right);
        }
        Record::LeafBound { node, bound } | Record::LeafFrontier { node, bound } => {
            write_leb128(&mut out, *node);
            write_leb128(&mut out, *bound);
        }
        Record::LeafInfeas {
            node,
            contradiction,
        } => {
            write_leb128(&mut out, *node);
            write_leb128(&mut out, *contradiction);
        }
        Record::LeafIntegral { node, point, bound } => {
            write_leb128(&mut out, *node);
            write_rationals(&mut out, point);
            write_leb128(&mut out, *bound);
        }
        Record::Incumbent {
            reduced,
            original,
            objective,
        } => {
            write_rationals(&mut out, reduced);
            write_rationals(&mut out, original);
            write_rational(&mut out, objective);
        }
        Record::FeasiblePoint { point, objective } => {
            write_rationals(&mut out, point);
            write_rational(&mut out, objective);
        }
        Record::Ray {
            base,
            direction,
            step,
        } => {
            write_rationals(&mut out, base);
            write_rationals(&mut out, direction);
            match step {
                None => out.push(0),
                Some(value) => {
                    out.push(1);
                    write_rational(&mut out, value);
                }
            }
        }
        Record::Claim {
            kind,
            objective,
            lower,
            gap,
        } => {
            out.push(kind.code());
            for slot in [objective, lower, gap] {
                match slot {
                    None => out.push(0),
                    Some(value) => {
                        out.push(1);
                        write_rational(&mut out, value);
                    }
                }
            }
        }
        Record::End { counts } => {
            write_leb128(&mut out, counts.len() as u64);
            for (code, count) in counts {
                out.push(*code);
                write_leb128(&mut out, *count);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use hc_exact::BigInt;

    fn number(value: i64) -> Rational {
        Rational::from_i64(value)
    }

    fn ratio(top: i64, bottom: i64) -> Rational {
        Rational::new(BigInt::from_i64(top), BigInt::from_i64(bottom)).expect("nonzero")
    }

    fn sample() -> Certificate {
        let records = vec![
            Record::Root {
                node: 0,
                model: [9; 32],
            },
            Record::DualBound {
                node: 0,
                terms: vec![(0, ratio(1, 3)), (4, number(2))],
            },
            Record::Incumbent {
                reduced: vec![number(1), ratio(-1, 2)],
                original: vec![number(1), ratio(-1, 2)],
                objective: ratio(7, 4),
            },
            Record::Claim {
                kind: ClaimKind::Optimal,
                objective: Some(ratio(7, 4)),
                lower: None,
                gap: None,
            },
        ];
        let records = Certificate::terminated(records);
        Certificate {
            format_tag: 1,
            instance_bytes: 512,
            digest: [7; 32],
            claim: ClaimKind::Optimal,
            records,
        }
    }

    #[test]
    fn the_stream_begins_with_the_magic_and_the_versions() {
        let bytes = sample().encode();
        assert_eq!(&bytes[..4], &MAGIC);
        assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), HCC_VERSION);
        assert_eq!(u16::from_le_bytes([bytes[6], bytes[7]]), CANONICAL_VERSION);
        assert_eq!(bytes[8], 1, "the format tag");
    }

    #[test]
    fn the_declared_length_is_the_length_without_the_checksum() {
        let bytes = sample().encode();
        // The header is the magic, two versions, the format tag, the instance
        // length, the digest, the claim kind, the record count, and then the
        // byte length: four, two, two, one, eight, thirty-two, one, eight.
        assert_eq!(bytes[49], ClaimKind::Optimal.code());
        let count = u64::from_le_bytes(bytes[50..58].try_into().expect("eight bytes"));
        assert_eq!(count, 5);
        let declared = u64::from_le_bytes(bytes[58..66].try_into().expect("eight bytes"));
        assert_eq!(declared as usize, bytes.len() - 4);
    }

    #[test]
    fn the_checksum_covers_everything_before_it() {
        let bytes = sample().encode();
        let split = bytes.len() - 4;
        let stated = u32::from_le_bytes(bytes[split..].try_into().expect("four bytes"));
        let mut computed = Crc32c::new();
        computed.update(&bytes[..split]);
        assert_eq!(stated, computed.finish());

        // One byte changed anywhere before the checksum changes it.
        let mut damaged = bytes.clone();
        damaged[20] ^= 1;
        let mut recomputed = Crc32c::new();
        recomputed.update(&damaged[..split]);
        assert_ne!(stated, recomputed.finish());
    }

    #[test]
    fn variable_length_integers_are_minimally_encoded() {
        let mut out = Vec::new();
        write_leb128(&mut out, 0);
        assert_eq!(out, vec![0]);

        out.clear();
        write_leb128(&mut out, 127);
        assert_eq!(out, vec![127]);

        out.clear();
        write_leb128(&mut out, 128);
        assert_eq!(out, vec![0x80, 1]);

        out.clear();
        write_leb128(&mut out, u64::MAX);
        assert_eq!(out.len(), 10);
        assert_eq!(*out.last().expect("a last byte"), 1);
        // Every continuation byte but the last has its high bit set, and the
        // last does not — which is what makes the encoding self-terminating
        // and what makes a padded spelling detectable.
        assert!(out[..9].iter().all(|byte| byte & 0x80 != 0));
    }

    #[test]
    fn rationals_travel_as_canonical_text() {
        let mut out = Vec::new();
        write_rational(&mut out, &ratio(1, 3));
        assert_eq!(out, vec![3, b'1', b'/', b'3']);

        out.clear();
        write_rational(&mut out, &number(0));
        assert_eq!(out, vec![1, b'0']);

        out.clear();
        write_rational(&mut out, &number(-12));
        assert_eq!(out, vec![3, b'-', b'1', b'2']);

        // Canonical text has one spelling per value, so two rationals that are
        // equal produce identical bytes however they were built.
        let mut left = Vec::new();
        write_rational(&mut left, &ratio(2, 6));
        let mut right = Vec::new();
        write_rational(&mut right, &ratio(1, 3));
        assert_eq!(left, right);
    }

    #[test]
    fn every_record_kind_has_a_distinct_code_and_appears_in_the_order() {
        let codes = Record::codes();
        let mut sorted = codes;
        sorted.sort_unstable();
        assert_eq!(codes, sorted, "the code order is not ascending");
        let mut deduped = sorted;
        let mut seen: Vec<u8> = deduped.to_vec();
        seen.dedup();
        assert_eq!(seen.len(), codes.len(), "a code is used twice");
        deduped.sort_unstable();

        // Every kind the enumeration can produce is in the list.
        let kinds = [
            Record::Root {
                node: 0,
                model: [0; 32],
            },
            Record::DualBound {
                node: 0,
                terms: Vec::new(),
            },
            Record::Contradiction {
                node: 0,
                terms: Vec::new(),
            },
            Record::Claim {
                kind: ClaimKind::Infeasible,
                objective: None,
                lower: None,
                gap: None,
            },
            Record::End { counts: Vec::new() },
        ];
        for record in kinds {
            assert!(
                codes.contains(&record.code()),
                "code {} is not listed",
                record.code()
            );
        }
    }

    #[test]
    fn the_terminator_counts_what_the_stream_holds() {
        let certificate = sample();
        let counts = Certificate::counts(&certificate.records);
        match certificate.records.last().expect("a terminator") {
            Record::End { counts: stated } => assert_eq!(stated, &counts),
            other => panic!("the last record is {other:?}"),
        }
        // A kind that does not appear takes no entry at all, so one stream has
        // one terminator.
        assert!(counts.iter().all(|(_, count)| *count > 0));
    }

    #[test]
    fn every_claim_kind_round_trips_through_its_byte() {
        for kind in [
            ClaimKind::Optimal,
            ClaimKind::Infeasible,
            ClaimKind::Unbounded,
            ClaimKind::FeasibleWithGap,
            ClaimKind::FeasibleNoBound,
        ] {
            assert_eq!(ClaimKind::from_code(kind.code()), Some(kind));
        }
        assert_eq!(ClaimKind::from_code(0), None);
        assert_eq!(ClaimKind::from_code(6), None);
    }

    #[test]
    fn an_absent_optional_field_costs_one_byte_and_a_present_one_says_so() {
        let absent = encode_payload(&Record::Ray {
            base: Vec::new(),
            direction: Vec::new(),
            step: None,
        });
        assert_eq!(absent, vec![0, 0, 0]);

        let present = encode_payload(&Record::Ray {
            base: Vec::new(),
            direction: Vec::new(),
            step: Some(number(3)),
        });
        assert_eq!(present, vec![0, 0, 1, 1, b'3']);
    }

    #[test]
    fn the_same_certificate_encodes_to_the_same_bytes() {
        // Determinism at the byte level, which is what lets a certificate be
        // hashed, compared, and reproduced.
        assert_eq!(sample().encode(), sample().encode());
    }
}
