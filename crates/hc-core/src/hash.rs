//! Hashes, written out rather than depended on.
//!
//! Two well-specified functions, a few hundred lines between them, both with
//! published vectors. Taking a dependency for them would put someone else's
//! code on the path that decides whether a certificate is bound to the file it
//! claims to be about — and would put the same code on both sides of a
//! boundary whose entire purpose is that the two sides share nothing. The
//! verifier carries its own implementations of both; these are the solver's.
//!
//! The instance digest is the reason either of these exists. A certificate does
//! not prove something about a model the solver happened to build in memory; it
//! proves something about a file, named by a hash over that file's exact bytes
//! with a format tag in front. No newline normalization, no trailing-whitespace
//! strip, no byte-order-mark removal, no case folding. The bytes are part of
//! the theorem.

/// The eight words a SHA-256 state starts from.
const INITIAL: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];

/// The sixty-four round constants.
const ROUND: [u32; 64] = [
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

/// A SHA-256 computation in progress.
///
/// Incremental because the digest covers a format tag followed by a file, and
/// building a copy of the file with a byte in front of it in order to hash it
/// is an allocation the size of the input for no reason.
#[derive(Clone, Debug)]
pub struct Sha256 {
    words: [u32; 8],
    block: [u8; 64],
    filled: usize,
    length: u64,
}

impl Sha256 {
    /// A fresh state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            words: INITIAL,
            block: [0; 64],
            filled: 0,
            length: 0,
        }
    }

    /// Adds bytes to the message.
    pub fn update(&mut self, bytes: &[u8]) {
        self.length = self.length.wrapping_add(bytes.len() as u64);
        let mut rest = bytes;
        while !rest.is_empty() {
            let room = 64 - self.filled;
            let take = room.min(rest.len());
            self.block[self.filled..self.filled + take].copy_from_slice(&rest[..take]);
            self.filled += take;
            rest = &rest[take..];
            if self.filled == 64 {
                let block = self.block;
                self.compress(&block);
                self.filled = 0;
            }
        }
    }

    /// Completes the message and returns the digest.
    #[must_use]
    pub fn finish(mut self) -> [u8; 32] {
        let bits = self.length.wrapping_mul(8);
        self.update(&[0x80]);
        // The length field occupies the final eight bytes of a block, so pad
        // to fifty-six and spill into another block when there is no room.
        while self.filled != 56 {
            self.update(&[0]);
        }
        // `update` has been counting these padding bytes into the length; the
        // value written below was captured before any of them arrived.
        let block = {
            let mut block = self.block;
            block[56..].copy_from_slice(&bits.to_be_bytes());
            block
        };
        self.compress(&block);

        let mut digest = [0u8; 32];
        for (index, word) in self.words.iter().enumerate() {
            digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        digest
    }

    /// One sixty-four byte block.
    fn compress(&mut self, block: &[u8; 64]) {
        let mut schedule = [0u32; 64];
        for (index, slot) in schedule.iter_mut().take(16).enumerate() {
            let at = index * 4;
            *slot = u32::from_be_bytes([block[at], block[at + 1], block[at + 2], block[at + 3]]);
        }
        for index in 16..64 {
            let previous = schedule[index - 15];
            let recent = schedule[index - 2];
            let small = previous.rotate_right(7) ^ previous.rotate_right(18) ^ (previous >> 3);
            let large = recent.rotate_right(17) ^ recent.rotate_right(19) ^ (recent >> 10);
            schedule[index] = schedule[index - 16]
                .wrapping_add(small)
                .wrapping_add(schedule[index - 7])
                .wrapping_add(large);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.words;
        for index in 0..64 {
            let sigma1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choose = (e & f) ^ ((!e) & g);
            let first = h
                .wrapping_add(sigma1)
                .wrapping_add(choose)
                .wrapping_add(ROUND[index])
                .wrapping_add(schedule[index]);
            let sigma0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let second = sigma0.wrapping_add(majority);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(first);
            d = c;
            c = b;
            b = a;
            a = first.wrapping_add(second);
        }

        for (word, added) in self.words.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *word = word.wrapping_add(added);
        }
    }
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

/// The SHA-256 digest of a byte string.
#[must_use]
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut state = Sha256::new();
    state.update(bytes);
    state.finish()
}

/// The instance digest: the format tag byte, then the file's exact bytes.
///
/// The tag is prepended so that one byte sequence read under two different
/// format rules cannot produce the same digest. Fixed and free MPS get
/// different tags precisely because a file can be legal under both and mean
/// different things.
#[must_use]
pub fn instance_digest(format_tag: u8, raw: &[u8]) -> [u8; 32] {
    let mut state = Sha256::new();
    state.update(&[format_tag]);
    state.update(raw);
    state.finish()
}

/// Lowercase hexadecimal, the form every digest is written in.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[usize::from(byte >> 4)] as char);
        out.push(DIGITS[usize::from(byte & 0x0f)] as char);
    }
    out
}

/// The reflected CRC-32C polynomial.
const CRC_POLYNOMIAL: u32 = 0x82f6_3b78;

/// The byte table, built at compile time so there is no initialization to get
/// wrong and nothing mutable at rest.
const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut index = 0usize;
    while index < 256 {
        let mut value = index as u32;
        let mut bit = 0;
        while bit < 8 {
            value = if value & 1 == 1 {
                (value >> 1) ^ CRC_POLYNOMIAL
            } else {
                value >> 1
            };
            bit += 1;
        }
        table[index] = value;
        index += 1;
    }
    table
};

/// A CRC-32C computation in progress.
#[derive(Clone, Copy, Debug)]
pub struct Crc32c {
    value: u32,
}

impl Crc32c {
    /// A fresh state.
    #[must_use]
    pub const fn new() -> Self {
        Self { value: u32::MAX }
    }

    /// Adds bytes.
    pub fn update(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            let index = usize::from((self.value ^ u32::from(byte)) as u8);
            self.value = (self.value >> 8) ^ CRC_TABLE[index];
        }
    }

    /// The checksum.
    #[must_use]
    pub fn finish(self) -> u32 {
        self.value ^ u32::MAX
    }
}

impl Default for Crc32c {
    fn default() -> Self {
        Self::new()
    }
}

/// The CRC-32C checksum of a byte string.
#[must_use]
pub fn crc32c(bytes: &[u8]) -> u32 {
    let mut state = Crc32c::new();
    state.update(bytes);
    state.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_published_digests_are_reproduced() {
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(&sha256(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn the_long_message_vector_exercises_the_length_counter() {
        let message = vec![b'a'; 1_000_000];
        assert_eq!(
            hex(&sha256(&message)),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn the_incremental_and_one_shot_forms_agree_at_every_split() {
        let message: Vec<u8> = (0..=255u8).cycle().take(1000).collect();
        let expected = sha256(&message);
        for split in [0usize, 1, 55, 56, 57, 63, 64, 65, 127, 128, 500, 999, 1000] {
            let mut state = Sha256::new();
            state.update(&message[..split]);
            state.update(&message[split..]);
            assert_eq!(state.finish(), expected, "split at {split}");
        }
    }

    #[test]
    fn the_length_field_spills_into_a_second_block_when_it_must() {
        // Fifty-five bytes leaves exactly room for the terminator and the
        // length; fifty-six does not, and the padding has to open a block.
        for length in [55usize, 56, 63, 64] {
            let message = vec![b'x'; length];
            let mut state = Sha256::new();
            for byte in &message {
                state.update(&[*byte]);
            }
            assert_eq!(state.finish(), sha256(&message), "length {length}");
        }
    }

    #[test]
    fn the_checksum_reproduces_its_published_values() {
        assert_eq!(crc32c(b""), 0x0000_0000);
        assert_eq!(crc32c(b"123456789"), 0xe306_9283);
        assert_eq!(crc32c(b"a"), 0xc1d0_4330);
        assert_eq!(crc32c(b"HCC1"), 0x31b6_9343);
        assert_eq!(crc32c(b"hard-cases"), 0xd48c_c147);
    }

    #[test]
    fn the_checksum_is_incremental_too() {
        let message = b"the quick brown fox jumps over the lazy dog";
        for split in 0..message.len() {
            let mut state = Crc32c::new();
            state.update(&message[..split]);
            state.update(&message[split..]);
            assert_eq!(state.finish(), crc32c(message), "split at {split}");
        }
    }

    #[test]
    fn the_instance_digest_puts_the_tag_in_front() {
        assert_eq!(instance_digest(1, b"abc"), sha256(b"\x01abc"));
        // The same bytes under two format rules are two different instances,
        // which is the entire reason the tag is there.
        assert_ne!(instance_digest(1, b"abc"), instance_digest(2, b"abc"));
        // And a tag is not a prefix that can be forged from content.
        assert_ne!(instance_digest(1, b"abc"), sha256(b"1abc"));
    }

    #[test]
    fn hexadecimal_is_lowercase_and_padded() {
        assert_eq!(hex(&[0x00, 0x0f, 0xff]), "000fff");
        assert_eq!(hex(&[]), "");
        assert_eq!(hex(&sha256(b"")).len(), 64);
    }
}
