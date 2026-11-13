//! GIF (87a/89a) first-frame decoder — `std` only, no `unsafe`.
//!
//! ## Supported subset
//!
//! - `GIF87a`/`GIF89a`, logical screen descriptor, global/local colour tables;
//! - LZW-compressed image data (interlaced and non-interlaced), first frame only;
//! - Graphic Control Extension transparency (`transparent index` -> alpha 0);
//! - the returned image is the **logical screen**: the first frame is placed at its
//!   descriptor offset and pixels outside it are transparent (documented; partial frames are
//!   rare in icon/photo usage);
//! - animated GIFs decode their first frame and stop (the rest is ignored, documented).
//!
//! Rejections are typed [`ImageError`]s (truncated data, bad LZW codes, dimensions over
//! [`PngLimits`]) and never panic. GIF87a has no transparency extension, so alpha is 255.

use crate::RgbaImage;
use crate::error::ImageError;
use crate::png::PngLimits;

/// Maximum LZW dictionary size (12-bit codes).
const MAX_LZW_CODES: usize = 4096;

/// Decodes the first frame of a GIF stream into an RGBA image (logical screen size).
pub(crate) fn decode_gif(bytes: &[u8], limits: &PngLimits) -> Result<RgbaImage, ImageError> {
    if bytes.len() < 13 {
        return Err(ImageError::Truncated {
            context: "GIF header",
        });
    }
    if !(bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) {
        return Err(ImageError::InvalidSignature);
    }
    let screen_width = u32::from(u16::from_le_bytes([bytes[6], bytes[7]]));
    let screen_height = u32::from(u16::from_le_bytes([bytes[8], bytes[9]]));
    let packed = bytes[10];
    if screen_width == 0 || screen_height == 0 {
        return Err(ImageError::InvalidDimensions {
            width: screen_width,
            height: screen_height,
        });
    }
    if screen_width > limits.max_dimension || screen_height > limits.max_dimension {
        return Err(ImageError::LimitExceeded {
            what: "GIF dimension",
            limit: u64::from(limits.max_dimension),
            actual: u64::from(screen_width.max(screen_height)),
        });
    }
    let pixels = u64::from(screen_width) * u64::from(screen_height);
    if pixels > limits.max_pixels {
        return Err(ImageError::LimitExceeded {
            what: "GIF pixel count",
            limit: limits.max_pixels,
            actual: pixels,
        });
    }
    let mut index = 13usize;
    let global_table = if packed & 0x80 != 0 {
        let entries = 1usize << ((packed & 0x07) + 1);
        Some(read_palette(bytes, &mut index, entries)?)
    } else {
        None
    };

    let mut transparent_index: Option<u8> = None;
    let mut canvas = vec![0u8; (screen_width as usize) * (screen_height as usize) * 4];
    loop {
        let block = *bytes.get(index).ok_or(ImageError::Truncated {
            context: "GIF block",
        })?;
        index += 1;
        match block {
            0x21 => {
                // Extension introducer: label + sub-blocks.
                let label = *bytes.get(index).ok_or(ImageError::Truncated {
                    context: "GIF extension",
                })?;
                index += 1;
                if label == 0xF9 {
                    // Graphic Control Extension (4-byte payload + terminator).
                    let size = *bytes
                        .get(index)
                        .ok_or(ImageError::Truncated { context: "GIF GCE" })?;
                    index += 1;
                    if size != 4 {
                        return Err(ImageError::InvalidData {
                            reason: "invalid GIF graphics control extension size",
                        });
                    }
                    let block = bytes.get(index..index + 4).ok_or(ImageError::Truncated {
                        context: "GIF GCE payload",
                    })?;
                    index += 4;
                    if block[0] & 0x01 != 0 {
                        transparent_index = Some(block[3]);
                    }
                    skip_sub_blocks(bytes, &mut index)?;
                } else {
                    skip_sub_blocks(bytes, &mut index)?;
                }
            }
            0x2C => {
                // Image descriptor.
                let descriptor = bytes.get(index..index + 9).ok_or(ImageError::Truncated {
                    context: "GIF image descriptor",
                })?;
                index += 9;
                let frame_left = u32::from(u16::from_le_bytes([descriptor[0], descriptor[1]]));
                let frame_top = u32::from(u16::from_le_bytes([descriptor[2], descriptor[3]]));
                let frame_width = u32::from(u16::from_le_bytes([descriptor[4], descriptor[5]]));
                let frame_height = u32::from(u16::from_le_bytes([descriptor[6], descriptor[7]]));
                let frame_packed = descriptor[8];
                if frame_width == 0 || frame_height == 0 {
                    return Err(ImageError::InvalidDimensions {
                        width: frame_width,
                        height: frame_height,
                    });
                }
                // The frame is decoded in full before clipping to the screen, so it must
                // obey the same dimension/pixel budgets as the screen (issue #874).
                if frame_width > limits.max_dimension || frame_height > limits.max_dimension {
                    return Err(ImageError::LimitExceeded {
                        what: "GIF frame dimension",
                        limit: u64::from(limits.max_dimension),
                        actual: u64::from(frame_width.max(frame_height)),
                    });
                }
                let local_table = if frame_packed & 0x80 != 0 {
                    let entries = 1usize << ((frame_packed & 0x07) + 1);
                    Some(read_palette(bytes, &mut index, entries)?)
                } else {
                    None
                };
                let palette = match local_table.as_ref().or(global_table.as_ref()) {
                    Some(palette) => palette,
                    None => {
                        return Err(ImageError::InvalidData {
                            reason: "GIF image has no colour table",
                        });
                    }
                };
                let interlaced = frame_packed & 0x40 != 0;
                let min_code_size = *bytes.get(index).ok_or(ImageError::Truncated {
                    context: "GIF LZW minimum code size",
                })?;
                index += 1;
                let frame_pixels = (frame_width as usize)
                    .checked_mul(frame_height as usize)
                    .ok_or(ImageError::LimitExceeded {
                        what: "GIF frame pixels",
                        limit: limits.max_pixels,
                        actual: pixels,
                    })?;
                if frame_pixels as u64 > limits.max_pixels {
                    return Err(ImageError::LimitExceeded {
                        what: "GIF frame pixel count",
                        limit: limits.max_pixels,
                        actual: frame_pixels as u64,
                    });
                }
                // Cap buffered LZW data against a multiple of the frame pixel
                // count so a 1-pixel frame cannot carry megabytes of payload
                // (#618). LZW data is never larger than a few bytes per pixel.
                let max_image_data = frame_pixels
                    .saturating_mul(4)
                    .saturating_add(65_536)
                    .min(limits.max_total_idat_bytes);
                let data = read_sub_blocks(bytes, &mut index, max_image_data)?;
                let indices = lzw_decode(min_code_size, &data, frame_pixels)?;
                let row_order = if interlaced {
                    interlaced_rows(frame_height as usize)
                } else {
                    (0..frame_height as usize).collect()
                };
                for (source_row, target_row) in row_order.iter().enumerate() {
                    let y = *target_row;
                    if y >= screen_height as usize {
                        break;
                    }
                    for x in 0..frame_width as usize {
                        let source = source_row * frame_width as usize + x;
                        let Some(palette_index) = indices.get(source).copied() else {
                            return Err(ImageError::InvalidData {
                                reason: "GIF LZW produced fewer pixels than the frame",
                            });
                        };
                        let canvas_x = frame_left as usize + x;
                        let canvas_y = frame_top as usize + y;
                        if canvas_x >= screen_width as usize || canvas_y >= screen_height as usize {
                            continue;
                        }
                        let [r, g, b] = palette.get(usize::from(palette_index)).copied().ok_or(
                            ImageError::InvalidData {
                                reason: "GIF palette index out of range",
                            },
                        )?;
                        let alpha = if transparent_index == Some(palette_index) {
                            0
                        } else {
                            255
                        };
                        let offset = (canvas_y * screen_width as usize + canvas_x) * 4;
                        canvas[offset] = r;
                        canvas[offset + 1] = g;
                        canvas[offset + 2] = b;
                        canvas[offset + 3] = alpha;
                    }
                }
                // First frame only (GIF animation is out of scope).
                return RgbaImage::new(screen_width, screen_height, canvas);
            }
            0x3B => {
                return Err(ImageError::InvalidData {
                    reason: "GIF ended without an image frame",
                });
            }
            _ => {
                return Err(ImageError::InvalidData {
                    reason: "unknown GIF block introducer",
                });
            }
        }
    }
}

fn read_palette(
    bytes: &[u8],
    index: &mut usize,
    entries: usize,
) -> Result<Vec<[u8; 3]>, ImageError> {
    let raw = bytes
        .get(*index..*index + entries * 3)
        .ok_or(ImageError::Truncated {
            context: "GIF colour table",
        })?;
    *index += entries * 3;
    Ok(raw.chunks(3).map(|rgb| [rgb[0], rgb[1], rgb[2]]).collect())
}

fn skip_sub_blocks(bytes: &[u8], index: &mut usize) -> Result<(), ImageError> {
    loop {
        let size = *bytes.get(*index).ok_or(ImageError::Truncated {
            context: "GIF data sub-block",
        })? as usize;
        *index += 1;
        if size == 0 {
            return Ok(());
        }
        *index += size;
        if *index > bytes.len() {
            return Err(ImageError::Truncated {
                context: "GIF data sub-block",
            });
        }
    }
}

fn read_sub_blocks(
    bytes: &[u8],
    index: &mut usize,
    max_bytes: usize,
) -> Result<Vec<u8>, ImageError> {
    let mut data = Vec::new();
    loop {
        let size = *bytes.get(*index).ok_or(ImageError::Truncated {
            context: "GIF data sub-block",
        })? as usize;
        *index += 1;
        if size == 0 {
            return Ok(data);
        }
        if data.len().saturating_add(size) > max_bytes {
            return Err(ImageError::LimitExceeded {
                what: "GIF image data",
                limit: max_bytes as u64,
                actual: data.len().saturating_add(size) as u64,
            });
        }
        let block = bytes
            .get(*index..*index + size)
            .ok_or(ImageError::Truncated {
                context: "GIF data sub-block",
            })?;
        data.extend_from_slice(block);
        *index += size;
    }
}

/// Interlaced GIF row order (GIF89a §20): 8-row passes.
fn interlaced_rows(height: usize) -> Vec<usize> {
    let mut rows = Vec::with_capacity(height);
    for (start, step) in [(0usize, 8usize), (4, 8), (2, 4), (1, 2)] {
        let mut row = start;
        while row < height {
            rows.push(row);
            row += step;
        }
    }
    rows
}

/// MSB-first bit reader for the LZW stream.
struct LzwBits<'a> {
    data: &'a [u8],
    bit: usize,
}

impl<'a> LzwBits<'a> {
    fn read(&mut self, count: u8) -> Result<u16, ImageError> {
        // GIF packs LZW codes least-significant-bit first across the byte stream.
        let mut value = 0u16;
        for shift in 0..count {
            let byte_index = self.bit / 8;
            let bit_index = self.bit % 8;
            let byte = *self.data.get(byte_index).ok_or(ImageError::Truncated {
                context: "GIF LZW stream",
            })?;
            value |= u16::from((byte >> bit_index) & 1) << shift;
            self.bit += 1;
        }
        Ok(value)
    }
}

/// Decompresses GIF LZW image data into palette indices.
fn lzw_decode(min_code_size: u8, data: &[u8], pixel_count: usize) -> Result<Vec<u8>, ImageError> {
    if !(2..=8).contains(&min_code_size) {
        return Err(ImageError::InvalidData {
            reason: "invalid GIF LZW minimum code size",
        });
    }
    let clear: u16 = 1 << min_code_size;
    let end: u16 = clear + 1;
    let mut code_size = min_code_size + 1;
    let mut dictionary: Vec<(u16, u8)> = Vec::with_capacity(MAX_LZW_CODES);
    reset_dictionary(&mut dictionary, clear);
    let mut previous: Option<u16> = None;
    let mut output: Vec<u8> = Vec::with_capacity(pixel_count);
    let mut stack: Vec<u8> = Vec::with_capacity(MAX_LZW_CODES);
    let mut bits = LzwBits { data, bit: 0 };
    while output.len() < pixel_count {
        let code = bits.read(code_size)?;
        if code == clear {
            reset_dictionary(&mut dictionary, clear);
            code_size = min_code_size + 1;
            previous = None;
            continue;
        }
        if code == end {
            break;
        }
        let Some(previous_code) = previous else {
            if code >= clear {
                return Err(ImageError::InvalidData {
                    reason: "GIF LZW first code is not a literal",
                });
            }
            output.push(code as u8);
            previous = Some(code);
            continue;
        };
        if code < dictionary.len() as u16 {
            emit_code(code, &dictionary, &mut stack, &mut output)?;
        } else if code == dictionary.len() as u16 {
            // KwKwK case: previous string plus its own first byte.
            emit_code(previous_code, &dictionary, &mut stack, &mut output)?;
            let first = *stack.last().unwrap_or(&0);
            output.push(first);
            stack.push(first);
            stack.pop();
        } else {
            return Err(ImageError::InvalidData {
                reason: "GIF LZW code beyond the dictionary",
            });
        }
        if dictionary.len() < MAX_LZW_CODES {
            let first = *stack.last().unwrap_or(&0);
            dictionary.push((previous_code, first));
        }
        if dictionary.len() == (1usize << code_size) && code_size < 12 {
            code_size += 1;
        }
        previous = Some(code);
    }
    if output.len() != pixel_count {
        return Err(ImageError::InvalidData {
            reason: "GIF LZW pixel count mismatch",
        });
    }
    output.truncate(pixel_count);
    Ok(output)
}

fn reset_dictionary(dictionary: &mut Vec<(u16, u8)>, clear: u16) {
    dictionary.clear();
    dictionary.push((u16::MAX, 0)); // code 0 is the first literal
    for value in 1..clear {
        dictionary.push((u16::MAX, value as u8));
    }
    dictionary.push((u16::MAX, 0)); // clear code placeholder (never emitted)
    dictionary.push((u16::MAX, 0)); // end code placeholder
}

/// Emits the string for `code` into `output`, keeping its first byte at `stack.last()`.
fn emit_code(
    code: u16,
    dictionary: &[(u16, u8)],
    stack: &mut Vec<u8>,
    output: &mut Vec<u8>,
) -> Result<(), ImageError> {
    stack.clear();
    let mut current = code;
    let mut guard = 0usize;
    while current != u16::MAX {
        let entry = dictionary
            .get(current as usize)
            .copied()
            .ok_or(ImageError::InvalidData {
                reason: "GIF LZW dictionary index out of range",
            })?;
        stack.push(entry.1);
        current = entry.0;
        guard += 1;
        if guard > MAX_LZW_CODES {
            return Err(ImageError::InvalidData {
                reason: "GIF LZW cyclic dictionary",
            });
        }
    }
    for value in stack.iter().rev() {
        output.push(*value);
    }
    Ok(())
}
