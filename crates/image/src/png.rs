//! PNG decoder (W3C PNG 1.2 / ISO 15948) to 8-bit RGBA.
//!
//! Supported colour types and bit depths:
//!
//! | Color type | Channels | Bit depths |
//! |---|---|---|
//! | 0 grayscale | 1 | 1, 2, 4, 8, 16 |
//! | 2 RGB | 3 | 8, 16 |
//! | 3 indexed | 1 | 1, 2, 4, 8 |
//! | 4 grayscale + alpha | 2 | 8, 16 |
//! | 6 RGBA | 4 | 8, 16 |
//!
//! 16-bit samples are narrowed to 8 bits by keeping the most significant byte;
//! sub-byte grayscale is scaled to `0..=255` (`sample * 255 / max`). All
//! scanline filters (0–4) and Adam7 interlacing are implemented. Each chunk's
//! CRC-32 is verified; unknown *ancillary* chunks are skipped, unknown
//! *critical* chunks are rejected with [`ImageError::UnsupportedFeature`].
//!
//! IDAT data is decompressed with the crate-local inflater and checked against
//! the exact scanline size implied by IHDR, so a decompression bomb can only
//! reach the configured [`PngLimits`].

use crate::crc32::crc32_update;
use crate::inflate::{InflateLimits, inflate_zlib};
use crate::rgba_len;
use crate::{ImageError, PNG_SIGNATURE, RgbaImage};

/// Default maximum image width/height in pixels.
pub const DEFAULT_MAX_DIMENSION: u32 = 16_384;
/// Default maximum number of pixels (`width * height`).
pub const DEFAULT_MAX_PIXELS: u64 = 16_000_000;
/// Default maximum size of a single PNG chunk payload.
pub const DEFAULT_MAX_CHUNK_BYTES: usize = 64 * 1024 * 1024;
/// Default maximum total `IDAT` payload.
pub const DEFAULT_MAX_TOTAL_IDAT_BYTES: usize = 256 * 1024 * 1024;

/// Defensive limits applied while decoding a PNG.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PngLimits {
    /// Maximum width and height, in pixels.
    pub max_dimension: u32,
    /// Maximum `width * height`.
    pub max_pixels: u64,
    /// Maximum payload size of a single chunk.
    pub max_chunk_bytes: usize,
    /// Maximum total size of the concatenated `IDAT` payloads.
    pub max_total_idat_bytes: usize,
}

impl PngLimits {
    /// Builds explicit limits.
    pub const fn new(
        max_dimension: u32,
        max_pixels: u64,
        max_chunk_bytes: usize,
        max_total_idat_bytes: usize,
    ) -> Self {
        Self {
            max_dimension,
            max_pixels,
            max_chunk_bytes,
            max_total_idat_bytes,
        }
    }
}

impl Default for PngLimits {
    fn default() -> Self {
        Self {
            max_dimension: DEFAULT_MAX_DIMENSION,
            max_pixels: DEFAULT_MAX_PIXELS,
            max_chunk_bytes: DEFAULT_MAX_CHUNK_BYTES,
            max_total_idat_bytes: DEFAULT_MAX_TOTAL_IDAT_BYTES,
        }
    }
}

/// Adam7 pass geometry: `(x_start, y_start, x_step, y_step)`.
const ADAM7: [[u32; 4]; 7] = [
    [0, 0, 8, 8],
    [4, 0, 8, 8],
    [0, 4, 4, 8],
    [2, 0, 4, 4],
    [0, 2, 2, 4],
    [1, 0, 2, 2],
    [0, 1, 1, 2],
];

/// Parsed and validated IHDR fields.
#[derive(Clone, Copy, Debug)]
struct Header {
    width: u32,
    height: u32,
    bit_depth: u8,
    color_type: u8,
    interlace: u8,
}

impl Header {
    /// Samples per pixel for the colour type.
    fn channels(self) -> u32 {
        match self.color_type {
            0 | 3 => 1,
            2 => 3,
            4 => 2,
            _ => 4,
        }
    }

    /// Bits per pixel (`channels * bit_depth`).
    fn bits_per_pixel(self) -> u64 {
        self.channels() as u64 * self.bit_depth as u64
    }

    /// Bytes per pixel rounded up to a whole byte (filter offset).
    fn filter_bpp(self) -> usize {
        self.bits_per_pixel().div_ceil(8).max(1) as usize
    }
}

/// Transparency from a `tRNS` chunk.
#[derive(Clone, Debug)]
enum Transparency {
    /// Transparent gray sample (colour type 0).
    Gray(u16),
    /// Transparent RGB triple (colour type 2).
    Rgb(u16, u16, u16),
    /// Per-palette-index alpha (colour type 3).
    Palette(Vec<u8>),
}

/// Decoder state shared by the per-pass row renderer.
struct Context<'a> {
    header: Header,
    palette: Option<&'a [[u8; 3]]>,
    transparency: Option<&'a Transparency>,
}

/// Decodes a complete PNG byte stream into an 8-bit RGBA image.
///
/// # Errors
///
/// [`ImageError`] for signature/CRC/truncation/format problems, unsupported
/// PNG features, inflated-size mismatch or exceeded [`PngLimits`].
pub fn decode_png(bytes: &[u8], limits: &PngLimits) -> Result<RgbaImage, ImageError> {
    if !bytes.starts_with(&PNG_SIGNATURE) {
        return Err(ImageError::InvalidSignature);
    }

    let mut position = PNG_SIGNATURE.len();
    let mut header: Option<Header> = None;
    let mut palette: Option<Vec<[u8; 3]>> = None;
    let mut transparency: Option<Transparency> = None;
    let mut idat: Vec<u8> = Vec::new();
    let mut seen_iend = false;

    while position < bytes.len() {
        let length_bytes = take(bytes, &mut position, 4, "chunk length")?;
        let length = be_u32(length_bytes);
        if length as u64 > limits.max_chunk_bytes as u64 {
            return Err(ImageError::LimitExceeded {
                what: "PNG chunk length",
                limit: limits.max_chunk_bytes as u64,
                actual: length as u64,
            });
        }
        let length = length as usize;
        let kind_bytes = take(bytes, &mut position, 4, "chunk type")?;
        let kind = [kind_bytes[0], kind_bytes[1], kind_bytes[2], kind_bytes[3]];
        let data = take(bytes, &mut position, length, "chunk data")?;
        let crc_bytes = take(bytes, &mut position, 4, "chunk crc")?;
        let stored_crc = be_u32(crc_bytes);
        let computed_crc = !crc32_update(crc32_update(0xFFFF_FFFF, &kind), data);
        if stored_crc != computed_crc {
            return Err(ImageError::CrcMismatch {
                chunk: kind,
                expected: stored_crc,
                actual: computed_crc,
            });
        }

        match &kind {
            b"IHDR" => {
                if header.is_some() {
                    return Err(invalid_chunk(kind, "duplicate IHDR"));
                }
                header = Some(parse_ihdr(data, limits)?);
            }
            b"PLTE" => {
                let current = header.ok_or_else(|| invalid_chunk(kind, "PLTE before IHDR"))?;
                if palette.is_some() {
                    return Err(invalid_chunk(kind, "duplicate PLTE"));
                }
                palette = Some(parse_palette(data, current)?);
            }
            b"tRNS" => {
                let current = header.ok_or_else(|| invalid_chunk(kind, "tRNS before IHDR"))?;
                transparency = Some(parse_transparency(data, current, palette.as_deref())?);
            }
            b"IDAT" => {
                let current = header.ok_or_else(|| invalid_chunk(kind, "IDAT before IHDR"))?;
                if current.color_type == 3 && palette.is_none() {
                    return Err(invalid_chunk(kind, "indexed PNG without PLTE"));
                }
                let total =
                    idat.len()
                        .checked_add(data.len())
                        .ok_or(ImageError::LimitExceeded {
                            what: "total IDAT size",
                            limit: limits.max_total_idat_bytes as u64,
                            actual: u64::MAX,
                        })?;
                if total > limits.max_total_idat_bytes {
                    return Err(ImageError::LimitExceeded {
                        what: "total IDAT size",
                        limit: limits.max_total_idat_bytes as u64,
                        actual: total as u64,
                    });
                }
                idat.extend_from_slice(data);
            }
            b"IEND" => {
                if length != 0 {
                    return Err(invalid_chunk(kind, "IEND must be empty"));
                }
                seen_iend = true;
                break;
            }
            _ => {
                // Unknown chunk: critical chunks have the first letter uppercase.
                if kind[0].is_ascii_uppercase() {
                    return Err(ImageError::UnsupportedFeature {
                        feature: "unknown critical PNG chunk",
                    });
                }
                // Ancillary chunks (gAMA, sRGB, iCCP, pHYs, tEXt, ...) are skipped.
            }
        }
    }

    if !seen_iend {
        return Err(ImageError::Truncated {
            context: "IEND chunk",
        });
    }
    let header = header.ok_or_else(|| invalid_chunk(*b"IHDR", "missing IHDR"))?;
    if idat.is_empty() {
        return Err(invalid_chunk(*b"IDAT", "missing IDAT data"));
    }
    if header.color_type == 3 && palette.is_none() {
        return Err(invalid_chunk(*b"PLTE", "indexed PNG without PLTE"));
    }

    let expected = filtered_len(header)?;
    let inflated = inflate_zlib(&idat, InflateLimits::new(expected.saturating_add(64)))?;
    if inflated.len() != expected {
        return Err(ImageError::InvalidData {
            reason: "decompressed size does not match the scanline layout",
        });
    }

    let rgba_size = rgba_len(header.width, header.height)?;
    if rgba_size as u64 > limits.max_pixels.saturating_mul(4) {
        return Err(ImageError::LimitExceeded {
            what: "decoded image bytes",
            limit: limits.max_pixels.saturating_mul(4),
            actual: rgba_size as u64,
        });
    }
    let mut output = vec![0u8; rgba_size];
    let context = Context {
        header,
        palette: palette.as_deref(),
        transparency: transparency.as_ref(),
    };

    let mut offset = 0usize;
    if header.interlace == 0 {
        decode_pass(&context, &inflated, &mut offset, [0, 0, 1, 1], &mut output)?;
    } else {
        for pass in ADAM7 {
            decode_pass(&context, &inflated, &mut offset, pass, &mut output)?;
        }
    }
    if offset != inflated.len() {
        return Err(ImageError::InvalidData {
            reason: "trailing bytes after the interlaced scanlines",
        });
    }

    Ok(RgbaImage {
        width: header.width,
        height: header.height,
        data: output,
    })
}

/// Parses and validates a 13-byte IHDR payload.
fn parse_ihdr(data: &[u8], limits: &PngLimits) -> Result<Header, ImageError> {
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
fn parse_palette(data: &[u8], header: Header) -> Result<Vec<[u8; 3]>, ImageError> {
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

/// Parses a tRNS payload into [`Transparency`].
fn parse_transparency(
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
            Ok(Transparency::Gray(u16::from_be_bytes([data[0], data[1]])))
        }
        2 => {
            if data.len() != 6 {
                return Err(invalid_chunk(*b"tRNS", "tRNS for RGB must be 6 bytes"));
            }
            Ok(Transparency::Rgb(
                u16::from_be_bytes([data[0], data[1]]),
                u16::from_be_bytes([data[2], data[3]]),
                u16::from_be_bytes([data[4], data[5]]),
            ))
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
fn filtered_len(header: Header) -> Result<usize, ImageError> {
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
fn row_bytes(width: u32, bits_per_pixel: u64) -> Result<usize, ImageError> {
    let bits = (width as u64)
        .checked_mul(bits_per_pixel)
        .ok_or_else(limit_error)?;
    let bytes = bits.div_ceil(8);
    usize::try_from(bytes).map_err(|_| limit_error())
}

/// Number of samples along one axis for an Adam7 pass.
fn pass_size(total: u32, start: u32, step: u32) -> u32 {
    if total <= start {
        0
    } else {
        (total - start).div_ceil(step)
    }
}

fn limit_error() -> ImageError {
    ImageError::LimitExceeded {
        what: "scanline size",
        limit: usize::MAX as u64,
        actual: u64::MAX,
    }
}

/// Decodes one interlace pass from `inflated` into `image`.
fn decode_pass(
    context: &Context<'_>,
    inflated: &[u8],
    offset: &mut usize,
    pass: [u32; 4],
    image: &mut [u8],
) -> Result<(), ImageError> {
    let [x0, y0, dx, dy] = pass;
    let width = pass_size(context.header.width, x0, dx);
    let height = pass_size(context.header.height, y0, dy);
    if width == 0 || height == 0 {
        return Ok(());
    }
    let row_length = row_bytes(width, context.header.bits_per_pixel())?;
    let bpp = context.header.filter_bpp();
    let mut previous = vec![0u8; row_length];
    let mut current = vec![0u8; row_length];

    for row_index in 0..height {
        let filter = *inflated.get(*offset).ok_or(ImageError::Truncated {
            context: "scanline filter byte",
        })?;
        *offset += 1;
        let end = offset
            .checked_add(row_length)
            .ok_or(ImageError::Truncated {
                context: "scanline data",
            })?;
        let raw = inflated.get(*offset..end).ok_or(ImageError::Truncated {
            context: "scanline data",
        })?;
        current.copy_from_slice(raw);
        *offset = end;

        unfilter(&mut current, &previous, filter, bpp)?;
        let y = y0 + row_index * dy;
        context.render_row(&current, width, x0, y, dx, image)?;
        std::mem::swap(&mut previous, &mut current);
    }
    Ok(())
}

/// Reverses PNG scanline filtering (types 0–4).
fn unfilter(row: &mut [u8], previous: &[u8], filter: u8, bpp: usize) -> Result<(), ImageError> {
    match filter {
        0 => Ok(()),
        1 => {
            for index in bpp..row.len() {
                row[index] = row[index].wrapping_add(row[index - bpp]);
            }
            Ok(())
        }
        2 => {
            for (index, value) in row.iter_mut().enumerate() {
                *value = value.wrapping_add(previous[index]);
            }
            Ok(())
        }
        3 => {
            for index in 0..row.len() {
                let left = if index >= bpp { row[index - bpp] } else { 0 };
                let up = previous[index];
                let average = ((left as u16 + up as u16) / 2) as u8;
                row[index] = row[index].wrapping_add(average);
            }
            Ok(())
        }
        4 => {
            for index in 0..row.len() {
                let left = if index >= bpp { row[index - bpp] } else { 0 };
                let up = previous[index];
                let upper_left = if index >= bpp {
                    previous[index - bpp]
                } else {
                    0
                };
                row[index] = row[index].wrapping_add(paeth(left, up, upper_left));
            }
            Ok(())
        }
        _ => Err(ImageError::InvalidData {
            reason: "invalid scanline filter type",
        }),
    }
}

/// PNG Paeth predictor (RFC 2083 §6.6).
fn paeth(left: u8, up: u8, upper_left: u8) -> u8 {
    let left = left as i32;
    let up = up as i32;
    let upper_left = upper_left as i32;
    let estimate = left + up - upper_left;
    let distance_left = (estimate - left).abs();
    let distance_up = (estimate - up).abs();
    let distance_upper_left = (estimate - upper_left).abs();
    if distance_left <= distance_up && distance_left <= distance_upper_left {
        left as u8
    } else if distance_up <= distance_upper_left {
        up as u8
    } else {
        upper_left as u8
    }
}

impl Context<'_> {
    /// Converts one unfiltered scanline from its native layout to RGBA8.
    fn render_row(
        &self,
        row: &[u8],
        width: u32,
        x0: u32,
        y: u32,
        dx: u32,
        image: &mut [u8],
    ) -> Result<(), ImageError> {
        let header = self.header;
        for pixel_index in 0..width {
            let Some(x) = x0.checked_add(pixel_index.checked_mul(dx).ok_or_else(bad_row)?) else {
                return Err(bad_row());
            };
            let rgba = self.pixel_from_row(row, pixel_index)?;
            let offset = (y as usize)
                .checked_mul(header.width as usize)
                .and_then(|value| value.checked_add(x as usize))
                .and_then(|value| value.checked_mul(4))
                .ok_or_else(bad_row)?;
            let target = image.get_mut(offset..offset + 4).ok_or_else(bad_row)?;
            target.copy_from_slice(&rgba);
        }
        Ok(())
    }

    /// Reads pixel `index` of an unfiltered scanline and converts it to RGBA8.
    fn pixel_from_row(&self, row: &[u8], index: u32) -> Result<[u8; 4], ImageError> {
        let header = self.header;
        let transparency = self.transparency;
        match header.color_type {
            0 => {
                let sample = match header.bit_depth {
                    8 => u16::from(byte_at(row, index as usize)?),
                    16 => sample_u16(row, index as usize)?,
                    depth => u16::from(packed_sample(row, index as usize, depth)?),
                };
                let gray = gray_to_u8(sample, header.bit_depth);
                let alpha = match transparency {
                    Some(Transparency::Gray(transparent)) if *transparent == sample => 0,
                    _ => 255,
                };
                Ok([gray, gray, gray, alpha])
            }
            2 => {
                let base = (index as usize).checked_mul(3).ok_or_else(bad_row)?;
                let (red, green, blue) = match header.bit_depth {
                    16 => (
                        sample_u16(row, base)?,
                        sample_u16(row, base + 1)?,
                        sample_u16(row, base + 2)?,
                    ),
                    8 => (
                        u16::from(byte_at(row, base)?),
                        u16::from(byte_at(row, base + 1)?),
                        u16::from(byte_at(row, base + 2)?),
                    ),
                    _ => {
                        return Err(ImageError::UnsupportedFeature {
                            feature: "unsupported RGB bit depth",
                        });
                    }
                };
                let alpha = match transparency {
                    Some(Transparency::Rgb(r, g, b)) if (*r, *g, *b) == (red, green, blue) => 0,
                    _ => 255,
                };
                let narrow = |sample: u16| -> u8 {
                    if header.bit_depth == 16 {
                        (sample >> 8) as u8
                    } else {
                        sample as u8
                    }
                };
                Ok([narrow(red), narrow(green), narrow(blue), alpha])
            }
            3 => {
                let palette = self.palette.ok_or_else(bad_row)?;
                let palette_index = match header.bit_depth {
                    8 => byte_at(row, index as usize)? as usize,
                    depth => packed_sample(row, index as usize, depth)? as usize,
                };
                let rgb = palette.get(palette_index).ok_or(ImageError::InvalidData {
                    reason: "palette index out of range",
                })?;
                let alpha = match transparency {
                    Some(Transparency::Palette(alpha)) => {
                        alpha.get(palette_index).copied().unwrap_or(255)
                    }
                    _ => 255,
                };
                Ok([rgb[0], rgb[1], rgb[2], alpha])
            }
            4 => {
                let base = (index as usize).checked_mul(2).ok_or_else(bad_row)?;
                let (gray, alpha) = match header.bit_depth {
                    16 => (sample_u16(row, base)?, sample_u16(row, base + 1)?),
                    8 => (
                        u16::from(byte_at(row, base)?),
                        u16::from(byte_at(row, base + 1)?),
                    ),
                    _ => {
                        return Err(ImageError::UnsupportedFeature {
                            feature: "unsupported grayscale+alpha bit depth",
                        });
                    }
                };
                let gray = gray_to_u8(gray, header.bit_depth);
                let alpha = gray_to_u8(alpha, header.bit_depth);
                Ok([gray, gray, gray, alpha])
            }
            6 => {
                let base = (index as usize).checked_mul(4).ok_or_else(bad_row)?;
                let mut samples = [0u16; 4];
                for (channel, sample) in samples.iter_mut().enumerate() {
                    *sample = match header.bit_depth {
                        16 => sample_u16(row, base + channel)?,
                        8 => u16::from(byte_at(row, base + channel)?),
                        _ => {
                            return Err(ImageError::UnsupportedFeature {
                                feature: "unsupported RGBA bit depth",
                            });
                        }
                    };
                }
                let narrow = |sample: u16| -> u8 {
                    if header.bit_depth == 16 {
                        (sample >> 8) as u8
                    } else {
                        sample as u8
                    }
                };
                Ok([
                    narrow(samples[0]),
                    narrow(samples[1]),
                    narrow(samples[2]),
                    narrow(samples[3]),
                ])
            }
            _ => Err(ImageError::UnsupportedFeature {
                feature: "unsupported PNG colour type",
            }),
        }
    }
}

fn bad_row() -> ImageError {
    ImageError::InvalidData {
        reason: "scanline pixel does not fit the image",
    }
}

/// Narrows a gray sample to 8 bits (MSB for 16-bit, scaled for sub-byte).
fn gray_to_u8(sample: u16, bit_depth: u8) -> u8 {
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
fn byte_at(row: &[u8], index: usize) -> Result<u8, ImageError> {
    row.get(index).copied().ok_or_else(bad_row)
}

/// Big-endian 16-bit sample at `index`.
fn sample_u16(row: &[u8], index: usize) -> Result<u16, ImageError> {
    let offset = index.checked_mul(2).ok_or_else(bad_row)?;
    let bytes = row.get(offset..offset + 2).ok_or_else(bad_row)?;
    Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
}

/// MSB-first packed sample of `bit_depth` (1, 2 or 4 bits) at `index`.
fn packed_sample(row: &[u8], index: usize, bit_depth: u8) -> Result<u8, ImageError> {
    let depth = bit_depth as usize;
    let bit = index.checked_mul(depth).ok_or_else(bad_row)?;
    let byte = byte_at(row, bit / 8)?;
    let shift = 8 - depth - (bit % 8);
    let mask = ((1u16 << depth) - 1) as u8;
    Ok((byte >> shift) & mask)
}

/// Reads a 4-byte big-endian integer from an exactly-4-byte slice.
fn be_u32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

/// Bounds-checked slice extraction used by the chunk reader.
fn take<'a>(
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

fn invalid_chunk(chunk: [u8; 4], reason: &'static str) -> ImageError {
    ImageError::InvalidChunk { chunk, reason }
}
