//! The byte-level shape every accepted input has to have.
//!
//! The restriction is derived rather than chosen. The canonical dump is ASCII
//! and writes names into it verbatim, so a name that cannot appear in the dump
//! cannot appear in a model. And a file with carriage returns is a different
//! byte sequence, hence a different instance under the digest; accepting both
//! and normalizing would make two digests produce one model.
//!
//! It lives here rather than inside one format because all three accepted
//! formats have it, and a fence duplicated per format is a fence three
//! implementations can drift apart on.

use hc_core::{Refusal, Rejection};

/// Refuses input that is not ASCII text with line-feed endings.
///
/// # Errors
///
/// Returns a refusal naming the shape check.
pub(crate) fn admit_text(raw: &[u8]) -> Result<&str, Refusal> {
    for (offset, &byte) in raw.iter().enumerate() {
        let acceptable = byte == b'\n' || byte == b'\t' || (b' '..=b'~').contains(&byte);
        if !acceptable {
            return Err(Refusal::new(
                Rejection::INSTANCE_NOT_TEXT,
                format!("byte {byte:#04x} at offset {offset} is not accepted input"),
            ));
        }
    }
    // Every byte is ASCII, so this cannot fail; the conversion is written as a
    // fallible one anyway rather than through an unchecked call.
    core::str::from_utf8(raw).map_err(|error| {
        Refusal::new(
            Rejection::INSTANCE_NOT_TEXT,
            format!("input is not text: {error}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_text_passes_and_keeps_its_bytes() {
        assert_eq!(
            admit_text(b"NAME\n A\tB\n").expect("ascii"),
            "NAME\n A\tB\n"
        );
    }

    #[test]
    fn a_carriage_return_is_refused_rather_than_normalized() {
        let refusal = admit_text(b"ROWS\r\n").expect_err("not accepted");
        assert_eq!(refusal.id, Rejection::INSTANCE_NOT_TEXT);
    }

    #[test]
    fn a_byte_outside_ascii_is_refused_at_its_offset() {
        let refusal = admit_text(b"ab\xc3\xa9").expect_err("not accepted");
        assert_eq!(refusal.id, Rejection::INSTANCE_NOT_TEXT);
        assert!(refusal.detail.contains("offset 2"), "{refusal}");
    }
}
