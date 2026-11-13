//! Shared helpers for the Megi fuzz targets.
//!
//! `cargo-fuzz` turns every file in `fuzz/fuzz_targets/` into its own binary, so code that
//! several targets need lives here instead of being copy-pasted into each harness. Keeping
//! one copy matters for correctness: the RIFF container layout below has to match in the
//! `webp_vp8` and `webp_lossy` targets, and a duplicated version had already drifted
//! (see issue #859).

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
