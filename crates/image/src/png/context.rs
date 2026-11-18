#[allow(unused_imports)]
use super::*;

impl Context<'_> {
    /// Converts one unfiltered scanline from its native layout to RGBA8.
    #[inline(always)]
    pub(crate) fn render_row(
        &self,
        row: &[u8],
        width: u32,
        x0: u32,
        y: u32,
        dx: u32,
        image: &mut [u8],
    ) -> Result<(), ImageError> {
        let header = self.header;
        // Fast path for the common non-interlaced full-byte layouts: compute the row
        // destination once and convert in tight chunks instead of doing checked index
        // arithmetic per pixel.
        if x0 == 0 && dx == 1 {
            let row_start = (y as usize)
                .checked_mul(header.width as usize)
                .and_then(|value| value.checked_mul(4))
                .ok_or_else(bad_row)?;
            let row_pixels = width as usize;
            let end = row_start
                .checked_add(row_pixels.checked_mul(4).ok_or_else(bad_row)?)
                .ok_or_else(bad_row)?;
            let target = image.get_mut(row_start..end).ok_or_else(bad_row)?;
            match (header.color_type, header.bit_depth, self.transparency) {
                (2, 8, None) => {
                    let (sources, _) = row.as_chunks::<3>();
                    let (destinations, _) = target.as_chunks_mut::<4>();
                    for (source, dest) in sources.iter().zip(destinations.iter_mut()) {
                        dest[0] = source[0];
                        dest[1] = source[1];
                        dest[2] = source[2];
                        dest[3] = 255;
                    }
                    return Ok(());
                }
                (6, 8, _) => {
                    target.copy_from_slice(&row[..row_pixels * 4]);
                    return Ok(());
                }
                (0, 8, None) => {
                    let (destinations, _) = target.as_chunks_mut::<4>();
                    for (source, dest) in row.iter().zip(destinations.iter_mut()) {
                        dest[0] = *source;
                        dest[1] = *source;
                        dest[2] = *source;
                        dest[3] = 255;
                    }
                    return Ok(());
                }
                (3, 8, None) => {
                    let palette = self.palette.ok_or_else(bad_row)?;
                    let (destinations, _) = target.as_chunks_mut::<4>();
                    for (source, dest) in row.iter().zip(destinations.iter_mut()) {
                        let entry = palette.get(*source as usize).ok_or_else(bad_row)?;
                        dest[0] = entry[0];
                        dest[1] = entry[1];
                        dest[2] = entry[2];
                        dest[3] = 255;
                    }
                    return Ok(());
                }
                _ => {}
            }
        }
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
    #[inline(always)]
    pub(crate) fn pixel_from_row(&self, row: &[u8], index: u32) -> Result<[u8; 4], ImageError> {
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

pub(crate) fn bad_row() -> ImageError {
    ImageError::InvalidData {
        reason: "scanline pixel does not fit the image",
    }
}
