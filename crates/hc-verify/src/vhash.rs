//! The verifier's hashes.
//!
//! The digest decides whether a certificate is about the file in front of it,
//! and the checksum decides whether the certificate's bytes arrived intact.
//! Both are written here rather than shared with the solver, for the same
//! reason nothing else is shared.
//!
//! An honest note about how much that independence buys. For a hash with
//! published vectors, two correct implementations are indistinguishable, and
//! the vectors are what makes either one trustworthy — writing it twice adds
//! far less here than it does for a parser, where the specification has to be
//! interpreted. The duplication is kept anyway, because the alternative is a
//! dependency on the proof path and because a rule with an exception is a rule
//! that acquires more of them.
//!
//! The construction is still deliberately unlike the solver's. The message
//! schedule is a rolling window of sixteen words rather than an array of
//! sixty-four, and the checksum consumes half a byte at a time from a table of
//! sixteen entries rather than a whole byte from a table of two hundred and
//! fifty-six.

/// The initial state words.
const START: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];

/// The round constants.
const CONSTANTS: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

/// A digest in progress.
#[derive(Clone, Debug)]
pub struct Digest {
    state: [u32; 8],
    pending: [u8; 64],
    pending_len: usize,
    total: u64,
}

impl Digest {
    /// A fresh digest.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: START,
            pending: [0; 64],
            pending_len: 0,
            total: 0,
        }
    }

    /// Adds bytes to the message.
    pub fn add(&mut self, bytes: &[u8]) {
        self.total = self.total.wrapping_add(bytes.len() as u64);
        let mut cursor = 0usize;
        while cursor < bytes.len() {
            let space = 64 - self.pending_len;
            let step = space.min(bytes.len() - cursor);
            self.pending[self.pending_len..self.pending_len + step]
                .copy_from_slice(&bytes[cursor..cursor + step]);
            self.pending_len += step;
            cursor += step;
            if self.pending_len == 64 {
                let full = self.pending;
                self.round(&full);
                self.pending_len = 0;
            }
        }
    }

    /// Finishes the message and produces the thirty-two byte digest.
    #[must_use]
    pub fn done(mut self) -> [u8; 32] {
        let bit_length = self.total.wrapping_mul(8);
        self.add(&[0x80]);
        while self.pending_len != 56 {
            self.add(&[0]);
        }
        let mut last = self.pending;
        last[56..64].copy_from_slice(&bit_length.to_be_bytes());
        self.round(&last);

        let mut out = [0u8; 32];
        let mut at = 0usize;
        for word in self.state {
            out[at..at + 4].copy_from_slice(&word.to_be_bytes());
            at += 4;
        }
        out
    }

    /// One block, with the schedule kept in a rolling window.
    fn round(&mut self, block: &[u8; 64]) {
        let mut window = [0u32; 16];
        for (slot, chunk) in window.iter_mut().zip(block.as_chunks::<4>().0.iter()) {
            *slot = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }

        let mut working = self.state;
        for step in 0..64usize {
            if step >= 16 {
                let back_fifteen = window[(step + 1) % 16];
                let back_two = window[(step + 14) % 16];
                let low = back_fifteen.rotate_right(7)
                    ^ back_fifteen.rotate_right(18)
                    ^ (back_fifteen >> 3);
                let high = back_two.rotate_right(17) ^ back_two.rotate_right(19) ^ (back_two >> 10);
                window[step % 16] = window[step % 16]
                    .wrapping_add(low)
                    .wrapping_add(window[(step + 9) % 16])
                    .wrapping_add(high);
            }

            let e = working[4];
            let spread = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let pick = (e & working[5]) ^ ((!e) & working[6]);
            let upper = working[7]
                .wrapping_add(spread)
                .wrapping_add(pick)
                .wrapping_add(CONSTANTS[step])
                .wrapping_add(window[step % 16]);

            let a = working[0];
            let gather = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let vote = (a & working[1]) ^ (a & working[2]) ^ (working[1] & working[2]);
            let lower = gather.wrapping_add(vote);

            working = [
                upper.wrapping_add(lower),
                working[0],
                working[1],
                working[2],
                working[3].wrapping_add(upper),
                working[4],
                working[5],
                working[6],
            ];
        }

        for (slot, added) in self.state.iter_mut().zip(working) {
            *slot = slot.wrapping_add(added);
        }
    }
}

impl Default for Digest {
    fn default() -> Self {
        Self::new()
    }
}

/// The digest of a byte string.
#[must_use]
pub fn digest(bytes: &[u8]) -> [u8; 32] {
    let mut running = Digest::new();
    running.add(bytes);
    running.done()
}

/// The instance digest: the format tag byte, then the file's exact bytes.
#[must_use]
pub fn instance_digest(format_tag: u8, raw: &[u8]) -> [u8; 32] {
    let mut running = Digest::new();
    running.add(&[format_tag]);
    running.add(raw);
    running.done()
}

/// Lowercase hexadecimal.
#[must_use]
pub fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(nibble(byte >> 4));
        out.push(nibble(byte & 0x0f));
    }
    out
}

/// Reads lowercase hexadecimal into bytes, or `None` if it is not that.
#[must_use]
pub fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() / 2);
    let bytes = text.as_bytes();
    let mut at = 0usize;
    while at < bytes.len() {
        let high = value_of(bytes[at])?;
        let low = value_of(bytes[at + 1])?;
        out.push((high << 4) | low);
        at += 2;
    }
    Some(out)
}

fn nibble(value: u8) -> char {
    char::from(if value < 10 {
        b'0' + value
    } else {
        b'a' + value - 10
    })
}

fn value_of(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

/// The reflected checksum polynomial.
const POLYNOMIAL: u32 = 0x82f6_3b78;

/// Half-byte table, sixteen entries rather than two hundred and fifty-six.
const HALF_BYTE: [u32; 16] = {
    let mut table = [0u32; 16];
    let mut index = 0usize;
    while index < 16 {
        let mut value = index as u32;
        let mut step = 0;
        while step < 4 {
            value = if value & 1 == 1 {
                (value >> 1) ^ POLYNOMIAL
            } else {
                value >> 1
            };
            step += 1;
        }
        table[index] = value;
        index += 1;
    }
    table
};

/// A checksum in progress.
#[derive(Clone, Copy, Debug)]
pub struct Checksum {
    running: u32,
}

impl Checksum {
    /// A fresh checksum.
    #[must_use]
    pub const fn new() -> Self {
        Self { running: u32::MAX }
    }

    /// Adds bytes.
    pub fn add(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            let mut value = self.running ^ u32::from(byte);
            value = (value >> 4) ^ HALF_BYTE[(value & 0x0f) as usize];
            value = (value >> 4) ^ HALF_BYTE[(value & 0x0f) as usize];
            self.running = value;
        }
    }

    /// The checksum.
    #[must_use]
    pub fn done(self) -> u32 {
        self.running ^ u32::MAX
    }
}

impl Default for Checksum {
    fn default() -> Self {
        Self::new()
    }
}

/// The checksum of a byte string.
#[must_use]
pub fn checksum(bytes: &[u8]) -> u32 {
    let mut running = Checksum::new();
    running.add(bytes);
    running.done()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_published_digests_are_reproduced() {
        assert_eq!(
            to_hex(&digest(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            to_hex(&digest(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            to_hex(&digest(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(
            to_hex(&digest(b"HCC1")),
            "100a778fd4f638d1fa94e55f66f313e81680b2c8285e460013fa14ac6ff2e183"
        );
    }

    #[test]
    fn the_long_message_exercises_the_length_counter() {
        let message = vec![b'a'; 1_000_000];
        assert_eq!(
            to_hex(&digest(&message)),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn adding_in_pieces_agrees_with_adding_at_once() {
        let message: Vec<u8> = (0..=255u8).cycle().take(1000).collect();
        let expected = digest(&message);
        for split in [0usize, 1, 55, 56, 57, 63, 64, 65, 127, 128, 999, 1000] {
            let mut running = Digest::new();
            running.add(&message[..split]);
            running.add(&message[split..]);
            assert_eq!(running.done(), expected, "split at {split}");
        }
    }

    #[test]
    fn the_checksum_reproduces_its_published_values() {
        assert_eq!(checksum(b""), 0x0000_0000);
        assert_eq!(checksum(b"123456789"), 0xe306_9283);
        assert_eq!(checksum(b"a"), 0xc1d0_4330);
        assert_eq!(checksum(b"HCC1"), 0x31b6_9343);
        assert_eq!(checksum(b"hard-cases"), 0xd48c_c147);
    }

    #[test]
    fn the_checksum_is_incremental() {
        let message = b"the quick brown fox jumps over the lazy dog";
        for split in 0..message.len() {
            let mut running = Checksum::new();
            running.add(&message[..split]);
            running.add(&message[split..]);
            assert_eq!(running.done(), checksum(message), "split at {split}");
        }
    }

    #[test]
    fn the_tag_is_in_front_of_the_instance_digest() {
        assert_eq!(instance_digest(1, b"abc"), digest(b"\x01abc"));
        assert_ne!(instance_digest(1, b"abc"), instance_digest(2, b"abc"));
        assert_ne!(instance_digest(1, b"abc"), digest(b"1abc"));
    }

    #[test]
    fn hexadecimal_round_trips_and_refuses_what_is_not_hexadecimal() {
        assert_eq!(to_hex(&[0x00, 0x0f, 0xff]), "000fff");
        assert_eq!(from_hex("000fff"), Some(vec![0x00, 0x0f, 0xff]));
        assert_eq!(
            from_hex(&to_hex(&digest(b"abc"))),
            Some(digest(b"abc").to_vec())
        );
        assert_eq!(from_hex("0"), None);
        assert_eq!(from_hex("0g"), None);
        assert_eq!(
            from_hex("00FF"),
            None,
            "uppercase is not the canonical spelling"
        );
        assert_eq!(from_hex(""), Some(Vec::new()));
    }
}
