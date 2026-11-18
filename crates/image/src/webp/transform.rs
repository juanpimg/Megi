#[allow(unused_imports)]
use super::*;

pub(crate) enum Transform {
    Predictor {
        width: usize,
        height: usize,
        bits: u32,
        data: Vec<u32>,
    },
    Color {
        width: usize,
        height: usize,
        bits: u32,
        data: Vec<u32>,
    },
    SubtractGreen,
    Indexing {
        width: usize,
        height: usize,
        bits: u32,
        palette: Vec<u32>,
    },
}

pub(crate) fn avg2(a: u32, b: u32) -> u32 {
    let mut result = 0u32;
    for shift in (0..32).step_by(8) {
        let ca = (a >> shift) & 0xff;
        let cb = (b >> shift) & 0xff;
        result |= ((ca + cb) >> 1) << shift;
    }
    result
}

pub(crate) fn add_pixels(a: u32, b: u32) -> u32 {
    let mut result = 0u32;
    for shift in (0..32).step_by(8) {
        let ca = (a >> shift) & 0xff;
        let cb = (b >> shift) & 0xff;
        result |= ((ca + cb) & 0xff) << shift;
    }
    result
}

pub(crate) fn clip255(value: i32) -> u32 {
    value.clamp(0, 255) as u32
}

pub(crate) fn clamped_add_subtract_full(a: u32, b: u32, c: u32) -> u32 {
    let mut result = 0u32;
    for shift in (0..32).step_by(8) {
        let ca = ((a >> shift) & 0xff) as i32;
        let cb = ((b >> shift) & 0xff) as i32;
        let cc = ((c >> shift) & 0xff) as i32;
        result |= clip255(ca + cb - cc) << shift;
    }
    result
}

pub(crate) fn clamped_add_subtract_half(a: u32, b: u32) -> u32 {
    let average = avg2(a, b);
    let mut result = 0u32;
    for shift in (0..32).step_by(8) {
        let ca = ((average >> shift) & 0xff) as i32;
        let cb = ((b >> shift) & 0xff) as i32;
        result |= clip255(ca + (ca - cb) / 2) << shift;
    }
    result
}

pub(crate) fn select(a: u32, b: u32, c: u32) -> u32 {
    let mut sum = 0i32;
    for shift in (0..32).step_by(8) {
        let ca = ((a >> shift) & 0xff) as i32;
        let cb = ((b >> shift) & 0xff) as i32;
        let cc = ((c >> shift) & 0xff) as i32;
        sum += (cb - cc).abs() - (ca - cc).abs();
    }
    if sum <= 0 { a } else { b }
}

pub(crate) fn predictor_mode(mode: u32, left: u32, top: u32, top_left: u32, top_right: u32) -> u32 {
    match mode {
        0 => 0xff00_0000,
        1 => left,
        2 => top,
        3 => top_right,
        4 => top_left,
        5 => avg2(avg2(left, top_right), top),
        6 => avg2(left, top_left),
        7 => avg2(left, top),
        8 => avg2(top_left, top),
        9 => avg2(top, top_right),
        10 => avg2(avg2(left, top_left), avg2(top, top_right)),
        11 => select(top, left, top_left),
        12 => clamped_add_subtract_full(left, top, top_left),
        13 => clamped_add_subtract_half(avg2(left, top), top_left),
        _ => 0xff00_0000,
    }
}

impl Transform {
    pub(crate) fn apply(self, pixels: Vec<u32>) -> Result<Vec<u32>, ImageError> {
        match self {
            Transform::Predictor {
                width,
                height,
                bits,
                data,
            } => {
                let tile_width = 1usize << bits;
                let tiles = width.div_ceil(tile_width);
                let mut out = Vec::with_capacity(pixels.len());
                for y in 0..height {
                    for x in 0..width {
                        let residual = pixels[y * width + x];
                        let prediction = if y == 0 && x == 0 {
                            0xff00_0000
                        } else if y == 0 {
                            out[out.len() - 1]
                        } else if x == 0 {
                            out[(y - 1) * width]
                        } else {
                            let mode_index = (y >> bits) * tiles + (x >> bits);
                            let mode = (data[mode_index] >> 8) & 0xf;
                            if mode >= 14 {
                                return Err(ImageError::InvalidData {
                                    reason: "invalid VP8L predictor mode",
                                });
                            }
                            let left = out[y * width + x - 1];
                            let top = out[(y - 1) * width + x];
                            let top_left = out[(y - 1) * width + x - 1];
                            let top_right = if x + 1 < width {
                                out[(y - 1) * width + x + 1]
                            } else {
                                out[(y - 1) * width + x]
                            };
                            predictor_mode(mode, left, top, top_left, top_right)
                        };
                        out.push(add_pixels(residual, prediction));
                    }
                }
                Ok(out)
            }
            Transform::Color {
                width,
                height,
                bits,
                data,
            } => {
                let tile_width = 1usize << bits;
                let tiles = width.div_ceil(tile_width);
                let mut out = Vec::with_capacity(pixels.len());
                for y in 0..height {
                    for x in 0..width {
                        let argb = pixels[y * width + x];
                        let code = data[(y >> bits) * tiles + (x >> bits)];
                        let green_to_red = code & 0xff;
                        let green_to_blue = (code >> 8) & 0xff;
                        let red_to_blue = (code >> 16) & 0xff;
                        let green = ((argb >> 8) & 0xff) as i8;
                        let red = (argb >> 16) & 0xff;
                        let blue = argb & 0xff;
                        let new_red = (red as i32
                            + (((green_to_red as i8 as i32) * (green as i32)) >> 5))
                            & 0xff;
                        let new_blue = (blue as i32
                            + (((green_to_blue as i8 as i32) * (green as i32)) >> 5)
                            + (((red_to_blue as i8 as i32) * (new_red as i8 as i32)) >> 5))
                            & 0xff;
                        out.push((argb & 0xff00_ff00) | ((new_red as u32) << 16) | new_blue as u32);
                    }
                }
                Ok(out)
            }
            Transform::SubtractGreen => Ok(pixels
                .into_iter()
                .map(|argb| {
                    let green = (argb >> 8) & 0xff;
                    let red_blue =
                        (argb & 0x00ff_00ff).wrapping_add((green << 16) | green) & 0x00ff_00ff;
                    (argb & 0xff00_ff00) | red_blue
                })
                .collect()),
            Transform::Indexing {
                width,
                height,
                bits,
                palette,
            } => {
                let input_width = pixels.len().checked_div(height).unwrap_or(0);
                let mut out = Vec::with_capacity(height * width);
                let pixels_per_byte = 1usize << bits;
                let bits_per_index = 8u32 >> bits;
                let mask = (1u32 << bits_per_index) - 1;
                for y in 0..height {
                    for x in 0..input_width {
                        let packed = (pixels[y * input_width + x] >> 8) & 0xff;
                        for slot in 0..pixels_per_byte {
                            if x * pixels_per_byte + slot >= width {
                                break;
                            }
                            let index =
                                ((packed >> (slot as u32 * bits_per_index)) & mask) as usize;
                            let Some(color) = palette.get(index).copied() else {
                                return Err(ImageError::InvalidData {
                                    reason: "VP8L palette index out of range",
                                });
                            };
                            out.push(color);
                        }
                    }
                }
                Ok(out)
            }
        }
    }
}

pub(crate) fn decode_transform(
    reader: &mut BitReader<'_>,
    limits: &PngLimits,
    width: &mut usize,
    height: usize,
) -> Result<Transform, ImageError> {
    let kind = reader.read_bits(2)?;
    let transform = match kind {
        0 | 1 => {
            let bits = 2 + reader.read_bits(3)?;
            let tile_width = width.div_ceil(1usize << bits);
            let tile_height = height.div_ceil(1usize << bits);
            let data = decode_entropy_image(reader, tile_width, tile_height, limits)?;
            if kind == 0 {
                Transform::Predictor {
                    width: *width,
                    height,
                    bits,
                    data,
                }
            } else {
                Transform::Color {
                    width: *width,
                    height,
                    bits,
                    data,
                }
            }
        }
        2 => Transform::SubtractGreen,
        3 => {
            let num_colors = reader.read_bits(8)? as usize + 1;
            let bits = if num_colors > 16 {
                0
            } else if num_colors > 4 {
                1
            } else if num_colors > 2 {
                2
            } else {
                3
            };
            let input_width = width.div_ceil(1usize << bits);
            let map = decode_entropy_image(reader, num_colors, 1, limits)?;
            let mut palette = Vec::with_capacity(num_colors);
            let mut expanded = [0u8; 4];
            for (index, &color) in map.iter().enumerate() {
                let bytes = color.to_le_bytes();
                if index == 0 {
                    expanded = bytes;
                } else {
                    for channel in 0..4 {
                        expanded[channel] = expanded[channel].wrapping_add(bytes[channel]);
                    }
                }
                palette.push(u32::from_le_bytes(expanded));
            }
            let transform = Transform::Indexing {
                width: *width,
                height,
                bits,
                palette,
            };
            *width = input_width;
            transform
        }
        _ => unreachable!(),
    };
    Ok(transform)
}
