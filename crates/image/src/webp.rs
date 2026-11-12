//! WebP decoder (VP8L lossless and VP8 lossy) — `std` only, no `unsafe`.
//!
//! Supports the full VP8L lossless bitstream used by libwebp/PIL photos, plus the
//! VP8 keyframe subset that WebP lossy files carry (internal `vp8_decoder` module).
//!
//! VP8L:
//!
//! - RIFF/WEBP container with a `VP8L` chunk;
//! - VP8L header (signature, 14-bit width/height, alpha-used flag, version 0);
//! - **all four transforms** and their inverses, applied in reverse order:
//!   predictor (14 modes, per-block modes), colour transform, subtract-green and
//!   colour indexing (palette + pixel bundling);
//! - **normal (canonical) Huffman codes** with code-length codes and repeat
//!   symbols, **simple codes** and **meta Huffman images** (code groups);
//! - **LZ77 backward references** with overlapping copies and the 120-entry
//!   plane-distance mapping, plus the **colour cache** (hash entries);
//! - typed errors for corrupt/truncated input, never a panic; dimensions, pixel
//!   counts, palette sizes, code-group counts and cache sizes are capped before
//!   allocating.
//!
//! VP8 lossy (`VP8 ` chunk) is decoded by the internal `vp8_decoder` module (boolean entropy
//! decoder, segmentation, loop filter, token partitions, intra prediction, IDCT and
//! YUV420→RGB); `image::decode` returns its pixels and corrupt payloads keep typed
//! errors. Animation/alpha-only WebP variants (VP8X) are out of scope.

use crate::RgbaImage;
use crate::error::ImageError;
use crate::png::PngLimits;

/// VP8L chunk signature byte.
const VP8L_SIGNATURE: u8 = 0x2f;
/// `RIFF` header + `WEBP` form type.
const RIFF_HEADER_LEN: usize = 12;
/// Hard caps (format + defensive allocation bounds).
const MAX_TRANSFORMS: usize = 4;
const MAX_HUFFMAN_GROUPS: usize = 256;
const MAX_HUFFMAN_CODE_LEN: u8 = 15;
const MAX_CACHE_BITS: u32 = 11;
const NUM_LENGTH_CODES: u16 = 24;
const NUM_LITERAL_CODES: u16 = 256;
const NUM_DISTANCE_CODES: u16 = 40;
const CODE_TO_PLANE_CODES: usize = 120;

/// LSB-first bit reader for the VP8L bitstream.
struct BitReader<'a> {
    data: &'a [u8],
    bit: usize,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, bit: 0 }
    }

    fn read_bit(&mut self) -> Result<u32, ImageError> {
        let byte = *self.data.get(self.bit / 8).ok_or(ImageError::Truncated {
            context: "WebP VP8L bitstream",
        })?;
        let value = u32::from((byte >> (self.bit % 8)) & 1);
        self.bit += 1;
        Ok(value)
    }

    fn read_bits(&mut self, count: u32) -> Result<u32, ImageError> {
        if count > 24 {
            return Err(ImageError::InvalidData {
                reason: "VP8L bit read larger than 24 bits",
            });
        }
        let mut value = 0u32;
        for shift in 0..count {
            value |= self.read_bit()? << shift;
        }
        Ok(value)
    }
}

/// A canonical prefix code. `single` decodes without consuming bits; `codes`
/// maps `(bit_length, canonical_code)` to the symbol.
struct Huffman {
    single: Option<u16>,
    /// Indexed by code length - 1; values are `(canonical_code, symbol)`.
    codes: [Vec<(u32, u16)>; 16],
}

impl Huffman {
    fn from_lengths(lengths: &[u8]) -> Result<Self, ImageError> {
        let mut count = [0u32; 16];
        for &length in lengths {
            let length = usize::from(length);
            if length > usize::from(MAX_HUFFMAN_CODE_LEN) {
                return Err(ImageError::InvalidData {
                    reason: "VP8L Huffman code length exceeds 15",
                });
            }
            count[length] += 1;
        }
        let symbols = count[1..].iter().sum::<u32>();
        if symbols == 0 {
            return Ok(Self {
                single: Some(0),
                codes: Default::default(),
            });
        }
        if symbols == 1 {
            let symbol = lengths.iter().position(|&length| length != 0).unwrap_or(0) as u16;
            return Ok(Self {
                single: Some(symbol),
                codes: Default::default(),
            });
        }
        // DEFLATE/VP8L canonical codes: compute the first code per length, then
        // assign in symbol order. `count[0]` (unused lengths) does not participate.
        let mut next_code = [0u32; 16];
        let mut code = 0u32;
        for length in 1..=15usize {
            code = (code + if length > 1 { count[length - 1] } else { 0 }) << 1;
            next_code[length] = code;
        }
        let mut codes: [Vec<(u32, u16)>; 16] = Default::default();
        for (symbol, &symbol_length) in lengths.iter().enumerate() {
            let length = usize::from(symbol_length);
            if length == 0 {
                continue;
            }
            codes[length].push((
                next_code[length],
                u16::try_from(symbol).map_err(|_| ImageError::InvalidData {
                    reason: "VP8L Huffman symbol out of range",
                })?,
            ));
            next_code[length] += 1;
        }
        Ok(Self {
            single: None,
            codes,
        })
    }

    fn decode(&self, reader: &mut BitReader<'_>) -> Result<u16, ImageError> {
        if let Some(symbol) = self.single {
            return Ok(symbol);
        }
        let mut code = 0u32;
        for length in 1..=15u8 {
            code = (code << 1) | reader.read_bit()?;
            for &(candidate, symbol) in &self.codes[usize::from(length)] {
                if candidate == code {
                    return Ok(symbol);
                }
            }
        }
        Err(ImageError::InvalidData {
            reason: "invalid VP8L Huffman code",
        })
    }
}

fn read_huffman_code(
    reader: &mut BitReader<'_>,
    alphabet_size: u16,
) -> Result<Huffman, ImageError> {
    let simple = reader.read_bit()? == 1;
    if simple {
        let num_symbols = reader.read_bit()? + 1;
        let first_len_code = reader.read_bit()?;
        let first = reader.read_bits(if first_len_code == 0 { 1 } else { 8 })?;
        if first >= u32::from(alphabet_size) {
            return Err(ImageError::InvalidData {
                reason: "VP8L simple Huffman symbol outside the alphabet",
            });
        }
        let mut lengths = vec![0u8; usize::from(alphabet_size)];
        lengths[first as usize] = 1;
        if num_symbols == 2 {
            let second = reader.read_bits(8)?;
            if second >= u32::from(alphabet_size) {
                return Err(ImageError::InvalidData {
                    reason: "VP8L simple Huffman symbol outside the alphabet",
                });
            }
            lengths[second as usize] = 1;
        }
        return Huffman::from_lengths(&lengths);
    }

    let num_code_lengths = 4 + reader.read_bits(4)? as usize;
    if num_code_lengths > 19 {
        return Err(ImageError::InvalidData {
            reason: "VP8L too many code-length code lengths",
        });
    }
    const ORDER: [usize; 19] = [
        17, 18, 0, 1, 2, 3, 4, 5, 16, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
    ];
    let mut code_length_code_lengths = [0u8; 19];
    for &slot in ORDER.iter().take(num_code_lengths) {
        code_length_code_lengths[slot] = reader.read_bits(3)? as u8;
    }
    let code_length_code = Huffman::from_lengths(&code_length_code_lengths)?;

    let (max_symbol, alphabet_size) = if reader.read_bit()? == 1 {
        let length_nbits = 2 + 2 * reader.read_bits(3)?;
        let max_symbol = 2 + reader.read_bits(length_nbits)?;
        (max_symbol, alphabet_size)
    } else {
        (u32::from(alphabet_size), alphabet_size)
    };
    if max_symbol > u32::from(alphabet_size) {
        return Err(ImageError::InvalidData {
            reason: "VP8L Huffman max_symbol exceeds the alphabet",
        });
    }
    let mut lengths = vec![0u8; usize::from(alphabet_size)];
    let mut symbol = 0usize;
    let mut previous = 8u8;
    while symbol < usize::try_from(max_symbol).unwrap_or(usize::MAX) {
        let code = code_length_code.decode(reader)?;
        match code {
            0..=15 => {
                lengths[symbol] = code as u8;
                if code != 0 {
                    previous = code as u8;
                }
                symbol += 1;
            }
            16 => {
                let repeat = 3 + reader.read_bits(2)? as usize;
                if symbol + repeat > lengths.len() {
                    return Err(ImageError::InvalidData {
                        reason: "VP8L Huffman repeat exceeds the alphabet",
                    });
                }
                for _ in 0..repeat {
                    lengths[symbol] = previous;
                    symbol += 1;
                }
            }
            17 => {
                let repeat = 3 + reader.read_bits(3)? as usize;
                if symbol + repeat > lengths.len() {
                    return Err(ImageError::InvalidData {
                        reason: "VP8L Huffman zero repeat exceeds the alphabet",
                    });
                }
                symbol += repeat;
            }
            18 => {
                let repeat = 11 + reader.read_bits(7)? as usize;
                if symbol + repeat > lengths.len() {
                    return Err(ImageError::InvalidData {
                        reason: "VP8L Huffman long zero repeat exceeds the alphabet",
                    });
                }
                symbol += repeat;
            }
            _ => {
                return Err(ImageError::InvalidData {
                    reason: "VP8L Huffman code-length symbol out of range",
                });
            }
        }
    }
    Huffman::from_lengths(&lengths)
}

struct Groups {
    codes: Vec<[Huffman; 5]>,
}

impl Groups {
    fn read(reader: &mut BitReader<'_>, cache_bits: u32, count: usize) -> Result<Self, ImageError> {
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

    fn get(&self, index: usize) -> Result<&[Huffman; 5], ImageError> {
        self.codes.get(index).ok_or(ImageError::InvalidData {
            reason: "VP8L meta prefix index out of range",
        })
    }
}

/// Color cache (hash table) used by VP8L literals/backward references.
struct ColorCache {
    colors: Vec<u32>,
    shift: u32,
}

impl ColorCache {
    fn new(bits: u32) -> Self {
        Self {
            colors: vec![0; 1usize << bits],
            shift: 32 - bits,
        }
    }

    fn insert(&mut self, argb: u32) {
        let key = ((argb.wrapping_mul(0x1e35_a7bd)) >> self.shift) as usize;
        if let Some(slot) = self.colors.get_mut(key) {
            *slot = argb;
        }
    }

    fn lookup(&self, index: usize) -> Result<u32, ImageError> {
        self.colors
            .get(index)
            .copied()
            .ok_or(ImageError::InvalidData {
                reason: "VP8L colour-cache index out of range",
            })
    }
}

fn read_color_cache(reader: &mut BitReader<'_>) -> Result<Option<ColorCache>, ImageError> {
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

fn get_copy_value(prefix_code: u16, reader: &mut BitReader<'_>) -> Result<u32, ImageError> {
    let code = u32::from(prefix_code);
    if code < 4 {
        return Ok(code + 1);
    }
    let extra_bits = (code - 2) >> 1;
    let offset = (2 + (code & 1)) << extra_bits;
    Ok(offset + reader.read_bits(extra_bits)? + 1)
}

const CODE_TO_PLANE: [u8; CODE_TO_PLANE_CODES] = [
    0x18, 0x07, 0x17, 0x19, 0x28, 0x06, 0x27, 0x29, 0x16, 0x1a, 0x26, 0x2a, 0x38, 0x05, 0x37, 0x39,
    0x15, 0x1b, 0x36, 0x3a, 0x25, 0x2b, 0x48, 0x04, 0x47, 0x49, 0x14, 0x1c, 0x35, 0x3b, 0x46, 0x4a,
    0x24, 0x2c, 0x58, 0x45, 0x4b, 0x34, 0x3c, 0x03, 0x57, 0x59, 0x13, 0x1d, 0x56, 0x5a, 0x23, 0x2d,
    0x44, 0x4c, 0x55, 0x5b, 0x33, 0x3d, 0x68, 0x02, 0x67, 0x69, 0x12, 0x1e, 0x66, 0x6a, 0x22, 0x2e,
    0x54, 0x5c, 0x43, 0x4d, 0x65, 0x6b, 0x32, 0x3e, 0x78, 0x01, 0x77, 0x79, 0x53, 0x5d, 0x11, 0x1f,
    0x64, 0x6c, 0x42, 0x4e, 0x76, 0x7a, 0x21, 0x2f, 0x75, 0x7b, 0x31, 0x3f, 0x63, 0x6d, 0x52, 0x5e,
    0x00, 0x74, 0x7c, 0x41, 0x4f, 0x10, 0x20, 0x62, 0x6e, 0x30, 0x73, 0x7d, 0x51, 0x5f, 0x40, 0x72,
    0x7e, 0x61, 0x6f, 0x50, 0x71, 0x7f, 0x60, 0x70,
];

fn plane_code_to_distance(width: usize, plane_code: u32) -> usize {
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
fn decode_entropy_image(
    reader: &mut BitReader<'_>,
    width: usize,
    height: usize,
    limits: &PngLimits,
) -> Result<Vec<u32>, ImageError> {
    check_dimensions(width, height, limits)?;
    let mut cache = read_color_cache(reader)?;
    let groups = Groups::read(reader, cache_size_bits(&cache), 1)?;
    decode_pixels(reader, width, height, &groups, None, 0, &mut cache, None)
}

fn cache_size_bits(cache: &Option<ColorCache>) -> u32 {
    cache.as_ref().map_or(0, |cache| 32 - cache.shift)
}

fn check_dimensions(width: usize, height: usize, limits: &PngLimits) -> Result<usize, ImageError> {
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

#[allow(clippy::too_many_arguments)]
fn decode_pixels(
    reader: &mut BitReader<'_>,
    width: usize,
    height: usize,
    groups: &Groups,
    huffman_image: Option<&MetaImage>,
    prefix_bits: u32,
    cache: &mut Option<ColorCache>,
    _limit: Option<usize>,
) -> Result<Vec<u32>, ImageError> {
    let limits = limit_limits(None);
    let pixels = check_dimensions(width, height, limits)?;
    let mut data: Vec<u32> = Vec::with_capacity(pixels);
    while data.len() < pixels {
        let group_index = match huffman_image {
            Some(image) => {
                let x = data.len() % width;
                let y = data.len() / width;
                image.group(x, y, prefix_bits, groups.codes.len())?
            }
            None => 0,
        };
        let group = groups.get(group_index)?;
        let green_symbol = group[0].decode(reader)?;
        if green_symbol < NUM_LITERAL_CODES {
            let red = group[1].decode(reader)? as u8;
            let blue = group[2].decode(reader)? as u8;
            let alpha = group[3].decode(reader)? as u8;
            let argb = (u32::from(alpha) << 24)
                | (u32::from(red) << 16)
                | (u32::from(green_symbol as u8) << 8)
                | u32::from(blue);
            data.push(argb);
            if let Some(cache) = cache.as_mut() {
                cache.insert(argb);
            }
        } else if green_symbol < NUM_LITERAL_CODES + NUM_LENGTH_CODES {
            let length = get_copy_value(green_symbol - NUM_LITERAL_CODES, reader)? as usize;
            if length == 0 {
                return Err(ImageError::InvalidData {
                    reason: "VP8L backward reference with zero length",
                });
            }
            let distance_symbol = group[4].decode(reader)?;
            let distance_code = get_copy_value(distance_symbol, reader)?;
            let distance = plane_code_to_distance(width, distance_code);
            if distance > data.len() {
                return Err(ImageError::InvalidData {
                    reason: "VP8L backward reference before the start of the image",
                });
            }
            if length > pixels - data.len() {
                return Err(ImageError::InvalidData {
                    reason: "VP8L backward reference exceeds the image",
                });
            }
            for _ in 0..length {
                let value = data[data.len() - distance];
                data.push(value);
                if let Some(cache) = cache.as_mut() {
                    cache.insert(value);
                }
            }
        } else {
            let index = usize::from(green_symbol - NUM_LITERAL_CODES - NUM_LENGTH_CODES);
            let value = match cache.as_ref() {
                Some(cache) => cache.lookup(index)?,
                None => {
                    return Err(ImageError::InvalidData {
                        reason: "VP8L colour-cache code without a cache",
                    });
                }
            };
            data.push(value);
            if let Some(cache) = cache.as_mut() {
                cache.insert(value);
            }
        }
    }
    Ok(data)
}

fn limit_limits(_limit: Option<usize>) -> &'static PngLimits {
    // All sub-images are bounded by the main image pixel cap; this helper keeps the
    // signature explicit for future per-role limits.
    static LIMITS: std::sync::OnceLock<PngLimits> = std::sync::OnceLock::new();
    LIMITS.get_or_init(PngLimits::default)
}

/// Meta Huffman image: groups a `VP8L` block to its prefix-code group.
struct MetaImage {
    width: usize,
    groups: Vec<u16>,
}

impl MetaImage {
    fn group(
        &self,
        x: usize,
        y: usize,
        prefix_bits: u32,
        count: usize,
    ) -> Result<usize, ImageError> {
        let index = (y >> prefix_bits) * self.width + (x >> prefix_bits);
        let value = self
            .groups
            .get(index)
            .copied()
            .ok_or(ImageError::InvalidData {
                reason: "VP8L meta Huffman image index out of range",
            })?;
        let value = usize::from(value);
        if value >= count {
            return Err(ImageError::InvalidData {
                reason: "VP8L meta Huffman code group out of range",
            });
        }
        Ok(value)
    }
}

fn decode_main_pixels(
    reader: &mut BitReader<'_>,
    width: usize,
    height: usize,
    limits: &PngLimits,
) -> Result<Vec<u32>, ImageError> {
    let mut cache = read_color_cache(reader)?;
    let has_meta = reader.read_bit()? == 1;
    let mut huffman_image = None;
    let mut prefix_bits = 0u32;
    let group_count;
    if has_meta {
        prefix_bits = reader.read_bits(3)? + 2;
        let meta_width = width.div_ceil(1usize << prefix_bits);
        let meta_height = height.div_ceil(1usize << prefix_bits);
        let meta_pixels = decode_entropy_image(reader, meta_width, meta_height, limits)?;
        let mut max_group = 0usize;
        let mut groups = Vec::with_capacity(meta_pixels.len());
        for pixel in &meta_pixels {
            let group = ((pixel >> 8) & 0xffff) as usize;
            max_group = max_group.max(group);
            groups.push(group as u16);
        }
        group_count = max_group + 1;
        huffman_image = Some(MetaImage {
            width: meta_width,
            groups,
        });
    } else {
        group_count = 1;
    }
    let groups = Groups::read(reader, cache_size_bits(&cache), group_count)?;
    decode_pixels(
        reader,
        width,
        height,
        &groups,
        huffman_image.as_ref(),
        prefix_bits,
        &mut cache,
        None,
    )
}

// ---------------------------------------------------------------------------
// Transforms (inverse application)
// ---------------------------------------------------------------------------

enum Transform {
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

fn avg2(a: u32, b: u32) -> u32 {
    let mut result = 0u32;
    for shift in (0..32).step_by(8) {
        let ca = (a >> shift) & 0xff;
        let cb = (b >> shift) & 0xff;
        result |= ((ca + cb) >> 1) << shift;
    }
    result
}

fn add_pixels(a: u32, b: u32) -> u32 {
    let mut result = 0u32;
    for shift in (0..32).step_by(8) {
        let ca = (a >> shift) & 0xff;
        let cb = (b >> shift) & 0xff;
        result |= ((ca + cb) & 0xff) << shift;
    }
    result
}

fn clip255(value: i32) -> u32 {
    value.clamp(0, 255) as u32
}

fn clamped_add_subtract_full(a: u32, b: u32, c: u32) -> u32 {
    let mut result = 0u32;
    for shift in (0..32).step_by(8) {
        let ca = ((a >> shift) & 0xff) as i32;
        let cb = ((b >> shift) & 0xff) as i32;
        let cc = ((c >> shift) & 0xff) as i32;
        result |= clip255(ca + cb - cc) << shift;
    }
    result
}

fn clamped_add_subtract_half(a: u32, b: u32) -> u32 {
    let average = avg2(a, b);
    let mut result = 0u32;
    for shift in (0..32).step_by(8) {
        let ca = ((average >> shift) & 0xff) as i32;
        let cb = ((b >> shift) & 0xff) as i32;
        result |= clip255(ca + (ca - cb) / 2) << shift;
    }
    result
}

fn select(a: u32, b: u32, c: u32) -> u32 {
    let mut sum = 0i32;
    for shift in (0..32).step_by(8) {
        let ca = ((a >> shift) & 0xff) as i32;
        let cb = ((b >> shift) & 0xff) as i32;
        let cc = ((c >> shift) & 0xff) as i32;
        sum += (cb - cc).abs() - (ca - cc).abs();
    }
    if sum <= 0 { a } else { b }
}

fn predictor_mode(mode: u32, left: u32, top: u32, top_left: u32, top_right: u32) -> u32 {
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
    fn apply(self, pixels: Vec<u32>) -> Result<Vec<u32>, ImageError> {
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
                            let left = out[y * width + x - 1];
                            let top = out[(y - 1) * width + x];
                            let top_left = out[(y - 1) * width + x - 1];
                            let top_right = if x + 1 < width {
                                out[(y - 1) * width + x + 1]
                            } else {
                                out[y * width]
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

fn decode_transform(
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

/// Locates a chunk `id` inside a RIFF/WEBP container and returns its payload.
fn find_chunk<'a>(bytes: &'a [u8], id: &[u8; 4]) -> Result<&'a [u8], ImageError> {
    if bytes.len() < RIFF_HEADER_LEN || !bytes.starts_with(b"RIFF") || &bytes[8..12] != b"WEBP" {
        return Err(ImageError::InvalidSignature);
    }
    let mut offset = RIFF_HEADER_LEN;
    while offset + 8 <= bytes.len() {
        let chunk_id = &bytes[offset..offset + 4];
        let length = u32::from_le_bytes([
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]) as usize;
        let payload_start = offset + 8;
        let payload_end = payload_start
            .checked_add(length)
            .ok_or(ImageError::Truncated {
                context: "WebP RIFF chunk",
            })?;
        if payload_end > bytes.len() {
            return Err(ImageError::Truncated {
                context: "WebP RIFF chunk payload",
            });
        }
        if chunk_id == id {
            return Ok(&bytes[payload_start..payload_end]);
        }
        // Chunks are padded to an even size.
        offset = payload_end + (length & 1);
    }
    Err(ImageError::InvalidData {
        reason: "WebP container has no matching chunk",
    })
}

/// Locates the `VP8L` chunk (lossless) and returns its payload.
fn find_vp8l_chunk(bytes: &[u8]) -> Result<&[u8], ImageError> {
    find_chunk(bytes, b"VP8L")
}

/// Locates the `VP8 ` chunk (lossy keyframe) and returns its payload.
pub(crate) fn find_vp8_chunk(bytes: &[u8]) -> Result<&[u8], ImageError> {
    find_chunk(bytes, b"VP8 ")
}

/// Decodes a VP8L (lossless) or VP8 (lossy keyframe) WebP image into RGBA8.
pub(crate) fn decode_webp(bytes: &[u8], limits: &PngLimits) -> Result<RgbaImage, ImageError> {
    // The extended container (VP8X: canvas, alpha, animation) is out of scope: decoding
    // only the inner VP8/VP8L chunk would silently drop alpha/offsets, so reject it with
    // a typed error instead of returning wrong pixels.
    if find_chunk(bytes, b"VP8X").is_ok() {
        return Err(ImageError::UnsupportedFeature {
            feature: "WebP extended container (VP8X alpha/animation)",
        });
    }
    // WebP lossy (`VP8 `) is a single VP8 keyframe: decode it instead of trying VP8L.
    if let Ok(payload) = find_vp8_chunk(bytes) {
        return crate::vp8_decoder::decode_keyframe(payload, limits);
    }
    let payload = find_vp8l_chunk(bytes)?;
    let Some((&signature, bitstream)) = payload.split_first() else {
        return Err(ImageError::Truncated {
            context: "WebP VP8L payload",
        });
    };
    if signature != VP8L_SIGNATURE {
        return Err(ImageError::InvalidData {
            reason: "invalid VP8L signature",
        });
    }
    let mut reader = BitReader::new(bitstream);
    let orig_width = reader.read_bits(14)? as usize + 1;
    let height = reader.read_bits(14)? as usize + 1;
    let _alpha_used = reader.read_bit()?;
    let version = reader.read_bits(3)?;
    if version != 0 {
        return Err(ImageError::UnsupportedFeature {
            feature: "unsupported WebP VP8L version",
        });
    }
    check_dimensions(orig_width, height, limits)?;

    let mut width = orig_width;
    let mut transforms = Vec::new();
    while reader.read_bit()? == 1 {
        if transforms.len() >= MAX_TRANSFORMS {
            return Err(ImageError::InvalidData {
                reason: "VP8L too many transforms",
            });
        }
        transforms.push(decode_transform(&mut reader, limits, &mut width, height)?);
    }
    let mut pixels = decode_main_pixels(&mut reader, width, height, limits)?;
    for transform in transforms.into_iter().rev() {
        pixels = transform.apply(pixels)?;
    }
    if pixels.len() != orig_width * height {
        return Err(ImageError::InvalidData {
            reason: "VP8L decoded pixel count mismatch",
        });
    }
    let mut data = Vec::with_capacity(pixels.len() * 4);
    for argb in pixels {
        data.push((argb >> 16) as u8);
        data.push((argb >> 8) as u8);
        data.push(argb as u8);
        data.push((argb >> 24) as u8);
    }
    RgbaImage::new(orig_width as u32, height as u32, data)
}

#[cfg(test)]
mod palette_tests {
    use super::*;

    #[test]
    fn out_of_range_palette_indices_are_rejected() {
        let palette = vec![0xFF00_00FFu32];
        let valid = Transform::Indexing {
            width: 1,
            height: 1,
            bits: 0,
            palette: palette.clone(),
        };
        assert_eq!(valid.apply(vec![0]).expect("index 0"), vec![0xFF00_00FF]);
        let invalid = Transform::Indexing {
            width: 1,
            height: 1,
            bits: 0,
            palette,
        };
        match invalid.apply(vec![1 << 8]) {
            Err(ImageError::InvalidData { reason }) => {
                assert!(reason.contains("palette index out of range"), "{reason}");
            }
            other => panic!("expected InvalidData, got {other:?}"),
        }
    }
}
