#[allow(unused_imports)]
use super::*;

pub(crate) fn trailer_state(mode: StreamMode) -> StreamState {
    match mode {
        StreamMode::RawDeflate => StreamState::Done,
        _ => StreamState::Trailer,
    }
}

pub(crate) fn append_output(
    bytes: &[u8],
    output: &mut Vec<u8>,
    window: &mut Vec<u8>,
    checksum: &mut StreamChecksum,
    limits: InflateLimits,
    total_output: &mut usize,
) -> Result<(), InflateError> {
    let new_total =
        total_output
            .checked_add(bytes.len())
            .ok_or(InflateError::OutputLimitExceeded {
                limit: limits.max_output,
            })?;
    if new_total > limits.max_output {
        return Err(InflateError::OutputLimitExceeded {
            limit: limits.max_output,
        });
    }
    output.extend_from_slice(bytes);
    window.extend_from_slice(bytes);
    trim_window(window);
    checksum.update(bytes);
    *total_output = new_total;
    Ok(())
}

pub(crate) fn trim_window(window: &mut Vec<u8>) {
    // Only compact when the window exceeds twice the target size, so the O(n) memmove
    // happens at most once per WINDOW_SIZE bytes instead of once per output byte.
    if window.len() > 2 * WINDOW_SIZE {
        let excess = window.len() - WINDOW_SIZE;
        window.drain(..excess);
    }
}

pub(crate) fn parse_gzip_header(input: &[u8]) -> Result<Option<usize>, InflateError> {
    if input.len() < 10 {
        return Ok(None);
    }
    if input[0] != 0x1F || input[1] != 0x8B {
        return Err(InflateError::InvalidGzipHeader);
    }
    if input[2] != 8 {
        return Err(InflateError::InvalidGzipHeader);
    }
    let flags = input[3];
    if flags & 0xE0 != 0 {
        return Err(InflateError::InvalidGzipHeader);
    }
    let mut offset = 10usize;
    if flags & 0x04 != 0 {
        let Some(extra_len_bytes) = input.get(offset..offset + 2) else {
            return Ok(None);
        };
        let extra_len = u16::from_le_bytes([extra_len_bytes[0], extra_len_bytes[1]]) as usize;
        offset += 2 + extra_len;
        if offset > input.len() {
            return Ok(None);
        }
    }
    for bit in [0x08u8, 0x10u8] {
        if flags & bit != 0 {
            let Some(end) = input[offset..].iter().position(|byte| *byte == 0) else {
                return Ok(None);
            };
            offset += end + 1;
        }
    }
    if flags & 0x02 != 0 {
        offset += 2;
    }
    if offset > input.len() {
        return Ok(None);
    }
    Ok(Some(offset))
}

/// Convenience: decompresses a zlib stream supplied as chunks.
///
/// The result is byte-identical to [`inflate_zlib`] for valid data; split sizes are
/// irrelevant to the caller.
pub fn inflate_zlib_streaming(
    chunks: &[&[u8]],
    limits: InflateLimits,
) -> Result<Vec<u8>, InflateError> {
    let mut inflater = StreamingInflater::new_zlib(limits);
    let mut output = Vec::new();
    for chunk in chunks {
        inflater.feed(chunk, &mut output)?;
    }
    inflater.finish()?;
    Ok(output)
}

/// Convenience: decompresses a gzip stream supplied as chunks.
pub fn inflate_gzip_streaming(
    chunks: &[&[u8]],
    limits: InflateLimits,
) -> Result<Vec<u8>, InflateError> {
    let mut inflater = StreamingInflater::new_gzip(limits);
    let mut output = Vec::new();
    for chunk in chunks {
        inflater.feed(chunk, &mut output)?;
    }
    inflater.finish()?;
    Ok(output)
}
