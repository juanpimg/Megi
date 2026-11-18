//! Subresource Integrity (SRI, #1527): parse an `integrity` attribute and verify SHA-256,
//! SHA-384, or SHA-512 digests over the decoded bytes of a subresource before it is used.
//!
//! Self-contained, dependency-free FIPS 180-4 SHA-256, SHA-384, and SHA-512 implementations
//! plus the `<alg>-<base64>` syntax from the W3C SRI specification. When multiple valid
//! hash expressions are present, the strongest algorithm tier (`sha512` > `sha384` >
//! `sha256`) is enforced per SRI §3.3.3.
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

/// A single parsed SRI hash: algorithm name and expected digest bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntegrityHash {
    /// Lowercase algorithm token (`sha256`, `sha384`, or `sha512`).
    pub algorithm: String,
    /// Decoded expected digest (`32`, `48`, or `64` bytes respectively).
    pub digest: Vec<u8>,
}

fn algorithm_strength(algorithm: &str) -> u8 {
    match algorithm {
        "sha512" => 3,
        "sha384" => 2,
        "sha256" => 1,
        _ => 0,
    }
}

/// Parses an `integrity` attribute into supported (`sha256`, `sha384`, `sha512`) hashes.
///
/// SRI allows a whitespace-separated list of `algorithm-base64 digest` tokens; any token using
/// an unsupported algorithm or wrong digest length is ignored, and empty/malformed input yields
/// no hashes.
pub fn parse_integrity(value: &str) -> Vec<IntegrityHash> {
    let mut out = Vec::new();
    for token in value.split_ascii_whitespace() {
        let Some((algorithm, encoded)) = token.split_once('-') else {
            continue;
        };
        let (canonical, expected_len) = if algorithm.eq_ignore_ascii_case("sha256") {
            ("sha256", 32)
        } else if algorithm.eq_ignore_ascii_case("sha384") {
            ("sha384", 48)
        } else if algorithm.eq_ignore_ascii_case("sha512") {
            ("sha512", 64)
        } else {
            continue;
        };
        if let Some(digest) = base64_decode(encoded)
            && digest.len() == expected_len
        {
            out.push(IntegrityHash {
                algorithm: canonical.to_owned(),
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
/// - Only hashes from the strongest supported algorithm tier present (`sha512` > `sha384` >
///   `sha256`) are checked (W3C SRI §3.3.3).
/// - A well-formed value whose strongest hashes don't match is [`IntegrityError::Mismatch`].
pub fn verify_integrity(bytes: &[u8], integrity: &str) -> Result<(), IntegrityError> {
    if integrity.trim().is_empty() {
        return Ok(());
    }
    let hashes = parse_integrity(integrity);
    let max_strength = hashes
        .iter()
        .map(|h| algorithm_strength(&h.algorithm))
        .max()
        .unwrap_or(0);
    if max_strength == 0 {
        return Err(IntegrityError::Malformed);
    }
    let strongest: Vec<&IntegrityHash> = hashes
        .iter()
        .filter(|h| algorithm_strength(&h.algorithm) == max_strength)
        .collect();
    let matched = match max_strength {
        3 => {
            let actual = sha512(bytes);
            strongest.iter().any(|h| h.digest.as_slice() == actual)
        }
        2 => {
            let actual = sha384(bytes);
            strongest.iter().any(|h| h.digest.as_slice() == actual)
        }
        _ => {
            let actual = sha256(bytes);
            strongest.iter().any(|h| h.digest.as_slice() == actual)
        }
    };
    if matched {
        Ok(())
    } else {
        Err(IntegrityError::Mismatch)
    }
}

/// Computes the SHA-1 digest of `data` (needed by the RFC 6455 WebSocket handshake accept
/// value; FIPS 180-4). Self-contained like `sha256`.
pub fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [
        0x6745_2301,
        0xEFCD_AB89,
        0x98BA_DCFE,
        0x1032_5476,
        0xC3D2_E1F0,
    ];
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut padded = data.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in padded.as_chunks::<64>().0 {
        let mut w = [0u32; 80];
        for (i, word) in w.iter_mut().enumerate().take(16) {
            *word = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, &word) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    let mut out = [0u8; 20];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// Whether the SHA-256 of `data` equals the base64-encoded digest `expected` (used by CSP
/// `'sha256-…'` source expressions). Malformed base64 never matches.
pub fn matches_sha256_base64(data: &[u8], expected: &str) -> bool {
    match base64_decode(expected) {
        Some(digest) => digest.as_slice() == sha256(data),
        None => false,
    }
}

/// Whether the SHA-384 of `data` equals the base64-encoded digest `expected` (used by CSP
/// `'sha384-…'` source expressions). Malformed base64 never matches.
pub fn matches_sha384_base64(data: &[u8], expected: &str) -> bool {
    match base64_decode(expected) {
        Some(digest) => digest.as_slice() == sha384(data),
        None => false,
    }
}

/// Whether the SHA-512 of `data` equals the base64-encoded digest `expected` (used by CSP
/// `'sha512-…'` source expressions). Malformed base64 never matches.
pub fn matches_sha512_base64(data: &[u8], expected: &str) -> bool {
    match base64_decode(expected) {
        Some(digest) => digest.as_slice() == sha512(data),
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

/// Shared FIPS 180-4 64-bit compression for SHA-384 and SHA-512.
fn sha512_core(data: &[u8], mut h: [u64; 8]) -> [u64; 8] {
    const K: [u64; 80] = [
        0x428a2f98d728ae22,
        0x7137449123ef65cd,
        0xb5c0fbcfec4d3b2f,
        0xe9b5dba58189dbbc,
        0x3956c25bf348b538,
        0x59f111f1b605d019,
        0x923f82a4af194f9b,
        0xab1c5ed5da6d8118,
        0xd807aa98a3030242,
        0x12835b0145706fbe,
        0x243185be4ee4b28c,
        0x550c7dc3d5ffb4e2,
        0x72be5d74f27b896f,
        0x80deb1fe3b1696b1,
        0x9bdc06a725c71235,
        0xc19bf174cf692694,
        0xe49b69c19ef14ad2,
        0xefbe4786384f25e3,
        0x0fc19dc68b8cd5b5,
        0x240ca1cc77ac9c65,
        0x2de92c6f592b0275,
        0x4a7484aa6ea6e483,
        0x5cb0a9dcbd41fbd4,
        0x76f988da831153b5,
        0x983e5152ee66dfab,
        0xa831c66d2db43210,
        0xb00327c898fb213f,
        0xbf597fc7beef0ee4,
        0xc6e00bf33da88fc2,
        0xd5a79147930aa725,
        0x06ca6351e003826f,
        0x142929670a0e6e70,
        0x27b70a8546d22ffc,
        0x2e1b21385c26c926,
        0x4d2c6dfc5ac42aed,
        0x53380d139d95b3df,
        0x650a73548baf63de,
        0x766a0abb3c77b2a8,
        0x81c2c92e47edaee6,
        0x92722c851482353b,
        0xa2bfe8a14cf10364,
        0xa81a664bbc423001,
        0xc24b8b70d0f89791,
        0xc76c51a30654be30,
        0xd192e819d6ef5218,
        0xd69906245565a910,
        0xf40e35855771202a,
        0x106aa07032bbd1b8,
        0x19a4c116b8d2d0c8,
        0x1e376c085141ab53,
        0x2748774cdf8eeb99,
        0x34b0bcb5e19b48a8,
        0x391c0cb3c5c95a63,
        0x4ed8aa4ae3418acb,
        0x5b9cca4f7763e373,
        0x682e6ff3d6b2b8a3,
        0x748f82ee5defb2fc,
        0x78a5636f43172f60,
        0x84c87814a1f0ab72,
        0x8cc702081a6439ec,
        0x90befffa23631e28,
        0xa4506cebde82bde9,
        0xbef9a3f7b2c67915,
        0xc67178f2e372532b,
        0xca273eceea26619c,
        0xd186b8c721c0c207,
        0xeada7dd6cde0eb1e,
        0xf57d4f7fee6ed178,
        0x06f067aa72176fba,
        0x0a637dc5a2c898a6,
        0x113f9804bef90dae,
        0x1b710b35131c471b,
        0x28db77f523047d84,
        0x32caab7b40c72493,
        0x3c9ebe0a15c9bebc,
        0x431d67c49c100d4c,
        0x4cc5d4becb3e42b6,
        0x597f299cfc657e2a,
        0x5fcb6fab3ad6faec,
        0x6c44198c4a475817,
    ];
    let bit_len = (data.len() as u128).wrapping_mul(8);
    let mut padded = data.to_vec();
    padded.push(0x80);
    while padded.len() % 128 != 112 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.as_chunks::<128>().0 {
        let mut w = [0u64; 80];
        for (i, word) in w.iter_mut().enumerate().take(16) {
            *word = u64::from_be_bytes([
                chunk[i * 8],
                chunk[i * 8 + 1],
                chunk[i * 8 + 2],
                chunk[i * 8 + 3],
                chunk[i * 8 + 4],
                chunk[i * 8 + 5],
                chunk[i * 8 + 6],
                chunk[i * 8 + 7],
            ]);
        }
        for i in 16..80 {
            let s0 = w[i - 15].rotate_right(1) ^ w[i - 15].rotate_right(8) ^ (w[i - 15] >> 7);
            let s1 = w[i - 2].rotate_right(19) ^ w[i - 2].rotate_right(61) ^ (w[i - 2] >> 6);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for (i, &word) in w.iter().enumerate() {
            let s1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(word);
            let s0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
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
    h
}

/// Computes the SHA-384 digest of `data`.
pub fn sha384(data: &[u8]) -> [u8; 48] {
    let h = sha512_core(
        data,
        [
            0xcbbb9d5dc1059ed8,
            0x629a292a367cd507,
            0x9159015a3070dd17,
            0x152fecd8f70e5939,
            0x67332667ffc00b31,
            0x8eb44a8768581511,
            0xdb0c2e0d64f98fa7,
            0x47b5481dbefa4fa4,
        ],
    );
    let mut out = [0u8; 48];
    for (i, word) in h.iter().take(6).enumerate() {
        out[i * 8..i * 8 + 8].copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// Computes the SHA-512 digest of `data`.
pub fn sha512(data: &[u8]) -> [u8; 64] {
    let h = sha512_core(
        data,
        [
            0x6a09e667f3bcc908,
            0xbb67ae8584caa73b,
            0x3c6ef372fe94f82b,
            0xa54ff53a5f1d36f1,
            0x510e527fade682d1,
            0x9b05688c2b3e6c1f,
            0x1f83d9abfb41bd6b,
            0x5be0cd19137e2179,
        ],
    );
    let mut out = [0u8; 64];
    for (i, word) in h.iter().enumerate() {
        out[i * 8..i * 8 + 8].copy_from_slice(&word.to_be_bytes());
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
    use crate::data::base64_encode;

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

    #[test]
    fn sha384_and_sha512_nist_vectors() {
        assert_eq!(
            hex(&sha384(b"")),
            "38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da274edebfe76f65fbd51ad2f14898b95b"
        );
        assert_eq!(
            hex(&sha384(b"abc")),
            "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7"
        );
        assert_eq!(
            hex(&sha512(b"")),
            "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"
        );
        assert_eq!(
            hex(&sha512(b"abc")),
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
        );
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn integrity_accepts_matching_sha256_sha384_sha512() {
        // base64 of sha256("hello") = LPJNul+wow4m6DsqxbninhsWHlwfp0JecwQzYpOLmCQ=
        let s256 = "sha256-LPJNul+wow4m6DsqxbninhsWHlwfp0JecwQzYpOLmCQ=";
        let s384 = format!("sha384-{}", base64_encode(&sha384(b"hello")));
        let s512 = format!("sha512-{}", base64_encode(&sha512(b"hello")));

        assert!(verify_integrity(b"hello", s256).is_ok());
        assert!(verify_integrity(b"hello", &s384).is_ok());
        assert!(verify_integrity(b"hello", &s512).is_ok());
        assert_eq!(
            verify_integrity(b"tampered", &s384),
            Err(IntegrityError::Mismatch)
        );
        assert_eq!(
            verify_integrity(b"tampered", &s512),
            Err(IntegrityError::Mismatch)
        );
        assert!(matches_sha384_base64(
            b"hello",
            s384.strip_prefix("sha384-").expect("prefix")
        ));
        assert!(matches_sha512_base64(
            b"hello",
            s512.strip_prefix("sha512-").expect("prefix")
        ));
    }

    #[test]
    fn integrity_multiple_hashes_and_strongest_tier() {
        assert_eq!(verify_integrity(b"x", ""), Ok(()));
        assert_eq!(
            verify_integrity(b"x", "sha512-abc"),
            Err(IntegrityError::Malformed)
        );
        let matching_256 = "sha256-LPJNul+wow4m6DsqxbninhsWHlwfp0JecwQzYpOLmCQ=";
        // Invalid digest lengths are ignored during parse_integrity.
        assert!(verify_integrity(b"hello", &format!("sha384-AAAA {matching_256}")).is_ok());
        // A valid stronger algorithm (`sha512`) takes precedence over a matching weaker one.
        let wrong_512 = format!("sha512-{}", base64_encode(&sha512(b"other")));
        assert_eq!(
            verify_integrity(b"hello", &format!("{matching_256} {wrong_512}")),
            Err(IntegrityError::Mismatch)
        );
    }
}
