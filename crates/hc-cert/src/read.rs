//! Reading a certificate back.
//!
//! The solver writes certificates and also has to read them: to check its own
//! output before shipping it, to mutate a copy for the tamper corpus, and to
//! answer `verify` with the same words the standalone checker would use. This
//! is that reader. It is not the one that matters — the verifier has its own,
//! written separately, and that is the one whose acceptance means anything —
//! but it applies the same rules in the same order, and a disagreement between
//! the two is a finding.
//!
//! Every refusal names the rule that refused. The order the rules are applied
//! in is part of the contract: a corrupted file must be diagnosed as corrupted
//! rather than as an invalid inference, a file that stops early as truncated
//! rather than as a missing record, and a declared size is checked against its
//! cap before anything sized by it is reserved.

use hc_core::{Refusal, Rejection};
use hc_exact::Rational;

use crate::hcc::{
    CANONICAL_VERSION, Certificate, ClaimKind, HCC_VERSION, MAGIC, MAX_BYTES, MAX_RECORDS,
    RATIONAL_BITS, RATIONAL_TOKEN_BYTES, Record,
};

/// Bytes of header before the first record.
const HEADER_BYTES: usize = 4 + 2 + 2 + 1 + 8 + 32 + 1 + 8 + 8;

/// Reads a certificate.
///
/// # Errors
///
/// Returns a refusal naming the rule that refused the stream.
pub fn decode(bytes: &[u8]) -> Result<Certificate, Refusal> {
    if bytes.len() < 4 || bytes[..4] != MAGIC {
        return Err(Refusal::new(
            Rejection::HCC_MAGIC,
            "the stream does not begin with HCC1",
        ));
    }
    if bytes.len() < HEADER_BYTES + 4 {
        return Err(Refusal::new(
            Rejection::HCC_TRUNCATED,
            format!("the stream is {} bytes, shorter than a header", bytes.len()),
        ));
    }

    let hcc_version = u16::from_le_bytes([bytes[4], bytes[5]]);
    let canonical_version = u16::from_le_bytes([bytes[6], bytes[7]]);
    if hcc_version != HCC_VERSION || canonical_version != CANONICAL_VERSION {
        return Err(Refusal::new(
            Rejection::HCC_VERSION,
            format!("certificate version {hcc_version}, canonicalization {canonical_version}"),
        ));
    }

    let format_tag = bytes[8];
    let instance_bytes = u64::from_le_bytes(bytes[9..17].try_into().expect("eight bytes"));
    let mut digest = [0u8; 32];
    digest.copy_from_slice(&bytes[17..49]);
    let Some(claim) = ClaimKind::from_code(bytes[49]) else {
        return Err(Refusal::new(
            Rejection::HCC_UNKNOWN_RECORD,
            format!("claim kind {} is not one of the five", bytes[49]),
        ));
    };
    let declared_records = u64::from_le_bytes(bytes[50..58].try_into().expect("eight bytes"));
    let declared_bytes = u64::from_le_bytes(bytes[58..66].try_into().expect("eight bytes"));

    // Caps before anything sized by a declared number.
    if declared_records > MAX_RECORDS {
        return Err(Refusal::new(
            Rejection::HCC_DECLARED_TOO_LARGE,
            format!("{declared_records} records declared, over the cap of {MAX_RECORDS}"),
        ));
    }
    if declared_bytes > MAX_BYTES {
        return Err(Refusal::new(
            Rejection::HCC_DECLARED_TOO_LARGE,
            format!("{declared_bytes} bytes declared, over the cap of {MAX_BYTES}"),
        ));
    }

    let actual = bytes.len() as u64 - 4;
    if actual < declared_bytes {
        return Err(Refusal::new(
            Rejection::HCC_TRUNCATED,
            format!("the header declares {declared_bytes} bytes and the stream holds {actual}"),
        ));
    }
    if actual > declared_bytes {
        return Err(Refusal::new(
            Rejection::HCC_TRAILING_BYTES,
            format!("the header declares {declared_bytes} bytes and the stream holds {actual}"),
        ));
    }

    // The checksum, before any record is interpreted, so a corrupted file is
    // diagnosed as corrupted.
    let split = bytes.len() - 4;
    let stated = u32::from_le_bytes(bytes[split..].try_into().expect("four bytes"));
    let mut computed = hc_core::Crc32c::new();
    computed.update(&bytes[..split]);
    if computed.finish() != stated {
        return Err(Refusal::new(
            Rejection::HCC_CHECKSUM,
            format!(
                "checksum {stated:#010x}, computed {:#010x}",
                computed.finish()
            ),
        ));
    }

    let mut cursor = Cursor {
        bytes: &bytes[..split],
        at: HEADER_BYTES,
    };
    let mut records = Vec::new();
    let mut previous: Option<u64> = None;

    while !cursor.done() {
        if records.len() as u64 >= MAX_RECORDS {
            return Err(Refusal::new(
                Rejection::HCC_RECORD_COUNT,
                format!("more than {MAX_RECORDS} records"),
            ));
        }
        let code = cursor.byte()?;
        let id = cursor.leb128()?;
        if let Some(before) = previous {
            if id <= before {
                return Err(Refusal::new(
                    Rejection::HCC_IDENTIFIER_ORDER,
                    format!("identifier {id} does not follow {before}"),
                ));
            }
        }
        previous = Some(id);

        let length = cursor.leb128()?;
        let payload = cursor.take(length)?;
        let mut inner = Cursor {
            bytes: payload,
            at: 0,
        };
        let record = read_record(code, &mut inner)?;
        if !inner.done() {
            return Err(Refusal::new(
                Rejection::HCC_RECORD_LENGTH,
                format!("record {id} left {} bytes unread", inner.remaining()),
            ));
        }
        let terminal = matches!(record, Record::End { .. });
        records.push(record);
        if terminal {
            if !cursor.done() {
                return Err(Refusal::new(
                    Rejection::HCC_TRAILING_BYTES,
                    format!("{} bytes follow the terminating record", cursor.remaining()),
                ));
            }
            break;
        }
    }

    let Some(Record::End { counts }) = records.last() else {
        return Err(Refusal::new(
            Rejection::HCC_TRUNCATED,
            "the stream ends without a terminating record",
        ));
    };
    let expected = Certificate::counts(&records);
    if *counts != expected {
        return Err(Refusal::new(
            Rejection::HCC_END_COUNTS,
            format!("the terminator states {counts:?} and the stream holds {expected:?}"),
        ));
    }
    if records.len() as u64 != declared_records {
        return Err(Refusal::new(
            Rejection::HCC_END_COUNTS,
            format!(
                "the header declares {declared_records} records and the stream holds {}",
                records.len()
            ),
        ));
    }

    Ok(Certificate {
        format_tag,
        instance_bytes,
        digest,
        claim,
        records,
    })
}

/// Checks that a certificate is about a particular instance.
///
/// # Errors
///
/// Returns a refusal when the digest recomputed from the instance is not the
/// one the certificate carries.
pub fn bind(certificate: &Certificate, instance: &[u8]) -> Result<(), Refusal> {
    let computed = hc_core::instance_digest(certificate.format_tag, instance);
    if computed != certificate.digest {
        return Err(Refusal::new(
            Rejection::HCC_NOT_THIS_INSTANCE,
            format!(
                "the certificate is about {} and this instance is {}",
                hc_core::hex(&certificate.digest),
                hc_core::hex(&computed)
            ),
        ));
    }
    if certificate.instance_bytes != instance.len() as u64 {
        return Err(Refusal::new(
            Rejection::HCC_NOT_THIS_INSTANCE,
            format!(
                "the certificate is about a file of {} bytes and this one is {}",
                certificate.instance_bytes,
                instance.len()
            ),
        ));
    }
    Ok(())
}

/// A position in a byte stream.
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn done(&self) -> bool {
        self.at >= self.bytes.len()
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.at)
    }

    fn byte(&mut self) -> Result<u8, Refusal> {
        if self.done() {
            return Err(Refusal::new(
                Rejection::HCC_TRUNCATED,
                "the stream ends inside a record",
            ));
        }
        let value = self.bytes[self.at];
        self.at += 1;
        Ok(value)
    }

    fn take(&mut self, count: u64) -> Result<&'a [u8], Refusal> {
        let count = usize::try_from(count).map_err(|_| {
            Refusal::new(
                Rejection::HCC_LENGTH_OVERFLOW,
                "a declared length does not fit in memory",
            )
        })?;
        if self.remaining() < count {
            return Err(Refusal::new(
                Rejection::HCC_TRUNCATED,
                format!("{count} bytes declared and {} remain", self.remaining()),
            ));
        }
        let slice = &self.bytes[self.at..self.at + count];
        self.at += count;
        Ok(slice)
    }

    /// Reads a minimally encoded variable-length integer.
    ///
    /// A padded encoding is refused rather than accepted: it is a second
    /// spelling of the same number, and two spellings mean two byte streams
    /// for one proof.
    fn leb128(&mut self) -> Result<u64, Refusal> {
        let mut value = 0u64;
        let mut shift = 0u32;
        loop {
            let byte = self.byte()?;
            let payload = u64::from(byte & 0x7f);
            if shift >= 64 || (shift == 63 && payload > 1) {
                return Err(Refusal::new(
                    Rejection::HCC_LENGTH_OVERFLOW,
                    "a variable-length integer does not fit in sixty-four bits",
                ));
            }
            value |= payload << shift;
            if byte & 0x80 == 0 {
                // The final byte may only be zero when it is also the first:
                // otherwise it adds nothing and the value had a shorter
                // spelling.
                if byte == 0 && shift != 0 {
                    return Err(Refusal::new(
                        Rejection::HCC_NON_MINIMAL_LENGTH,
                        "a variable-length integer is padded",
                    ));
                }
                return Ok(value);
            }
            shift += 7;
        }
    }

    fn rational(&mut self) -> Result<Rational, Refusal> {
        let length = self.leb128()?;
        if length > RATIONAL_TOKEN_BYTES {
            return Err(Refusal::new(
                Rejection::HCC_RATIONAL_TOKEN,
                format!("a rational token of {length} bytes, over the cap"),
            ));
        }
        let raw = self.take(length)?;
        let text = core::str::from_utf8(raw).map_err(|_| {
            Refusal::new(Rejection::HCC_RATIONAL_FORM, "a rational token is not text")
        })?;
        let value = Rational::parse_canonical(text).map_err(|error| {
            Refusal::new(Rejection::HCC_RATIONAL_FORM, format!("{error}: {text:?}"))
        })?;
        if value.bit_len() > RATIONAL_BITS {
            return Err(Refusal::new(
                Rejection::HCC_RATIONAL_BITS,
                format!("a rational of {} bits, over the cap", value.bit_len()),
            ));
        }
        Ok(value)
    }

    fn rationals(&mut self) -> Result<Vec<Rational>, Refusal> {
        let count = self.leb128()?;
        // A count is a claim by the input. Nothing is reserved for it beyond
        // what the remaining bytes could possibly hold.
        if count > self.remaining() as u64 {
            return Err(Refusal::new(
                Rejection::HCC_TRUNCATED,
                format!(
                    "{count} elements declared and {} bytes remain",
                    self.remaining()
                ),
            ));
        }
        let mut out = Vec::with_capacity(count as usize);
        for _ in 0..count {
            out.push(self.rational()?);
        }
        Ok(out)
    }

    /// Reads a set-valued array of identifier and value pairs.
    fn terms(&mut self) -> Result<Vec<(u64, Rational)>, Refusal> {
        let count = self.leb128()?;
        if count > self.remaining() as u64 {
            return Err(Refusal::new(
                Rejection::HCC_TRUNCATED,
                format!(
                    "{count} elements declared and {} bytes remain",
                    self.remaining()
                ),
            ));
        }
        let mut out: Vec<(u64, Rational)> = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let id = self.leb128()?;
            if let Some((previous, _)) = out.last() {
                if id <= *previous {
                    return Err(Refusal::new(
                        Rejection::HCC_SET_ORDER,
                        format!("element {id} does not follow {previous}"),
                    ));
                }
            }
            out.push((id, self.rational()?));
        }
        Ok(out)
    }

    /// A counted array of numbers with no ordering rule.
    ///
    /// A transform contradiction's sources are the rule's index parameters,
    /// and nothing about them is a set: a derivation about row zero and column
    /// zero names the same number twice, legitimately.
    fn plain(&mut self) -> Result<Vec<u64>, Refusal> {
        let count = self.leb128()?;
        if count > self.remaining() as u64 {
            return Err(Refusal::new(
                Rejection::HCC_TRUNCATED,
                format!(
                    "{count} elements declared and {} bytes remain",
                    self.remaining()
                ),
            ));
        }
        let mut out: Vec<u64> = Vec::with_capacity(count as usize);
        for _ in 0..count {
            out.push(self.leb128()?);
        }
        Ok(out)
    }

    fn ids(&mut self) -> Result<Vec<u64>, Refusal> {
        let count = self.leb128()?;
        if count > self.remaining() as u64 {
            return Err(Refusal::new(
                Rejection::HCC_TRUNCATED,
                format!(
                    "{count} elements declared and {} bytes remain",
                    self.remaining()
                ),
            ));
        }
        let mut out: Vec<u64> = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let id = self.leb128()?;
            if let Some(previous) = out.last() {
                if id <= *previous {
                    return Err(Refusal::new(
                        Rejection::HCC_SET_ORDER,
                        format!("element {id} does not follow {previous}"),
                    ));
                }
            }
            out.push(id);
        }
        Ok(out)
    }

    fn hash(&mut self) -> Result<[u8; 32], Refusal> {
        let raw = self.take(32)?;
        let mut out = [0u8; 32];
        out.copy_from_slice(raw);
        Ok(out)
    }

    fn optional(&mut self) -> Result<Option<Rational>, Refusal> {
        match self.byte()? {
            0 => Ok(None),
            1 => Ok(Some(self.rational()?)),
            other => Err(Refusal::new(
                Rejection::HCC_RECORD_LENGTH,
                format!("an optional field is marked {other}, which is neither absent nor present"),
            )),
        }
    }
}

/// Reads one record's payload.
fn read_record(code: u8, cursor: &mut Cursor<'_>) -> Result<Record, Refusal> {
    match code {
        1 => Ok(Record::Transform {
            rule: cursor.leb128()?,
            parameters: cursor.rationals()?,
            result: cursor.hash()?,
        }),
        2 => Ok(Record::TransformContradiction {
            rule: cursor.leb128()?,
            sources: cursor.plain()?,
        }),
        3 => Ok(Record::Root {
            node: cursor.leb128()?,
            model: cursor.hash()?,
        }),
        16 => Ok(Record::DualBound {
            node: cursor.leb128()?,
            terms: cursor.terms()?,
        }),
        17 => Ok(Record::Contradiction {
            node: cursor.leb128()?,
            terms: cursor.terms()?,
        }),
        32 => Ok(Record::GmiCut {
            scope: cursor.leb128()?,
            combination: cursor.terms()?,
            isolated: cursor.leb128()?,
            coefficients: cursor.terms()?,
            right: cursor.rational()?,
        }),
        33 => Ok(Record::CoverCut {
            scope: cursor.leb128()?,
            source: cursor.leb128()?,
            cover: cursor.ids()?,
            lift_order: cursor.ids()?,
            coefficients: cursor.terms()?,
        }),
        48 => Ok(Record::Branch {
            parent: cursor.leb128()?,
            variable: cursor.leb128()?,
            split: cursor.rational()?,
            left: cursor.leb128()?,
            right: cursor.leb128()?,
        }),
        64 => Ok(Record::LeafBound {
            node: cursor.leb128()?,
            bound: cursor.leb128()?,
        }),
        65 => Ok(Record::LeafInfeas {
            node: cursor.leb128()?,
            contradiction: cursor.leb128()?,
        }),
        66 => Ok(Record::LeafIntegral {
            node: cursor.leb128()?,
            point: cursor.rationals()?,
            bound: cursor.leb128()?,
        }),
        67 => Ok(Record::LeafFrontier {
            node: cursor.leb128()?,
            bound: cursor.leb128()?,
        }),
        80 => Ok(Record::Incumbent {
            reduced: cursor.rationals()?,
            original: cursor.rationals()?,
            objective: cursor.rational()?,
        }),
        81 => Ok(Record::FeasiblePoint {
            point: cursor.rationals()?,
            objective: cursor.rational()?,
        }),
        82 => Ok(Record::Ray {
            base: cursor.rationals()?,
            direction: cursor.rationals()?,
            step: cursor.optional()?,
        }),
        96 => {
            let kind = cursor.byte()?;
            let Some(kind) = ClaimKind::from_code(kind) else {
                return Err(Refusal::new(
                    Rejection::HCC_UNKNOWN_RECORD,
                    format!("claim kind {kind} is not one of the five"),
                ));
            };
            Ok(Record::Claim {
                kind,
                objective: cursor.optional()?,
                lower: cursor.optional()?,
                gap: cursor.optional()?,
            })
        }
        127 => {
            let count = cursor.leb128()?;
            if count > cursor.remaining() as u64 {
                return Err(Refusal::new(
                    Rejection::HCC_TRUNCATED,
                    format!(
                        "{count} counts declared and {} bytes remain",
                        cursor.remaining()
                    ),
                ));
            }
            let mut counts: Vec<(u8, u64)> = Vec::with_capacity(count as usize);
            for _ in 0..count {
                let code = cursor.byte()?;
                if let Some((previous, _)) = counts.last() {
                    if code <= *previous {
                        return Err(Refusal::new(
                            Rejection::HCC_SET_ORDER,
                            format!("record kind {code} does not follow {previous}"),
                        ));
                    }
                }
                counts.push((code, cursor.leb128()?));
            }
            Ok(Record::End { counts })
        }
        other => Err(Refusal::new(
            Rejection::HCC_UNKNOWN_RECORD,
            format!("record type {other} is not one of the closed list"),
        )),
    }
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
    fn a_certificate_survives_the_round_trip_exactly() {
        let original = sample();
        let bytes = original.encode();
        let recovered = decode(&bytes).expect("a well-formed certificate");
        assert_eq!(recovered, original);
        assert_eq!(recovered.encode(), bytes);
    }

    #[test]
    fn every_record_kind_survives_the_round_trip() {
        let records = vec![
            Record::Transform {
                rule: 3,
                parameters: vec![ratio(1, 7)],
                result: [1; 32],
            },
            Record::Root {
                node: 0,
                model: [2; 32],
            },
            Record::DualBound {
                node: 0,
                terms: vec![(1, number(2))],
            },
            Record::Contradiction {
                node: 0,
                terms: vec![(0, ratio(1, 2)), (3, number(1))],
            },
            Record::GmiCut {
                scope: 0,
                combination: vec![(0, number(1))],
                isolated: 2,
                coefficients: vec![(0, ratio(1, 3)), (1, number(-1))],
                right: ratio(2, 5),
            },
            Record::CoverCut {
                scope: 0,
                source: 1,
                cover: vec![0, 2, 5],
                lift_order: vec![1, 3],
                coefficients: vec![(0, number(1))],
            },
            Record::Branch {
                parent: 0,
                variable: 4,
                split: number(3),
                left: 1,
                right: 2,
            },
            Record::LeafBound { node: 1, bound: 2 },
            Record::LeafInfeas {
                node: 2,
                contradiction: 3,
            },
            Record::LeafIntegral {
                node: 3,
                point: vec![number(0), number(1)],
                bound: 4,
            },
            Record::LeafFrontier { node: 4, bound: 5 },
            Record::Incumbent {
                reduced: vec![number(1)],
                original: vec![number(1)],
                objective: number(5),
            },
            Record::FeasiblePoint {
                point: vec![ratio(-3, 8)],
                objective: ratio(1, 8),
            },
            Record::Ray {
                base: vec![number(0)],
                direction: vec![number(1)],
                step: Some(number(2)),
            },
            Record::Claim {
                kind: ClaimKind::FeasibleWithGap,
                objective: Some(number(9)),
                lower: Some(number(4)),
                gap: Some(ratio(5, 9)),
            },
        ];
        let records = Certificate::terminated(records);
        let certificate = Certificate {
            format_tag: 2,
            instance_bytes: 1,
            digest: [3; 32],
            claim: ClaimKind::FeasibleWithGap,
            records,
        };
        let bytes = certificate.encode();
        assert_eq!(decode(&bytes).expect("well formed"), certificate);
    }

    #[test]
    fn a_damaged_byte_is_diagnosed_as_damage_rather_than_as_bad_mathematics() {
        let bytes = sample().encode();
        for position in [70usize, 90, 110] {
            if position >= bytes.len() - 4 {
                continue;
            }
            let mut damaged = bytes.clone();
            damaged[position] ^= 0xff;
            let refusal = decode(&damaged).expect_err("the checksum notices");
            assert_eq!(
                refusal.id,
                Rejection::HCC_CHECKSUM,
                "at {position}: {refusal}"
            );
        }
    }

    #[test]
    fn the_header_is_checked_before_anything_else() {
        let bytes = sample().encode();

        let mut wrong_magic = bytes.clone();
        wrong_magic[0] = b'X';
        assert_eq!(decode(&wrong_magic).unwrap_err().id, Rejection::HCC_MAGIC);

        let mut wrong_version = bytes.clone();
        wrong_version[4] = 2;
        assert_eq!(
            decode(&wrong_version).unwrap_err().id,
            Rejection::HCC_VERSION
        );

        assert_eq!(
            decode(&bytes[..10]).unwrap_err().id,
            Rejection::HCC_TRUNCATED
        );
        assert_eq!(decode(b"HCC").unwrap_err().id, Rejection::HCC_MAGIC);
    }

    #[test]
    fn a_truncated_stream_and_a_padded_one_are_named_differently() {
        let bytes = sample().encode();
        let short = &bytes[..bytes.len() - 8];
        assert_eq!(decode(short).unwrap_err().id, Rejection::HCC_TRUNCATED);

        let mut padded = bytes.clone();
        padded.extend_from_slice(&[0, 0, 0, 0]);
        assert_eq!(
            decode(&padded).unwrap_err().id,
            Rejection::HCC_TRAILING_BYTES
        );
    }

    #[test]
    fn a_padded_variable_length_integer_is_refused() {
        // The same number, spelled with one byte too many. Accepting it would
        // give one proof two byte streams.
        let mut cursor = Cursor {
            bytes: &[0x81, 0x00],
            at: 0,
        };
        assert_eq!(
            cursor.leb128().unwrap_err().id,
            Rejection::HCC_NON_MINIMAL_LENGTH
        );

        let mut minimal = Cursor {
            bytes: &[0x01],
            at: 0,
        };
        assert_eq!(minimal.leb128().expect("minimal"), 1);

        let mut zero = Cursor {
            bytes: &[0x00],
            at: 0,
        };
        assert_eq!(zero.leb128().expect("minimal"), 0);

        let mut huge = Cursor {
            bytes: &[0xff; 11],
            at: 0,
        };
        assert_eq!(
            huge.leb128().unwrap_err().id,
            Rejection::HCC_LENGTH_OVERFLOW
        );
    }

    #[test]
    fn a_rational_that_is_not_canonical_is_refused() {
        for text in ["6/4", "3/1", "0/5", "1.5", "+3", "-0"] {
            let mut payload = vec![text.len() as u8];
            payload.extend_from_slice(text.as_bytes());
            let mut cursor = Cursor {
                bytes: &payload,
                at: 0,
            };
            assert_eq!(
                cursor.rational().unwrap_err().id,
                Rejection::HCC_RATIONAL_FORM,
                "for {text:?}"
            );
        }
    }

    #[test]
    fn an_unsorted_or_repeated_set_element_is_refused() {
        // Two identifiers out of order inside a multiplier list.
        let mut payload = vec![2u8];
        payload.push(5);
        payload.extend_from_slice(&[1, b'1']);
        payload.push(3);
        payload.extend_from_slice(&[1, b'2']);
        let mut cursor = Cursor {
            bytes: &payload,
            at: 0,
        };
        assert_eq!(cursor.terms().unwrap_err().id, Rejection::HCC_SET_ORDER);

        let mut repeated = vec![2u8];
        repeated.push(3);
        repeated.extend_from_slice(&[1, b'1']);
        repeated.push(3);
        repeated.extend_from_slice(&[1, b'2']);
        let mut cursor = Cursor {
            bytes: &repeated,
            at: 0,
        };
        assert_eq!(cursor.terms().unwrap_err().id, Rejection::HCC_SET_ORDER);
    }

    #[test]
    fn an_unknown_record_type_is_refused_rather_than_skipped() {
        let mut cursor = Cursor { bytes: &[], at: 0 };
        assert_eq!(
            read_record(200, &mut cursor).unwrap_err().id,
            Rejection::HCC_UNKNOWN_RECORD
        );
        // Proof data has no forward-compatible ignore rule.
        assert_eq!(
            read_record(4, &mut cursor).unwrap_err().id,
            Rejection::HCC_UNKNOWN_RECORD
        );
    }

    #[test]
    fn a_certificate_is_bound_to_one_instance() {
        let instance = b"ROWS\n N obj\nCOLUMNS\nENDATA\n";
        let mut certificate = sample();
        certificate.digest = hc_core::instance_digest(1, instance);
        certificate.instance_bytes = instance.len() as u64;
        assert!(bind(&certificate, instance).is_ok());

        let other = b"ROWS\n N cost\nCOLUMNS\nENDATA\n";
        assert_eq!(
            bind(&certificate, other).unwrap_err().id,
            Rejection::HCC_NOT_THIS_INSTANCE
        );
    }

    #[test]
    fn a_terminator_that_miscounts_is_refused() {
        let mut certificate = sample();
        let last = certificate.records.len() - 1;
        certificate.records[last] = Record::End {
            counts: vec![(3, 99)],
        };
        let bytes = certificate.encode();
        assert_eq!(decode(&bytes).unwrap_err().id, Rejection::HCC_END_COUNTS);
    }
}
