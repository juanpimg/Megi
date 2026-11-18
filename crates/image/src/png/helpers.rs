#[allow(unused_imports)]
use super::*;

/// Narrows a gray sample to 8 bits (MSB for 16-bit, scaled for sub-byte).
pub(crate) fn gray_to_u8(sample: u16, bit_depth: u8) -> u8 {
    match bit_depth {
        16 => (sample >> 8) as u8,
        8 => sample as u8,
        depth => {
            let max = u32::from((1u16 << depth) - 1);
            ((u32::from(sample) * 255) / max) as u8
        }
    }
}

/// Byte at `index` of `row` or a typed error.
pub(crate) fn byte_at(row: &[u8], index: usize) -> Result<u8, ImageError> {
    row.get(index).copied().ok_or_else(bad_row)
}

/// Big-endian 16-bit sample at `index`.
pub(crate) fn sample_u16(row: &[u8], index: usize) -> Result<u16, ImageError> {
    let offset = index.checked_mul(2).ok_or_else(bad_row)?;
    let bytes = row.get(offset..offset + 2).ok_or_else(bad_row)?;
    Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
}

/// MSB-first packed sample of `bit_depth` (1, 2 or 4 bits) at `index`.
pub(crate) fn packed_sample(row: &[u8], index: usize, bit_depth: u8) -> Result<u8, ImageError> {
    let depth = bit_depth as usize;
    let bit = index.checked_mul(depth).ok_or_else(bad_row)?;
    let byte = byte_at(row, bit / 8)?;
    let shift = 8 - depth - (bit % 8);
    let mask = ((1u16 << depth) - 1) as u8;
    Ok((byte >> shift) & mask)
}

/// Reads a 4-byte big-endian integer from an exactly-4-byte slice.
pub(crate) fn be_u32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

/// Bounds-checked slice extraction used by the chunk reader.
pub(crate) fn take<'a>(
    data: &'a [u8],
    position: &mut usize,
    count: usize,
    context: &'static str,
) -> Result<&'a [u8], ImageError> {
    let end = position
        .checked_add(count)
        .ok_or(ImageError::Truncated { context })?;
    let slice = data
        .get(*position..end)
        .ok_or(ImageError::Truncated { context })?;
    *position = end;
    Ok(slice)
}

pub(crate) fn invalid_chunk(chunk: [u8; 4], reason: &'static str) -> ImageError {
    ImageError::InvalidChunk { chunk, reason }
}
