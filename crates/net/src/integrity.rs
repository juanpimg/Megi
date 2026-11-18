//! Subresource Integrity (SRI, #1527): parse an `integrity` attribute and verify SHA-256,
//! SHA-384, or SHA-512 digests over the decoded bytes of a subresource before it is used.
//!
//! Self-contained, dependency-free FIPS 180-4 SHA-256, SHA-384, and SHA-512 implementations
//! plus the `<alg>-<base64>` syntax from the W3C SRI specification. When multiple valid
//! hash expressions are present, the strongest algorithm tier (`sha512` > `sha384` >
//! `sha256`) is enforced per SRI §3.3.3.
//!
//! The API is pure (bytes in, `Result` out) so callers can enforce it at any load boundary.

mod sha;

pub use sha::{
    matches_sha256_base64, matches_sha384_base64, matches_sha512_base64, sha1, sha256, sha384,
    sha512,
};

use sha::base64_decode;

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
