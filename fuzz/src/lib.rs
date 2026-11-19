//! Shared helpers for the Megi fuzz targets.
//!
//! `cargo-fuzz` turns every file in `fuzz/fuzz_targets/` into its own binary, so code that
//! several targets need lives here instead of being copy-pasted into each harness. Keeping
//! one copy matters for correctness: the RIFF container layout below is shared by the
//! `webp` target's lossy and lossless paths, and a duplicated version had already drifted
//! (see issues #859 and #471).

/// Wraps `payload` in a minimal `RIFF....WEBP` container whose single chunk id is `chunk`.
///
/// The RIFF size field, the chunk size field and the mandatory even-length padding are all
/// written per the WebP container spec, so the container validators see a well-formed
/// envelope around possibly malformed payload bytes — which is exactly the boundary the
/// WebP decoders have to defend.
pub fn riff_webp(chunk: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(20 + payload.len());
    let padded = payload.len() + (payload.len() & 1);
    // "WEBP" (4) + chunk id (4) + chunk size (4) + padded payload.
    let riff_size = 4u32 + 8 + padded as u32;
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_size.to_le_bytes());
    out.extend_from_slice(b"WEBP");
    out.extend_from_slice(chunk);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    if payload.len() & 1 == 1 {
        out.push(0);
    }
    out
}

/// Returns `data` as a `&str` of at most `max_bytes`, never splitting a UTF-8 sequence.
///
/// Slicing a `String` at an arbitrary byte index panics when the index lands inside a
/// multi-byte character; a harness that truncates fuzzer-provided text needs this instead.
/// Input that is not valid UTF-8 is converted lossily, which mirrors what a browser does
/// with a malformed UTF-8 document (U+FFFD substitution).
pub fn lossy_prefix(data: &[u8], max_bytes: usize) -> String {
    let text = String::from_utf8_lossy(data);
    let text = if text.len() <= max_bytes {
        text
    } else {
        let mut end = max_bytes;
        while end > 0 && !text.is_char_boundary(end) {
            end -= 1;
        }
        std::borrow::Cow::Owned(text[..end].to_string())
    };
    text.into_owned()
}

/// Splits `data` into length-prefixed parts: `[u16 LE len][len bytes]…`, up to `max_parts`.
///
/// Frames are explicit so module sources keep embedded NUL bytes and arbitrary binary
/// content; the previous "split on NUL" framing silently truncated real inputs (#701).
/// A frame longer than the remaining bytes ends the loop, so truncated seeds still yield
/// the complete parts that precede them.
pub fn length_prefixed_parts(data: &[u8], max_parts: usize) -> Vec<&[u8]> {
    let mut parts = Vec::with_capacity(max_parts.min(8));
    let mut offset = 0usize;
    while parts.len() < max_parts && offset + 2 <= data.len() {
        let len = u16::from_le_bytes([data[offset], data[offset + 1]]) as usize;
        let start = offset + 2;
        let Some(end) = start.checked_add(len) else {
            break;
        };
        if end > data.len() {
            break;
        }
        parts.push(&data[start..end]);
        offset = end;
    }
    parts
}

/// Reads `width` bytes starting at `offset` as a little-endian `u32` (0 when out of range).
pub fn read_u32(data: &[u8], offset: usize) -> u32 {
    let mut bytes = [0u8; 4];
    for (index, byte) in bytes.iter_mut().enumerate() {
        if let Some(value) = data.get(offset + index) {
            *byte = *value;
        }
    }
    u32::from_le_bytes(bytes)
}

/// Headers derived from length-prefixed fuzz parts (#1762).
pub fn net_surface_headers(data: &[u8]) -> Vec<(String, String)> {
    let parts = length_prefixed_parts(data, 8);
    parts
        .chunks(2)
        .filter(|pair| pair.len() == 2)
        .map(|pair| {
            (
                String::from_utf8_lossy(pair[0]).into_owned(),
                String::from_utf8_lossy(pair[1]).into_owned(),
            )
        })
        .collect()
}

/// Referrer policy derived from a fuzz part (#1762).
pub fn net_surface_referrer_policy(data: &[u8]) -> megi_net::ReferrerPolicy {
    megi_net::ReferrerPolicy::parse(&String::from_utf8_lossy(data)).unwrap_or_default()
}

/// URL derived from a fuzz part, for APIs that need a parsed `Url` (#1762).
pub fn net_surface_data_url(data: &[u8]) -> Option<megi_net::Url> {
    let parts = length_prefixed_parts(data, 8);
    let first = parts.first()?;
    megi_net::Url::parse(&String::from_utf8_lossy(first)).ok()
}

/// Chunk returned by the harness-owned producer (#1762).
///
/// Mode 2 exercises the `chunk == max` boundary by padding a short source to
/// exactly `max`, instead of duplicating mode 0 and leaving the boundary untested.
pub fn producer_chunk(source: &[u8], max: usize, mode: usize) -> Vec<u8> {
    match mode % 3 {
        0 => source[..source.len().min(max)].to_vec(),
        1 => {
            if source.len() > max {
                source[..max].to_vec()
            } else {
                source.to_vec()
            }
        }
        _ => {
            if source.len() >= max {
                source[..max].to_vec()
            } else {
                let mut out = source.to_vec();
                out.resize(max, 0);
                out
            }
        }
    }
}

#[cfg(test)]
mod net_surface_tests {
    use super::*;

    #[test]
    fn headers_are_derived_from_the_fuzz_input() {
        // #1762: fixed empty headers meant parse_cache_policy and
        // resolve_content_type only ever saw one branch.
        let mut data = Vec::new();
        for part in [b"cache-control".as_slice(), b"no-store".as_slice()] {
            data.extend_from_slice(&(part.len() as u16).to_le_bytes());
            data.extend_from_slice(part);
        }
        let headers = net_surface_headers(&data);
        assert_eq!(
            headers,
            vec![("cache-control".to_owned(), "no-store".to_owned())]
        );
    }

    #[test]
    fn referrer_policy_is_derived_from_the_fuzz_input() {
        assert_eq!(
            net_surface_referrer_policy(b"no-referrer"),
            megi_net::ReferrerPolicy::NoReferrer
        );
        assert_eq!(
            net_surface_referrer_policy(b"garbage"),
            megi_net::ReferrerPolicy::default()
        );
    }

    #[test]
    fn producer_chunk_mode_two_returns_a_boundary_chunk() {
        // #1762: mode 2 used to be identical to mode 0, so the boundary path
        // (`chunk == max`) was never exercised.
        assert_eq!(producer_chunk(b"abc", 8, 0), b"abc");
        assert_eq!(producer_chunk(b"abc", 8, 1), b"abc");
        assert_eq!(producer_chunk(b"abc", 8, 2).len(), 8);
        assert_eq!(producer_chunk(b"abcdefghij", 4, 2).len(), 4);
    }
}
