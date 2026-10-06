//! Reading a certificate, on the side that does not trust it.
//!
//! This is the second implementation of the encoding in `spec/v1/hcc.toml`,
//! written against the specification rather than against the writer, and it is
//! held to the same annotated bytes under `spec/v1/vectors/hcc/`. Where the
//! writer's reader is arranged for convenience, this one is arranged around a
//! single rule: a length in the input is a claim, never an instruction.
//!
//! Three consequences of that rule are visible throughout. Nothing is reserved
//! from a declared count — every vector grows as elements actually arrive, so
//! a header claiming four billion records costs four bytes rather than an
//! allocation. Every variable-length integer must be minimally encoded, so one
//! number has one spelling and one proof has one byte stream. And the checksum
//! is verified before a single record is interpreted, which is the one thing
//! it is good for: a truncated file should be diagnosed as truncated, not as
//! an unsound inference.
//!
//! The reader is flat. There is no recursion anywhere in it, because a legal
//! proof may be deeper than the process call stack and a verifier that
//! overflows on a valid certificate has rejected it for a reason that has
//! nothing to do with mathematics.
//!
//! What comes out of here is structure and nothing more. No record is believed
//! by having been parsed; the rules that decide whether any of it is true live
//! elsewhere and recompute every number they are shown.

use crate::vhash::{checksum, instance_digest};
use crate::vrat::Frac;
use crate::vreject::{Code, Rejected};
use crate::vtext::NumberError;

/// The bytes every certificate begins with.
pub const MAGIC: [u8; 4] = *b"HCC1";
/// The certificate format version this reader accepts.
pub const FORMAT_VERSION: u16 = 1;
/// The canonicalization version a certificate must be about.
pub const MODEL_VERSION: u16 = 1;
/// The fixed size of the header, in bytes.
pub const HEADER_BYTES: usize = 66;
/// The size of the trailing checksum, in bytes.
pub const CHECKSUM_BYTES: usize = 4;

/// The largest certificate this reader will look at, in bytes.
pub const BYTE_CAP: u64 = 1_073_741_824;
/// The largest number of records a certificate may carry.
pub const RECORD_CAP: u64 = 20_000_000;
/// The largest rational, in bits.
pub const RATIONAL_BIT_CAP: u64 = 1_048_576;
/// The largest rational token, in bytes.
pub const RATIONAL_TOKEN_CAP: u64 = 262_144;

/// What a certificate claims.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Claim {
    /// The stated objective is attained and nothing feasible is smaller.
    Optimal,
    /// The model has no feasible point.
    Infeasible,
    /// The objective decreases without limit.
    Unbounded,
    /// A point, a proved lower bound, and the exact distance between them.
    Gap,
    /// A point, and no statement about any lower bound.
    PointOnly,
}

impl Claim {
    /// The kind a header byte names.
    #[must_use]
    pub const fn of_byte(byte: u8) -> Option<Self> {
        match byte {
            1 => Some(Self::Optimal),
            2 => Some(Self::Infeasible),
            3 => Some(Self::Unbounded),
            4 => Some(Self::Gap),
            5 => Some(Self::PointOnly),
            _ => None,
        }
    }

    /// The word this crate prints for the kind.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Optimal => "optimal",
            Self::Infeasible => "infeasible",
            Self::Unbounded => "unbounded",
            Self::Gap => "feasible with a certified gap",
            Self::PointOnly => "feasible with no certified bound",
        }
    }
}

/// One record, as read.
///
/// The variants carry what the bytes said and nothing derived from them. A
/// field named `stated` is one the rules must recompute rather than adopt; the
/// name is a reminder at every use site.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// A presolve equivalence and the hash of the model it is said to produce.
    Transform {
        /// The rule identifier.
        rule: u64,
        /// The rule's parameters.
        parameters: Vec<Frac>,
        /// The hash the record states for its result.
        stated_hash: [u8; 32],
    },
    /// A presolve step whose domain came out empty.
    TransformContradiction {
        /// The rule identifier.
        rule: u64,
        /// The inequalities the empty domain is said to follow from.
        sources: Vec<u64>,
    },
    /// The root of the tree, and the model it is about.
    Root {
        /// Always zero.
        node: u64,
        /// The hash the record states for the reduced model.
        stated_hash: [u8; 32],
    },
    /// A lower bound from nonnegative multipliers over visible inequalities.
    DualBound {
        /// The node the inference is made at.
        node: u64,
        /// Cited inequalities and their multipliers.
        terms: Vec<(u64, Frac)>,
    },
    /// An infeasibility from nonnegative multipliers over visible
    /// inequalities.
    Contradiction {
        /// The node the inference is made at.
        node: u64,
        /// Cited inequalities and their multipliers.
        terms: Vec<(u64, Frac)>,
    },
    /// A Gomory mixed-integer cut and the combination it came from.
    GmiCut {
        /// The node whose subtree the cut may be used in.
        scope: u64,
        /// The combination over visible inequalities.
        combination: Vec<(u64, Frac)>,
        /// The integer variable the combination isolates.
        isolated: u64,
        /// The coefficients the record states.
        stated_coefficients: Vec<(u64, Frac)>,
        /// The right-hand side the record states.
        stated_right: Frac,
    },
    /// A knapsack cover cut and its lifting.
    CoverCut {
        /// The node whose subtree the cut may be used in.
        scope: u64,
        /// The inequality the cover was taken from.
        source: u64,
        /// The cover's members.
        cover: Vec<u64>,
        /// The order lifting was performed in.
        lift_order: Vec<u64>,
        /// The coefficients the record states.
        stated_coefficients: Vec<(u64, Frac)>,
    },
    /// A two-way integer split.
    Branch {
        /// The node being split.
        parent: u64,
        /// The variable split on.
        variable: u64,
        /// The integer the split is made at.
        split: Frac,
        /// The child taking the at-most side.
        left: u64,
        /// The child taking the at-least side.
        right: u64,
    },
    /// A node closed by a bound.
    LeafBound {
        /// The node.
        node: u64,
        /// The dual bound cited.
        bound: u64,
    },
    /// A node closed by a contradiction.
    LeafInfeas {
        /// The node.
        node: u64,
        /// The contradiction cited.
        contradiction: u64,
    },
    /// A node closed by an integral point attaining its own bound.
    LeafIntegral {
        /// The node.
        node: u64,
        /// The point, in the reduced model's coordinates.
        point: Vec<Frac>,
        /// The dual bound the point is said to attain.
        bound: u64,
    },
    /// A node left open, with a bound.
    LeafFrontier {
        /// The node.
        node: u64,
        /// The dual bound cited.
        bound: u64,
    },
    /// The best point found, in both coordinate systems.
    Incumbent {
        /// The point in the reduced model's coordinates.
        reduced: Vec<Frac>,
        /// The point in the original model's coordinates.
        original: Vec<Frac>,
        /// The objective the record states.
        stated_objective: Frac,
    },
    /// A point, supporting nothing else.
    FeasiblePoint {
        /// The point in the original model's coordinates.
        point: Vec<Frac>,
        /// The objective the record states.
        stated_objective: Frac,
    },
    /// A direction the objective improves along without limit.
    Ray {
        /// A point to travel from.
        base: Vec<Frac>,
        /// The direction.
        direction: Vec<Frac>,
        /// The positive integer step, for a mixed-integer claim.
        step: Option<Frac>,
    },
    /// What the certificate claims, and the numbers it claims.
    Assertion {
        /// The kind claimed.
        kind: Claim,
        /// The objective, where the claim has one.
        stated_objective: Option<Frac>,
        /// The lower bound, where the claim has one.
        stated_lower: Option<Frac>,
        /// The relative gap, where the claim has one.
        stated_gap: Option<Frac>,
    },
    /// The terminator, with a count per record kind.
    Terminator {
        /// One count per record kind, ordered by type code.
        counts: Vec<(u8, u64)>,
    },
}

impl Step {
    /// The type code this record travels as.
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
            Self::Assertion { .. } => 96,
            Self::Terminator { .. } => 127,
        }
    }

    /// Whether the record is one a point-only or ray claim forbids.
    ///
    /// Those two claims are proved directly against the original model. A
    /// transform, a root, or a tree record alongside one would imply support
    /// the claim does not have, so their presence is refused rather than
    /// ignored.
    #[must_use]
    pub const fn is_structural(&self) -> bool {
        matches!(
            self,
            Self::Transform { .. }
                | Self::TransformContradiction { .. }
                | Self::Root { .. }
                | Self::DualBound { .. }
                | Self::Contradiction { .. }
                | Self::GmiCut { .. }
                | Self::CoverCut { .. }
                | Self::Branch { .. }
                | Self::LeafBound { .. }
                | Self::LeafInfeas { .. }
                | Self::LeafIntegral { .. }
                | Self::LeafFrontier { .. }
                | Self::Incumbent { .. }
        )
    }
}

/// A certificate, read but not believed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proof {
    /// The format tag the instance was read under.
    pub tag: u8,
    /// The instance length the header states.
    pub instance_bytes: u64,
    /// The instance digest the header states, already confirmed to be the
    /// digest of the instance this reader was given.
    pub digest: [u8; 32],
    /// The claim the header states.
    pub claim: Claim,
    /// The records, in stream order.
    pub steps: Vec<Step>,
    /// Where each record's payload sat, as a start and an end offset into the
    /// certificate, in the same order as `steps`.
    ///
    /// A reader's own bookkeeping, kept because a caller that wants to point
    /// at the bytes a record came from otherwise has to parse the stream a
    /// second time to find them. No rule consults it and nothing derived from
    /// a certificate's content depends on it; removing it would change no
    /// verdict.
    pub spans: Vec<(usize, usize)>,
    /// How many bytes the certificate occupied.
    pub bytes: usize,
}

impl Proof {
    /// The record at a stream identifier, when the identifier names one.
    ///
    /// Identifiers are the record's position, so this is an index. It is a
    /// method rather than an index expression so that a citation of something
    /// past the end is an absence rather than a panic.
    #[must_use]
    pub fn at(&self, id: u64) -> Option<&Step> {
        usize::try_from(id).ok().and_then(|at| self.steps.get(at))
    }

    /// How many records of a kind the certificate carries.
    #[must_use]
    pub fn tally(&self, code: u8) -> u64 {
        self.steps.iter().filter(|step| step.code() == code).count() as u64
    }
}

/// Reads a certificate and confirms it is about this instance.
///
/// The instance bytes are needed before any record is examined: a proof is
/// about one model, and a proof detached from its model and reattached to
/// another is the cheapest forgery there is.
///
/// # Errors
///
/// Returns [`Rejected`] naming the earliest rule that refused.
pub fn read(certificate: &[u8], instance: &[u8]) -> Result<Proof, Rejected> {
    let header = read_header(certificate, instance)?;

    // The checksum comes before interpretation, and only for the reason stated
    // at the top of this file. Nothing downstream leans on it.
    let split = certificate.len() - CHECKSUM_BYTES;
    let stated = u32::from_le_bytes([
        certificate[split],
        certificate[split + 1],
        certificate[split + 2],
        certificate[split + 3],
    ]);
    if checksum(&certificate[..split]) != stated {
        return Err(Rejected::new(
            Code::PARSE_CHECKSUM,
            "the checksum does not match the bytes it covers",
        ));
    }

    let mut scan = Scan {
        bytes: &certificate[..split],
        at: HEADER_BYTES,
    };
    let mut steps: Vec<Step> = Vec::new();
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut previous: Option<u64> = None;
    loop {
        if scan.done() {
            // The stream ran out without a terminator. Reported as truncation
            // rather than as a missing record, because that is what it is.
            return Err(Rejected::new(
                Code::PARSE_TRUNCATED,
                "the stream ends without a terminating record",
            ));
        }
        let code = scan.byte()?;
        let id = scan.varint()?;
        if let Some(last) = previous
            && id <= last
        {
            return Err(Rejected::new(
                Code::PARSE_IDENTIFIER_ORDER,
                format!("record identifier {id} does not follow {last}"),
            ));
        }
        previous = Some(id);

        let length = scan.varint()?;
        let length = usize::try_from(length).map_err(|_| {
            Rejected::new(
                Code::PARSE_TRUNCATED,
                "a record declares a length larger than the stream",
            )
        })?;
        let start = scan.at;
        if scan.remaining() < length {
            return Err(Rejected::new(
                Code::PARSE_TRUNCATED,
                format!("a record declares {length} payload bytes and fewer remain"),
            ));
        }

        let step = read_payload(&mut scan, code)?;
        if scan.at - start != length {
            return Err(Rejected::new(
                Code::PARSE_RECORD_LENGTH,
                format!(
                    "a record of type {code} declared {length} payload bytes and used {}",
                    scan.at - start
                ),
            ));
        }

        steps.push(step);
        spans.push((start, scan.at));
        if steps.len() as u64 > RECORD_CAP {
            return Err(Rejected::new(
                Code::PARSE_RECORD_COUNT,
                format!("the stream carries more than {RECORD_CAP} records"),
            ));
        }
        if matches!(steps.last(), Some(Step::Terminator { .. })) {
            break;
        }
    }

    if !scan.done() {
        return Err(Rejected::new(
            Code::PARSE_TRAILING,
            format!("{} bytes follow the terminating record", scan.remaining()),
        ));
    }
    if steps.len() as u64 != header.declared_records {
        return Err(Rejected::new(
            Code::PARSE_END_COUNTS,
            format!(
                "the header declares {} records and the stream carries {}",
                header.declared_records,
                steps.len()
            ),
        ));
    }

    let proof = Proof {
        tag: header.tag,
        instance_bytes: header.instance_bytes,
        digest: header.digest,
        claim: header.claim,
        bytes: certificate.len(),
        steps,
        spans,
    };
    check_counts(&proof)?;
    Ok(proof)
}

/// What the fixed part of a certificate says.
struct Header {
    tag: u8,
    instance_bytes: u64,
    digest: [u8; 32],
    claim: Claim,
    declared_records: u64,
}

/// Reads and checks the fixed header, including the binding to the instance.
fn read_header(certificate: &[u8], instance: &[u8]) -> Result<Header, Rejected> {
    if certificate.len() < HEADER_BYTES + CHECKSUM_BYTES {
        return Err(Rejected::new(
            Code::PARSE_TRUNCATED,
            format!(
                "a certificate is at least {} bytes and this one is {}",
                HEADER_BYTES + CHECKSUM_BYTES,
                certificate.len()
            ),
        ));
    }
    if certificate[..4] != MAGIC {
        return Err(Rejected::new(
            Code::PARSE_MAGIC,
            "the first four bytes are not the certificate magic",
        ));
    }

    let format = u16::from_le_bytes([certificate[4], certificate[5]]);
    let model = u16::from_le_bytes([certificate[6], certificate[7]]);
    if format != FORMAT_VERSION || model != MODEL_VERSION {
        return Err(Rejected::new(
            Code::PARSE_VERSION,
            format!("certificate version {format} and canonicalization version {model}"),
        ));
    }

    let tag = certificate[8];
    let instance_bytes = word(&certificate[9..17]);
    let mut digest = [0_u8; 32];
    digest.copy_from_slice(&certificate[17..49]);

    let Some(claim) = Claim::of_byte(certificate[49]) else {
        return Err(Rejected::new(
            Code::PARSE_UNKNOWN_RECORD,
            format!("claim kind {} is outside the closed list", certificate[49]),
        ));
    };
    let declared_records = word(&certificate[50..58]);
    let declared_bytes = word(&certificate[58..66]);

    // Both declared sizes meet their caps before anything else looks at them,
    // and neither is used to size an allocation at any point.
    if declared_records > RECORD_CAP {
        return Err(Rejected::new(
            Code::PARSE_DECLARED_TOO_LARGE,
            format!("the header declares {declared_records} records against a cap of {RECORD_CAP}"),
        ));
    }
    if declared_bytes > BYTE_CAP {
        return Err(Rejected::new(
            Code::PARSE_DECLARED_TOO_LARGE,
            format!("the header declares {declared_bytes} bytes against a cap of {BYTE_CAP}"),
        ));
    }
    if certificate.len() as u64 > BYTE_CAP {
        return Err(Rejected::new(
            Code::PARSE_BYTE_COUNT,
            format!("the certificate is {} bytes", certificate.len()),
        ));
    }
    if declared_bytes != (certificate.len() - CHECKSUM_BYTES) as u64 {
        return Err(Rejected::new(
            Code::PARSE_TRUNCATED,
            format!(
                "the header declares {declared_bytes} bytes and {} are present",
                certificate.len() - CHECKSUM_BYTES
            ),
        ));
    }

    // The binding, before a single record is interpreted.
    if instance.len() as u64 != instance_bytes || instance_digest(tag, instance) != digest {
        return Err(Rejected::new(
            Code::PARSE_NOT_THIS_INSTANCE,
            "the certificate is bound to a different instance",
        ));
    }

    Ok(Header {
        tag,
        instance_bytes,
        digest,
        claim,
        declared_records,
    })
}

/// Checks the terminator's tallies against what the stream actually held.
fn check_counts(proof: &Proof) -> Result<(), Rejected> {
    let Some(Step::Terminator { counts }) = proof.steps.last() else {
        return Err(Rejected::new(
            Code::PARSE_TRUNCATED,
            "the stream ends without a terminating record",
        ));
    };
    let mut previous: Option<u8> = None;
    for (code, count) in counts {
        if previous.is_some_and(|last| *code <= last) {
            return Err(Rejected::new(
                Code::PARSE_SET_ORDER,
                format!("the terminator's counts revisit record type {code}"),
            ));
        }
        previous = Some(*code);
        let actual = proof.tally(*code);
        if actual != *count {
            return Err(Rejected::new(
                Code::PARSE_END_COUNTS,
                format!(
                    "the terminator counts {count} of record type {code} and the stream holds {actual}"
                ),
            ));
        }
    }
    let named: Vec<u8> = counts.iter().map(|(code, _)| *code).collect();
    for step in &proof.steps {
        if !named.contains(&step.code()) {
            return Err(Rejected::new(
                Code::PARSE_END_COUNTS,
                format!("the terminator omits record type {}", step.code()),
            ));
        }
    }
    Ok(())
}

/// A little-endian eight-byte word.
fn word(bytes: &[u8]) -> u64 {
    let mut value = 0_u64;
    for (index, byte) in bytes.iter().enumerate() {
        value |= u64::from(*byte) << (8 * index);
    }
    value
}

/// A position in the stream.
///
/// Deliberately the simplest thing that can work: a slice and an offset, with
/// every read checking the offset against the length. There is no buffering,
/// no lookahead, and no state that could disagree with the offset.
struct Scan<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Scan<'_> {
    fn remaining(&self) -> usize {
        self.bytes.len() - self.at
    }

    fn done(&self) -> bool {
        self.at >= self.bytes.len()
    }

    fn byte(&mut self) -> Result<u8, Rejected> {
        if self.done() {
            return Err(Rejected::new(
                Code::PARSE_TRUNCATED,
                "the stream ends where a byte was expected",
            ));
        }
        let value = self.bytes[self.at];
        self.at += 1;
        Ok(value)
    }

    fn take(&mut self, count: usize) -> Result<&[u8], Rejected> {
        if self.remaining() < count {
            return Err(Rejected::new(
                Code::PARSE_TRUNCATED,
                format!("the stream ends where {count} bytes were expected"),
            ));
        }
        let slice = &self.bytes[self.at..self.at + count];
        self.at += count;
        Ok(slice)
    }

    fn hash(&mut self) -> Result<[u8; 32], Rejected> {
        let slice = self.take(32)?;
        let mut out = [0_u8; 32];
        out.copy_from_slice(slice);
        Ok(out)
    }

    /// A variable-length unsigned integer, refusing every non-minimal
    /// encoding.
    ///
    /// A continuation byte that adds nothing is a second spelling of the same
    /// number, and two spellings of a number are two byte streams for one
    /// proof.
    fn varint(&mut self) -> Result<u64, Rejected> {
        let mut value = 0_u64;
        let mut shift = 0_u32;
        loop {
            let byte = self.byte()?;
            let payload = u64::from(byte & 0x7f);
            if shift >= 64 || (shift == 63 && payload > 1) {
                return Err(Rejected::new(
                    Code::PARSE_LENGTH_OVERFLOW,
                    "a variable-length integer does not fit in sixty-four bits",
                ));
            }
            value |= payload << shift;
            if byte & 0x80 == 0 {
                if shift > 0 && payload == 0 {
                    return Err(Rejected::new(
                        Code::PARSE_NON_MINIMAL,
                        "a variable-length integer ends in a byte that adds nothing",
                    ));
                }
                return Ok(value);
            }
            shift += 7;
        }
    }

    /// A rational, as canonical text with a length prefix.
    fn rational(&mut self) -> Result<Frac, Rejected> {
        let length = self.varint()?;
        if length > RATIONAL_TOKEN_CAP {
            return Err(Rejected::new(
                Code::PARSE_RATIONAL_TOKEN,
                format!("a rational token declares {length} bytes"),
            ));
        }
        let length = usize::try_from(length).map_err(|_| {
            Rejected::new(Code::PARSE_RATIONAL_TOKEN, "a rational token is too long")
        })?;
        let slice = self.take(length)?;
        let text = core::str::from_utf8(slice).map_err(|_| {
            Rejected::new(
                Code::PARSE_RATIONAL_FORM,
                "a rational token is not valid text",
            )
        })?;
        let value = Frac::from_canonical(text).map_err(|error| {
            let code = match error {
                NumberError::TooWide => Code::PARSE_RATIONAL_BITS,
                _ => Code::PARSE_RATIONAL_FORM,
            };
            Rejected::new(code, format!("{error}: {text}"))
        })?;
        if value.bits() > RATIONAL_BIT_CAP {
            return Err(Rejected::new(
                Code::PARSE_RATIONAL_BITS,
                format!("a rational occupies {} bits", value.bits()),
            ));
        }
        Ok(value)
    }

    /// A counted array of rationals.
    ///
    /// The count is read and then ignored as an allocation hint: the vector
    /// grows as elements arrive, so a declared count larger than the stream
    /// costs the bytes of the count and nothing more.
    fn rationals(&mut self) -> Result<Vec<Frac>, Rejected> {
        let count = self.varint()?;
        let mut out = Vec::new();
        for _ in 0..count {
            out.push(self.rational()?);
        }
        Ok(out)
    }

    /// A counted array of identifier and multiplier pairs, strictly ascending.
    fn terms(&mut self) -> Result<Vec<(u64, Frac)>, Rejected> {
        let count = self.varint()?;
        let mut out: Vec<(u64, Frac)> = Vec::new();
        for _ in 0..count {
            let id = self.varint()?;
            let value = self.rational()?;
            if out.last().is_some_and(|(last, _)| *last >= id) {
                return Err(Rejected::new(
                    Code::PARSE_SET_ORDER,
                    format!("term identifiers are not strictly ascending at {id}"),
                ));
            }
            out.push((id, value));
        }
        Ok(out)
    }

    /// A counted array of identifiers, strictly ascending.
    fn ids(&mut self) -> Result<Vec<u64>, Rejected> {
        let count = self.varint()?;
        let mut out: Vec<u64> = Vec::new();
        for _ in 0..count {
            let id = self.varint()?;
            if out.last().is_some_and(|last| *last >= id) {
                return Err(Rejected::new(
                    Code::PARSE_SET_ORDER,
                    format!("identifiers are not strictly ascending at {id}"),
                ));
            }
            out.push(id);
        }
        Ok(out)
    }

    /// A counted array of numbers with no ordering rule at all.
    ///
    /// A transform contradiction's sources are the rule's index parameters —
    /// a row and a column, say — and nothing about them is a set. Requiring
    /// them to ascend would reject `row 0, column 0`, which is an ordinary
    /// derivation.
    fn plain(&mut self) -> Result<Vec<u64>, Rejected> {
        let count = self.varint()?;
        let mut out = Vec::new();
        for _ in 0..count {
            out.push(self.varint()?);
        }
        Ok(out)
    }

    /// An array of identifiers whose order is the record's meaning.
    ///
    /// A lift order is a sequence, not a set: it is the order the dynamic
    /// program ran in, and sorting it would change the coefficients. Repeats
    /// are still refused, because lifting a variable twice is not a thing that
    /// happens.
    fn sequence(&mut self) -> Result<Vec<u64>, Rejected> {
        let count = self.varint()?;
        let mut out: Vec<u64> = Vec::new();
        for _ in 0..count {
            let id = self.varint()?;
            if out.contains(&id) {
                return Err(Rejected::new(
                    Code::PARSE_SET_ORDER,
                    format!("identifier {id} appears twice in a sequence"),
                ));
            }
            out.push(id);
        }
        Ok(out)
    }

    /// An optional rational, behind a presence byte.
    fn optional(&mut self) -> Result<Option<Frac>, Rejected> {
        match self.byte()? {
            0 => Ok(None),
            1 => Ok(Some(self.rational()?)),
            other => Err(Rejected::new(
                Code::PARSE_RECORD_LENGTH,
                format!("a presence flag holds {other}"),
            )),
        }
    }
}

/// Reads one record's payload, given its type code.
fn read_payload(scan: &mut Scan<'_>, code: u8) -> Result<Step, Rejected> {
    Ok(match code {
        1 => Step::Transform {
            rule: scan.varint()?,
            parameters: scan.rationals()?,
            stated_hash: scan.hash()?,
        },
        2 => Step::TransformContradiction {
            rule: scan.varint()?,
            sources: scan.plain()?,
        },
        3 => Step::Root {
            node: scan.varint()?,
            stated_hash: scan.hash()?,
        },
        16 => Step::DualBound {
            node: scan.varint()?,
            terms: scan.terms()?,
        },
        17 => Step::Contradiction {
            node: scan.varint()?,
            terms: scan.terms()?,
        },
        32 => Step::GmiCut {
            scope: scan.varint()?,
            combination: scan.terms()?,
            isolated: scan.varint()?,
            stated_coefficients: scan.terms()?,
            stated_right: scan.rational()?,
        },
        33 => Step::CoverCut {
            scope: scan.varint()?,
            source: scan.varint()?,
            cover: scan.ids()?,
            lift_order: scan.sequence()?,
            stated_coefficients: scan.terms()?,
        },
        48 => Step::Branch {
            parent: scan.varint()?,
            variable: scan.varint()?,
            split: scan.rational()?,
            left: scan.varint()?,
            right: scan.varint()?,
        },
        64 => Step::LeafBound {
            node: scan.varint()?,
            bound: scan.varint()?,
        },
        65 => Step::LeafInfeas {
            node: scan.varint()?,
            contradiction: scan.varint()?,
        },
        66 => Step::LeafIntegral {
            node: scan.varint()?,
            point: scan.rationals()?,
            bound: scan.varint()?,
        },
        67 => Step::LeafFrontier {
            node: scan.varint()?,
            bound: scan.varint()?,
        },
        80 => Step::Incumbent {
            reduced: scan.rationals()?,
            original: scan.rationals()?,
            stated_objective: scan.rational()?,
        },
        81 => Step::FeasiblePoint {
            point: scan.rationals()?,
            stated_objective: scan.rational()?,
        },
        82 => Step::Ray {
            base: scan.rationals()?,
            direction: scan.rationals()?,
            step: scan.optional()?,
        },
        96 => {
            let byte = scan.byte()?;
            let Some(kind) = Claim::of_byte(byte) else {
                return Err(Rejected::new(
                    Code::PARSE_UNKNOWN_RECORD,
                    format!("claim kind {byte} is outside the closed list"),
                ));
            };
            Step::Assertion {
                kind,
                stated_objective: scan.optional()?,
                stated_lower: scan.optional()?,
                stated_gap: scan.optional()?,
            }
        }
        127 => {
            let count = scan.varint()?;
            let mut counts: Vec<(u8, u64)> = Vec::new();
            for _ in 0..count {
                let kind = scan.byte()?;
                let tally = scan.varint()?;
                if counts.last().is_some_and(|(last, _)| *last >= kind) {
                    return Err(Rejected::new(
                        Code::PARSE_SET_ORDER,
                        format!("the terminator's record types are not ascending at {kind}"),
                    ));
                }
                counts.push((kind, tally));
            }
            Step::Terminator { counts }
        }
        other => {
            return Err(Rejected::new(
                Code::PARSE_UNKNOWN_RECORD,
                format!("record type {other} is outside the closed list"),
            ));
        }
    })
}

/// Strips the annotation from a hexadecimal vector file.
///
/// The certificate vectors are written as commented hexadecimal so that the
/// bytes and the reasoning about them are one artifact. This is the one place
/// that knows how to undo that, and it is `pub` because the tests on both
/// sides of the project read the same files.
///
/// # Errors
///
/// Returns the offending line's one-based number when a line holds something
/// that is neither a comment nor a pair of hexadecimal digits.
pub fn from_annotated_hex(text: &str) -> Result<Vec<u8>, usize> {
    let mut out = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let body = match line.find('#') {
            Some(at) => &line[..at],
            None => line,
        };
        for token in body.split_whitespace() {
            if token.len() != 2 {
                return Err(index + 1);
            }
            let value = u8::from_str_radix(token, 16).map_err(|_| index + 1)?;
            out.push(value);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VECTOR: &str = include_str!("../../../spec/v1/vectors/hcc/tiny-optimal.hex");
    const INSTANCE: &[u8] = include_bytes!("../../../spec/v1/vectors/mps/tiny.mps");

    fn bytes() -> Vec<u8> {
        from_annotated_hex(VECTOR).expect("the frozen vector is hexadecimal")
    }

    /// Re-checksums a mutated stream so that a test of a later rule is not
    /// answered by the checksum.
    fn reseal(mut stream: Vec<u8>) -> Vec<u8> {
        let split = stream.len() - CHECKSUM_BYTES;
        let sum = checksum(&stream[..split]).to_le_bytes();
        stream[split..].copy_from_slice(&sum);
        stream
    }

    #[test]
    fn the_frozen_vector_is_the_length_its_manifest_states() {
        assert_eq!(bytes().len(), 150);
    }

    #[test]
    fn the_frozen_vector_reads_as_the_proof_it_documents() {
        let proof = read(&bytes(), INSTANCE).expect("the frozen vector is accepted");
        assert_eq!(proof.claim, Claim::Optimal);
        assert_eq!(proof.tag, 1);
        assert_eq!(proof.instance_bytes, INSTANCE.len() as u64);
        assert_eq!(proof.steps.len(), 5);
        assert_eq!(proof.bytes, 150);

        let Some(Step::DualBound { node, terms }) = proof.at(1) else {
            panic!("record 1 is the dual bound");
        };
        assert_eq!(*node, 0);
        assert_eq!(terms.len(), 1);
        assert_eq!(terms[0].0, 0);
        assert_eq!(terms[0].1.to_canonical(), "1");

        let Some(Step::Assertion {
            kind,
            stated_objective,
            stated_lower,
            stated_gap,
        }) = proof.at(3)
        else {
            panic!("record 3 is the claim");
        };
        assert_eq!(*kind, Claim::Optimal);
        assert_eq!(
            stated_objective.as_ref().map(Frac::to_canonical),
            Some("-4".to_owned())
        );
        assert_eq!(*stated_lower, None);
        assert_eq!(*stated_gap, None);
    }

    #[test]
    fn every_record_reports_where_its_payload_sat() {
        let stream = bytes();
        let proof = read(&stream, INSTANCE).expect("the frozen vector is accepted");
        assert_eq!(proof.spans.len(), proof.steps.len());

        // The spans partition the stream between the header and the checksum,
        // in order, with only each record's own type, identifier and length
        // between one payload and the next.
        let mut previous = HEADER_BYTES;
        for (at, (start, end)) in proof.spans.iter().copied().enumerate() {
            assert!(
                start >= previous,
                "record {at} begins before record {at} - 1 ends"
            );
            assert!(start <= end, "record {at} ends before it begins");
            assert!(
                end <= stream.len() - CHECKSUM_BYTES,
                "record {at} runs into the checksum"
            );
            previous = end;
        }
        assert_eq!(
            previous,
            stream.len() - CHECKSUM_BYTES,
            "the last payload does not end where the checksum begins"
        );

        // And a span names its own record's bytes: the dual bound's payload
        // starts with the node it is made at, which is zero.
        let (start, end) = proof.spans[1];
        assert!(matches!(proof.at(1), Some(Step::DualBound { .. })));
        assert!(end > start);
        assert_eq!(stream[start], 0);
    }

    #[test]
    fn a_certificate_for_another_instance_is_refused_before_any_record() {
        let mut other = INSTANCE.to_vec();
        other.push(b'\n');
        let refusal = read(&bytes(), &other).expect_err("the binding must fail");
        assert_eq!(refusal.code, Code::PARSE_NOT_THIS_INSTANCE);
    }

    #[test]
    fn a_flipped_byte_is_reported_as_corruption_and_not_as_bad_mathematics() {
        let mut stream = bytes();
        stream[100] ^= 0x01;
        let refusal = read(&stream, INSTANCE).expect_err("the checksum must fail");
        assert_eq!(refusal.code, Code::PARSE_CHECKSUM);
    }

    #[test]
    fn truncation_is_reported_as_truncation() {
        let stream = bytes();
        for cut in [0, 10, 65, 100, 149] {
            let refusal = read(&stream[..cut], INSTANCE).expect_err("a short stream must fail");
            assert!(
                matches!(
                    refusal.code,
                    Code::PARSE_TRUNCATED | Code::PARSE_MAGIC | Code::PARSE_CHECKSUM
                ),
                "cutting at {cut} gave {refusal}"
            );
        }
    }

    #[test]
    fn the_magic_and_the_versions_are_checked_before_anything_else() {
        let mut stream = bytes();
        stream[0] = b'X';
        assert_eq!(
            read(&reseal(stream), INSTANCE).expect_err("bad magic").code,
            Code::PARSE_MAGIC
        );

        let mut stream = bytes();
        stream[4] = 2;
        assert_eq!(
            read(&reseal(stream), INSTANCE)
                .expect_err("bad version")
                .code,
            Code::PARSE_VERSION
        );
    }

    #[test]
    fn a_declared_record_count_above_the_cap_is_refused_without_allocating() {
        let mut stream = bytes();
        stream[50..58].copy_from_slice(&u64::MAX.to_le_bytes());
        let refusal = read(&reseal(stream), INSTANCE).expect_err("the cap must fire");
        assert_eq!(refusal.code, Code::PARSE_DECLARED_TOO_LARGE);
    }

    #[test]
    fn a_declared_byte_length_above_the_cap_is_refused_without_allocating() {
        let mut stream = bytes();
        stream[58..66].copy_from_slice(&u64::MAX.to_le_bytes());
        let refusal = read(&reseal(stream), INSTANCE).expect_err("the cap must fire");
        assert_eq!(refusal.code, Code::PARSE_DECLARED_TOO_LARGE);
    }

    /// A rational token that declares more bytes than the cap allows is
    /// refused before the length is used for anything.
    ///
    /// The frozen vector's one multiplier is the text "1", written as the
    /// length prefix `01` followed by `31`, at the end of the DualBound
    /// record. Respelling that prefix as a three-byte variable-length integer
    /// naming three hundred thousand is the smallest edit that reaches this
    /// cap: the record grows by one byte, so its payload length and the
    /// header's declared length both move with it, and the checksum is
    /// recomputed. What arrives at the reader is a well-formed stream whose
    /// only defect is a token nobody could afford to read.
    #[test]
    fn a_rational_token_above_the_cap_is_refused_without_allocating() {
        const TOKEN: usize = 108;
        let mut stream = bytes();
        assert_eq!(
            &stream[TOKEN..=TOKEN + 1],
            &[0x01, 0x31],
            "the multiplier token is not where the annotated vector says it is"
        );

        // 300_000 in the seven-bits-per-byte encoding, least significant group
        // first, every group but the last carrying the continuation bit.
        stream.splice(TOKEN..=TOKEN + 1, [0xE0, 0xA7, 0x12]);
        // The DualBound payload grew by one byte, and so did the stream the
        // header declares.
        stream[104] += 1;
        let declared = u64::from_le_bytes(stream[58..66].try_into().expect("eight bytes"));
        stream[58..66].copy_from_slice(&(declared + 1).to_le_bytes());

        let refusal = read(&reseal(stream), INSTANCE).expect_err("the cap must fire");
        assert_eq!(refusal.code, Code::PARSE_RATIONAL_TOKEN);
    }

    #[test]
    fn a_declared_byte_length_that_disagrees_with_the_stream_is_refused() {
        let mut stream = bytes();
        stream[58..66].copy_from_slice(&99_u64.to_le_bytes());
        let refusal = read(&reseal(stream), INSTANCE).expect_err("the length must fire");
        assert_eq!(refusal.code, Code::PARSE_TRUNCATED);
    }

    #[test]
    fn an_unknown_record_type_is_refused_rather_than_skipped() {
        let mut stream = bytes();
        stream[66] = 7;
        let refusal = read(&reseal(stream), INSTANCE).expect_err("the type must fire");
        assert_eq!(refusal.code, Code::PARSE_UNKNOWN_RECORD);
    }

    #[test]
    fn a_non_minimal_variable_length_integer_is_refused() {
        // The dual bound's multiplier count is a single byte holding one.
        // Respelling it as two bytes names the same number a second way.
        let mut stream = bytes();
        let at = stream
            .windows(5)
            .position(|window| window == [0x10, 0x01, 0x05, 0x00, 0x01])
            .expect("the dual bound record is in the stream")
            + 3;
        let mut respelt = stream.split_off(at);
        respelt.splice(1..2, [0x81, 0x00]);
        stream.extend(respelt);
        // The record's declared length grows with the respelling, so the only
        // rule left to fire is the minimality rule itself.
        stream[at - 1] = 0x06;
        let declared = (stream.len() - CHECKSUM_BYTES) as u64;
        stream[58..66].copy_from_slice(&declared.to_le_bytes());
        let refusal = read(&reseal(stream), INSTANCE).expect_err("minimality must fire");
        assert_eq!(refusal.code, Code::PARSE_NON_MINIMAL);
    }

    #[test]
    fn a_terminator_that_miscounts_the_stream_is_refused() {
        let mut stream = bytes();
        let at = stream.len() - CHECKSUM_BYTES - 1;
        stream[at] = 2;
        let refusal = read(&reseal(stream), INSTANCE).expect_err("the counts must fire");
        assert_eq!(refusal.code, Code::PARSE_END_COUNTS);
    }

    #[test]
    fn trailing_bytes_after_the_terminator_are_refused() {
        let mut stream = bytes();
        let mut tail = stream.split_off(stream.len() - CHECKSUM_BYTES);
        stream.push(0);
        stream.append(&mut tail);
        let declared = (stream.len() - CHECKSUM_BYTES) as u64;
        stream[58..66].copy_from_slice(&declared.to_le_bytes());
        let refusal = read(&reseal(stream), INSTANCE).expect_err("the trailer must fire");
        assert_eq!(refusal.code, Code::PARSE_TRAILING);
    }

    #[test]
    fn annotated_hexadecimal_reports_the_line_it_could_not_read() {
        assert_eq!(
            from_annotated_hex("00 ff # fine\n# comment\n"),
            Ok(vec![0, 255])
        );
        assert_eq!(from_annotated_hex("00\nzz\n"), Err(2));
        assert_eq!(from_annotated_hex("00\n123\n"), Err(2));
    }
}
