#[allow(unused_imports)]
use super::*;

pub(crate) struct Groups {
    pub(crate) codes: Vec<[Huffman; 5]>,
}

impl Groups {
    pub(crate) fn read(
        reader: &mut BitReader<'_>,
        cache_bits: u32,
        count: usize,
    ) -> Result<Self, ImageError> {
        if count == 0 || count > MAX_HUFFMAN_GROUPS {
            return Err(ImageError::LimitExceeded {
                what: "WebP VP8L Huffman code groups",
                limit: MAX_HUFFMAN_GROUPS as u64,
                actual: count as u64,
            });
        }
        let green_alphabet = NUM_LITERAL_CODES
            + NUM_LENGTH_CODES
            + if cache_bits > 0 {
                1u16 << cache_bits
            } else {
                0
            };
        let mut codes = Vec::with_capacity(count);
        for _ in 0..count {
            let green = read_huffman_code(reader, green_alphabet)?;
            let red = read_huffman_code(reader, NUM_LITERAL_CODES)?;
            let blue = read_huffman_code(reader, NUM_LITERAL_CODES)?;
            let alpha = read_huffman_code(reader, NUM_LITERAL_CODES)?;
            let distance = read_huffman_code(reader, NUM_DISTANCE_CODES)?;
            codes.push([green, red, blue, alpha, distance]);
        }
        Ok(Self { codes })
    }

    pub(crate) fn get(&self, index: usize) -> Result<&[Huffman; 5], ImageError> {
        self.codes.get(index).ok_or(ImageError::InvalidData {
            reason: "VP8L meta prefix index out of range",
        })
    }
}

/// Color cache (hash table) used by VP8L literals/backward references.
pub(crate) struct ColorCache {
    pub(crate) colors: Vec<u32>,
    pub(crate) shift: u32,
}

impl ColorCache {
    pub(crate) fn new(bits: u32) -> Self {
        Self {
            colors: vec![0; 1usize << bits],
            shift: 32 - bits,
        }
    }

    pub(crate) fn insert(&mut self, argb: u32) {
        let key = ((argb.wrapping_mul(0x1e35_a7bd)) >> self.shift) as usize;
        if let Some(slot) = self.colors.get_mut(key) {
            *slot = argb;
        }
    }

    pub(crate) fn lookup(&self, index: usize) -> Result<u32, ImageError> {
        self.colors
            .get(index)
            .copied()
            .ok_or(ImageError::InvalidData {
                reason: "VP8L colour-cache index out of range",
            })
    }
}

pub(crate) fn read_color_cache(
    reader: &mut BitReader<'_>,
) -> Result<Option<ColorCache>, ImageError> {
    if reader.read_bit()? == 0 {
        return Ok(None);
    }
    let bits = reader.read_bits(4)?;
    if !(1..=MAX_CACHE_BITS).contains(&bits) {
        return Err(ImageError::InvalidData {
            reason: "VP8L colour-cache size out of range",
        });
    }
    Ok(Some(ColorCache::new(bits)))
}

pub(crate) fn get_copy_value(
    prefix_code: u16,
    reader: &mut BitReader<'_>,
) -> Result<u32, ImageError> {
    let code = u32::from(prefix_code);
    if code < 4 {
        return Ok(code + 1);
    }
    let extra_bits = (code - 2) >> 1;
    let offset = (2 + (code & 1)) << extra_bits;
    Ok(offset + reader.read_bits(extra_bits)? + 1)
}

pub(crate) const CODE_TO_PLANE: [u8; CODE_TO_PLANE_CODES] = [
    0x18, 0x07, 0x17, 0x19, 0x28, 0x06, 0x27, 0x29, 0x16, 0x1a, 0x26, 0x2a, 0x38, 0x05, 0x37, 0x39,
    0x15, 0x1b, 0x36, 0x3a, 0x25, 0x2b, 0x48, 0x04, 0x47, 0x49, 0x14, 0x1c, 0x35, 0x3b, 0x46, 0x4a,
    0x24, 0x2c, 0x58, 0x45, 0x4b, 0x34, 0x3c, 0x03, 0x57, 0x59, 0x13, 0x1d, 0x56, 0x5a, 0x23, 0x2d,
    0x44, 0x4c, 0x55, 0x5b, 0x33, 0x3d, 0x68, 0x02, 0x67, 0x69, 0x12, 0x1e, 0x66, 0x6a, 0x22, 0x2e,
    0x54, 0x5c, 0x43, 0x4d, 0x65, 0x6b, 0x32, 0x3e, 0x78, 0x01, 0x77, 0x79, 0x53, 0x5d, 0x11, 0x1f,
    0x64, 0x6c, 0x42, 0x4e, 0x76, 0x7a, 0x21, 0x2f, 0x75, 0x7b, 0x31, 0x3f, 0x63, 0x6d, 0x52, 0x5e,
    0x00, 0x74, 0x7c, 0x41, 0x4f, 0x10, 0x20, 0x62, 0x6e, 0x30, 0x73, 0x7d, 0x51, 0x5f, 0x40, 0x72,
    0x7e, 0x61, 0x6f, 0x50, 0x71, 0x7f, 0x60, 0x70,
];

pub(crate) fn plane_code_to_distance(width: usize, plane_code: u32) -> usize {
    if plane_code > CODE_TO_PLANE_CODES as u32 {
        return (plane_code - CODE_TO_PLANE_CODES as u32) as usize;
    }
    let dist_code = u32::from(CODE_TO_PLANE[plane_code.saturating_sub(1) as usize]);
    let yoffset = (dist_code >> 4) as i32;
    let xoffset = 8 - (dist_code & 0xf) as i32;
    let distance = yoffset * width as i32 + xoffset;
    distance.max(1) as usize
}

/// Decodes one entropy-coded image (transform/entropy/palette sub-image).
pub(crate) fn decode_entropy_image(
    reader: &mut BitReader<'_>,
    width: usize,
    height: usize,
    limits: &PngLimits,
) -> Result<Vec<u32>, ImageError> {
    check_dimensions(width, height, limits)?;
    let mut cache = read_color_cache(reader)?;
    let groups = Groups::read(reader, cache_size_bits(&cache), 1)?;
    decode_pixels(reader, width, height, &groups, None, 0, &mut cache, limits)
}

pub(crate) fn cache_size_bits(cache: &Option<ColorCache>) -> u32 {
    cache.as_ref().map_or(0, |cache| 32 - cache.shift)
}

pub(crate) fn check_dimensions(
    width: usize,
    height: usize,
    limits: &PngLimits,
) -> Result<usize, ImageError> {
    if width == 0 || height == 0 {
        return Err(ImageError::InvalidData {
            reason: "VP8L zero image dimension",
        });
    }
    if width > limits.max_dimension as usize || height > limits.max_dimension as usize {
        return Err(ImageError::LimitExceeded {
            what: "WebP dimension",
            limit: u64::from(limits.max_dimension),
            actual: width.max(height) as u64,
        });
    }
    let pixels = (width as u64)
        .checked_mul(height as u64)
        .ok_or(ImageError::LimitExceeded {
            what: "WebP pixel count",
            limit: limits.max_pixels,
            actual: u64::MAX,
        })?;
    if pixels > limits.max_pixels {
        return Err(ImageError::LimitExceeded {
            what: "WebP pixel count",
            limit: limits.max_pixels,
            actual: pixels,
        });
    }
    usize::try_from(pixels).map_err(|_| ImageError::LimitExceeded {
        what: "WebP pixel count",
        limit: limits.max_pixels,
        actual: pixels,
    })
}
