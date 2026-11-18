#[allow(unused_imports)]
use super::*;

/// Parses and validates a 13-byte IHDR payload.
pub(crate) fn parse_ihdr(data: &[u8], limits: &PngLimits) -> Result<Header, ImageError> {
    if data.len() != 13 {
        return Err(invalid_chunk(*b"IHDR", "IHDR must be 13 bytes"));
    }
    let width = be_u32(&data[0..4]);
    let height = be_u32(&data[4..8]);
    let bit_depth = data[8];
    let color_type = data[9];
    let compression = data[10];
    let filter = data[11];
    let interlace = data[12];

    if width == 0 || height == 0 {
        return Err(ImageError::InvalidDimensions { width, height });
    }
    if width > limits.max_dimension || height > limits.max_dimension {
        return Err(ImageError::LimitExceeded {
            what: "PNG dimension",
            limit: limits.max_dimension as u64,
            actual: width.max(height) as u64,
        });
    }
    let pixels = (width as u64) * (height as u64);
    if pixels > limits.max_pixels {
        return Err(ImageError::LimitExceeded {
            what: "PNG total pixels",
            limit: limits.max_pixels,
            actual: pixels,
        });
    }
    if compression != 0 {
        return Err(invalid_chunk(*b"IHDR", "unsupported compression method"));
    }
    if filter != 0 {
        return Err(invalid_chunk(*b"IHDR", "unsupported filter method"));
    }
    if interlace > 1 {
        return Err(invalid_chunk(*b"IHDR", "invalid interlace method"));
    }
    let depth_ok = match color_type {
        0 => matches!(bit_depth, 1 | 2 | 4 | 8 | 16),
        2 | 4 | 6 => matches!(bit_depth, 8 | 16),
        3 => matches!(bit_depth, 1 | 2 | 4 | 8),
        _ => false,
    };
    if !depth_ok {
        return Err(invalid_chunk(
            *b"IHDR",
            "invalid colour type / bit depth combination",
        ));
    }

    Ok(Header {
        width,
        height,
        bit_depth,
        color_type,
        interlace,
    })
}

/// Parses and validates a PLTE payload.
pub(crate) fn parse_palette(data: &[u8], header: Header) -> Result<Vec<[u8; 3]>, ImageError> {
    if matches!(header.color_type, 0 | 4) {
        return Err(invalid_chunk(
            *b"PLTE",
            "PLTE is not allowed for grayscale images",
        ));
    }
    if !data.len().is_multiple_of(3) {
        return Err(invalid_chunk(
            *b"PLTE",
            "PLTE length is not a multiple of 3",
        ));
    }
    let entries = data.len() / 3;
    if entries == 0 || entries > 256 {
        return Err(invalid_chunk(*b"PLTE", "PLTE has an invalid entry count"));
    }
    if header.color_type == 3 && entries > (1usize << header.bit_depth) {
        return Err(invalid_chunk(
            *b"PLTE",
            "PLTE has more entries than the bit depth allows",
        ));
    }
    let (entries, _remainder) = data.as_chunks::<3>();
    Ok(entries.to_vec())
}

/// Largest sample value representable at `bit_depth` bits.
pub(crate) fn sample_max(bit_depth: u8) -> u32 {
    if bit_depth >= 16 {
        0xFFFF
    } else {
        (1u32 << bit_depth) - 1
    }
}

/// Parses a tRNS payload into [`Transparency`].
pub(crate) fn parse_transparency(
    data: &[u8],
    header: Header,
    palette: Option<&[[u8; 3]]>,
) -> Result<Transparency, ImageError> {
    match header.color_type {
        0 => {
            if data.len() != 2 {
                return Err(invalid_chunk(
                    *b"tRNS",
                    "tRNS for grayscale must be 2 bytes",
                ));
            }
            let value = u16::from_be_bytes([data[0], data[1]]);
            if u32::from(value) > sample_max(header.bit_depth) {
                return Err(invalid_chunk(
                    *b"tRNS",
                    "tRNS grayscale sample exceeds the bit depth",
                ));
            }
            Ok(Transparency::Gray(value))
        }
        2 => {
            if data.len() != 6 {
                return Err(invalid_chunk(*b"tRNS", "tRNS for RGB must be 6 bytes"));
            }
            let max = sample_max(header.bit_depth);
            let red = u16::from_be_bytes([data[0], data[1]]);
            let green = u16::from_be_bytes([data[2], data[3]]);
            let blue = u16::from_be_bytes([data[4], data[5]]);
            if [red, green, blue]
                .iter()
                .any(|sample| u32::from(*sample) > max)
            {
                return Err(invalid_chunk(
                    *b"tRNS",
                    "tRNS RGB sample exceeds the bit depth",
                ));
            }
            Ok(Transparency::Rgb(red, green, blue))
        }
        3 => {
            let palette = palette.ok_or_else(|| invalid_chunk(*b"tRNS", "tRNS before PLTE"))?;
            if data.len() > palette.len() {
                return Err(invalid_chunk(
                    *b"tRNS",
                    "tRNS has more alpha entries than PLTE has colours",
                ));
            }
            Ok(Transparency::Palette(data.to_vec()))
        }
        _ => Err(invalid_chunk(
            *b"tRNS",
            "tRNS is not allowed for alpha colour types",
        )),
    }
}

/// Decompressed scanline size implied by IHDR (including filter bytes).
pub(crate) fn filtered_len(header: Header) -> Result<usize, ImageError> {
    let bits_per_pixel = header.bits_per_pixel();
    let mut total = 0usize;
    if header.interlace == 0 {
        let row = row_bytes(header.width, bits_per_pixel)?;
        total = row
            .checked_add(1)
            .and_then(|row| row.checked_mul(header.height as usize))
            .ok_or_else(limit_error)?;
        return Ok(total);
    }
    for [x0, y0, dx, dy] in ADAM7 {
        let width = pass_size(header.width, x0, dx);
        let height = pass_size(header.height, y0, dy);
        if width == 0 || height == 0 {
            continue;
        }
        let row = row_bytes(width, bits_per_pixel)?;
        total = total
            .checked_add(
                row.checked_add(1)
                    .and_then(|row| row.checked_mul(height as usize))
                    .ok_or_else(limit_error)?,
            )
            .ok_or_else(limit_error)?;
    }
    Ok(total)
}

/// Bytes per scanline (without the filter byte).
pub(crate) fn row_bytes(width: u32, bits_per_pixel: u64) -> Result<usize, ImageError> {
    let bits = (width as u64)
        .checked_mul(bits_per_pixel)
        .ok_or_else(limit_error)?;
    let bytes = bits.div_ceil(8);
    usize::try_from(bytes).map_err(|_| limit_error())
}

/// Number of samples along one axis for an Adam7 pass.
pub(crate) fn pass_size(total: u32, start: u32, step: u32) -> u32 {
    if total <= start {
        0
    } else {
        (total - start).div_ceil(step)
    }
}

pub(crate) fn limit_error() -> ImageError {
    ImageError::LimitExceeded {
        what: "scanline size",
        limit: usize::MAX as u64,
        actual: u64::MAX,
    }
}
