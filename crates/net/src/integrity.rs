//! Subresource Integrity (SRI, #1527): parse an `integrity` attribute and verify a SHA-256
//! digest over the decoded bytes of a subresource before it is used.
//!
//! This is a self-contained, dependency-free SHA-256 implementation (FIPS 180-4) plus the
//! `sha256-<base64>` syntax from the SRI spec. Stronger algorithms in an integrity string
//! (`sha384`/`sha512`) are accepted as a metadata signal but fail closed here: only SHA-256 is
//! implemented, and an unverifiable resource is always rejected.
//!
//! The API is pure (bytes in, `Result` out) so callers can enforce it at any load boundary.

/// Errors from SRI parsing/verification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IntegrityError {
    /// The integrity string is malformed or names only unsupported algorithms.
    Malformed,
    /// The computed digest did not match any accepted hash.
    Mismatch,
}

impl core::fmt::Display for IntegrityError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Malformed => f.write_str("malformed integrity metadata"),
            Self::Mismatch => f.write_str("subresource integrity mismatch"),
        }
    }
}

impl std::error::Error for IntegrityError {}

/// A single parsed SRI hash: algorithm name and expected SHA-256 digest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntegrityHash {
    /// Algorithm token as authored (`sha256`).
    pub algorithm: String,
    /// Decoded expected digest (32 bytes for sha256).
    pub digest: Vec<u8>,
}

/// Parses an `integrity` attribute into SHA-256 hashes.
///
/// SRI allows a whitespace-separated list of `algorithm-base64 digest` tokens; any token using
/// an unsupported algorithm is ignored, and empty/malformed input yields no hashes.
pub fn parse_integrity(value: &str) -> Vec<IntegrityHash> {
    let mut out = Vec::new();
    for token in value.split_ascii_whitespace() {
        let Some((algorithm, encoded)) = token.split_once('-') else {
            continue;
        };
        if !algorithm.eq_ignore_ascii_case("sha256") {
            continue;
        }
        if let Some(digest) = base64_decode(encoded)
            && digest.len() == 32
        {
            out.push(IntegrityHash {
                algorithm: "sha256".to_owned(),
                digest,
            });
        }
    }
    out
}

/// Verifies `bytes` against an `integrity` attribute value.
///
/// - An empty/absent integrity string is a no-op (`Ok(())`): SRI is opt-in.
/// - A malformed value with no usable hashes is [`IntegrityError::Malformed`].
/// - A well-formed value whose hashes don't match is [`IntegrityError::Mismatch`].
pub fn verify_integrity(bytes: &[u8], integrity: &str) -> Result<(), IntegrityError> {
    if integrity.trim().is_empty() {
        return Ok(());
    }
    let hashes = parse_integrity(integrity);
    if hashes.is_empty() {
        return Err(IntegrityError::Malformed);
    }
    let actual = sha256(bytes);
    if hashes.iter().any(|hash| hash.digest == actual) {
        Ok(())
    } else {
        Err(IntegrityError::Mismatch)
    }
}

/// Whether the SHA-256 of `data` equals the base64-encoded digest `expected` (used by CSP
/// `'sha256-…'` source expressions). Malformed base64 never matches.
pub fn matches_sha256_base64(data: &[u8], expected: &str) -> bool {
    match base64_decode(expected) {
        Some(digest) => digest == sha256(data).to_vec(),
        None => false,
    }
}

/// Computes the SHA-256 digest of `data`.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    // Message schedule.
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut padded = data.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for (i, word) in w.iter_mut().enumerate().take(16) {
            *word = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for (i, &word) in w.iter().enumerate() {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(word);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// Minimal standard-base64 decoder (no padding required); returns `None` on invalid input.
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    fn value(byte: u8) -> Option<u8> {
        match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'a'..=b'z' => Some(byte - b'a' + 26),
            b'0'..=b'9' => Some(byte - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            b'=' => None,
            _ => None,
        }
    }
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return Some(Vec::new());
    }
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut accumulator: u32 = 0;
    let mut bits = 0u32;
    for &byte in bytes {
        if byte == b'=' {
            break;
        }
        let v = value(byte)? as u32;
        accumulator = (accumulator << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((accumulator >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_nist_vectors() {
        assert_eq!(
            sha256(b""),
            [
                0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f,
                0xb9, 0x24, 0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c, 0xa4, 0x95, 0x99, 0x1b,
                0x78, 0x52, 0xb8, 0x55
            ]
        );
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        // Multi-block input exercises message-schedule boundaries (NIST million-a vector).
        assert_eq!(
            hex(&sha256(&[b'a'; 1_000_000])),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn integrity_accepts_matching_sha256() {
        // base64 of sha256("hello") = LPJNul+wow4m6DsqxbninhsWHlwfp0JecwQzYpOLmCQ=
        let integrity = "sha256-LPJNul+wow4m6DsqxbninhsWHlwfp0JecwQzYpOLmCQ=";
        assert!(verify_integrity(b"hello", integrity).is_ok());
        assert_eq!(
            verify_integrity(b"tampered", integrity),
            Err(IntegrityError::Mismatch)
        );
    }

    #[test]
    fn integrity_multiple_hashes_and_absent() {
        assert_eq!(verify_integrity(b"x", ""), Ok(()));
        assert_eq!(
            verify_integrity(b"x", "sha512-abc"),
            Err(IntegrityError::Malformed)
        );
        let matching = "sha256-LPJNul+wow4m6DsqxbninhsWHlwfp0JecwQzYpOLmCQ=";
        let integrity = format!("sha384-AAAA {matching}");
        assert!(verify_integrity(b"hello", &integrity).is_ok());
    }
}
