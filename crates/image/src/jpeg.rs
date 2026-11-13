//! Baseline JPEG (ITU-T T.81 / JFIF) decoder — `std` only, no `unsafe`.
//!
//! ## Supported subset
//!
//! - **Baseline sequential DCT** (`SOF0`) and **progressive DCT (`SOF2`)**:
//!   DC-first/DC-refinement, AC-first and AC-refinement scans; spectral selection
//!   and successive approximation are decoded into coefficient buffers and IDCT'd
//!   once at EOI. Multi-component progressive images are supported up to their
//!   first/refinement scan split: colour refinement scans (`Ah > 0` with more than
//!   one component) return a clear [`ImageError::UnsupportedFeature`]; grayscale
//!   progressive (including refinement) and colour DC/AC-first scans decode fully;
//! - Huffman entropy coding (`DHT`/`SOS`);
//! - quantization tables (`DQT`, 8- and 16-bit precision), 1-4 components;
//! - sampling factors **4:4:4, 4:2:2 and 4:2:0** (any `H/V` 1-4 within the MCU limits);
//! - restart intervals (`DRI` + `RSTn` markers, DC predictor reset);
//! - grayscale (`1` component) and `YCbCr` (3 components); Adobe `APP14` transform `0`
//!   (RGB) is honoured, `CMYK`/`YCCK` are rejected with a clear error;
//! - nearest-neighbour chroma upsampling (4:4:4 is exact; 4:2:2/4:2:0 are documented as
//!   blocky chroma — acceptable for photographs at the engine's current fidelity);
//! - EXIF `APP1`/JFIF `APP0` are skipped by the JPEG decoder itself; **EXIF orientation**
//!   (`IFD0` tag `0x0112`) is parsed by [`crate::exif`] and applied by [`crate::decode`]
//!   after decoding (all eight orientations).
//!
//! ## Rejected with a clear error (never a panic)
//!
//! - progressive **refinement scans for multi-component** images (subset documented
//!   above), arithmetic/lossless/hierarchical DCT modes;
//! - multiple scans: only the first `SOS` scan is decoded (baseline files encode
//!   everything in one interleaved scan; extra scans are ignored, documented);
//! - `CMYK`/`YCCK` (4-component Adobe) and 12-bit sample precision.
//!
//! Malformed/truncated streams return a typed [`ImageError`]; dimensions are checked against
//! [`PngLimits`] before any pixel allocation, so a hostile header cannot allocate unbounded
//! memory. Missing/extra entropy bits are tolerated only where the spec allows padding.

use crate::RgbaImage;
use crate::error::ImageError;
use crate::png::PngLimits;

/// Maximum components accepted (Y, Cb, Cr, K).
const MAX_COMPONENTS: usize = 4;
/// Maximum Huffman code length (T.81 Table B.5).
const MAX_HUFFMAN_LEN: usize = 16;
/// Number of coefficients in an 8x8 DCT block.
const BLOCK_LEN: usize = 64;

/// Zig-zag index -> natural (row-major) index for coefficient dequantization.
const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

/// Cosine basis table `COS[u][x] = cos((2x+1) u pi / 16)` scaled by `1/sqrt(2)` for `u=0`.
fn cosine_table() -> [[f32; 8]; 8] {
    let mut table = [[0.0f32; 8]; 8];
    for (u, row) in table.iter_mut().enumerate() {
        let scale = if u == 0 {
            std::f32::consts::FRAC_1_SQRT_2
        } else {
            1.0
        };
        for (x, value) in row.iter_mut().enumerate() {
            let angle = ((2 * x + 1) as f32) * (u as f32) * std::f32::consts::PI / 16.0;
            *value = scale * angle.cos() / 2.0;
        }
    }
    table
}

/// Quantization table (`DQT`).
#[derive(Clone)]
struct QuantTable {
    values: [u16; BLOCK_LEN],
}

/// Canonical Huffman table (`DHT`) in the T.81 F.2.2.3 decoding form.
#[derive(Clone)]
struct HuffmanTable {
    min_code: [u16; MAX_HUFFMAN_LEN + 1],
    max_code: [i32; MAX_HUFFMAN_LEN + 1],
    value_index: [usize; MAX_HUFFMAN_LEN + 1],
    values: Vec<u8>,
}

impl HuffmanTable {
    fn from_raw(counts: &[u8; 16], values: Vec<u8>) -> Result<Self, ImageError> {
        let mut min_code = [0u16; MAX_HUFFMAN_LEN + 1];
        let mut max_code = [-1i32; MAX_HUFFMAN_LEN + 1];
        let mut value_index = [0usize; MAX_HUFFMAN_LEN + 1];
        // T.81 F.2.2.3: canonical code assignment. `code` is the first code of the current
        // length; after each length it shifts left for the next (shorter/higher) length.
        let mut code: i32 = 0;
        let mut index = 0usize;
        for length in 1..=MAX_HUFFMAN_LEN {
            let count = i32::from(counts[length - 1]);
            min_code[length] = code as u16;
            value_index[length] = index;
            if count == 0 {
                max_code[length] = -1;
            } else {
                if code + count > (1i32 << MAX_HUFFMAN_LEN) {
                    return Err(ImageError::InvalidData {
                        reason: "JPEG Huffman table is over-subscribed",
                    });
                }
                max_code[length] = code + count - 1;
                code += count;
                index += count as usize;
            }
            code <<= 1;
            if code > (1i32 << (MAX_HUFFMAN_LEN + 1)) {
                return Err(ImageError::InvalidData {
                    reason: "JPEG Huffman table is over-subscribed",
                });
            }
        }
        if index != values.len() {
            return Err(ImageError::InvalidData {
                reason: "JPEG Huffman table has fewer values than its counts",
            });
        }
        Ok(Self {
            min_code,
            max_code,
            value_index,
            values,
        })
    }
}

/// MSB-first bit reader over an entropy-coded segment with JPEG byte stuffing.
struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    buffer: u32,
    bits: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8], pos: usize) -> Self {
        Self {
            data,
            pos,
            buffer: 0,
            bits: 0,
        }
    }

    /// Reads one entropy byte (removing `0xFF 0x00` stuffing) into the bit buffer.
    fn fill_byte(&mut self) -> Result<(), ImageError> {
        let byte = *self.data.get(self.pos).ok_or(ImageError::Truncated {
            context: "entropy-coded JPEG segment",
        })?;
        self.pos += 1;
        let byte = if byte == 0xFF {
            let next = *self.data.get(self.pos).ok_or(ImageError::Truncated {
                context: "JPEG marker after 0xFF",
            })?;
            self.pos += 1;
            match next {
                0x00 => 0xFF,
                0xD0..=0xD7 => {
                    return Err(ImageError::InvalidData {
                        reason: "unexpected JPEG restart marker inside an MCU",
                    });
                }
                _ => {
                    return Err(ImageError::Truncated {
                        context: "JPEG scan ends before all MCUs are decoded",
                    });
                }
            }
        } else {
            byte
        };
        self.buffer = (self.buffer << 8) | u32::from(byte);
        self.bits += 8;
        Ok(())
    }

    fn get_bits(&mut self, count: u8) -> Result<u32, ImageError> {
        if count == 0 {
            return Ok(0);
        }
        while self.bits < count {
            self.fill_byte()?;
        }
        self.bits -= count;
        Ok((self.buffer >> self.bits) & ((1u32 << count) - 1))
    }

    /// Consumes a `RSTn` marker after aligning to a byte boundary.
    fn expect_restart(&mut self, expected: u8) -> Result<(), ImageError> {
        // Entropy-coded segments are padded to a byte boundary before each RST marker.
        self.bits = 0;
        self.buffer = 0;
        let byte = *self.data.get(self.pos).ok_or(ImageError::Truncated {
            context: "JPEG restart marker",
        })?;
        self.pos += 1;
        if byte != 0xFF {
            return Err(ImageError::InvalidData {
                reason: "JPEG restart marker missing",
            });
        }
        loop {
            let marker = *self.data.get(self.pos).ok_or(ImageError::Truncated {
                context: "JPEG restart marker",
            })?;
            self.pos += 1;
            match marker {
                0xFF => continue,
                0x00 => {
                    return Err(ImageError::InvalidData {
                        reason: "JPEG restart marker missing",
                    });
                }
                0xD0..=0xD7 => {
                    if marker & 0x0F != expected {
                        return Err(ImageError::InvalidData {
                            reason: "JPEG restart markers out of sequence",
                        });
                    }
                    return Ok(());
                }
                _ => {
                    return Err(ImageError::InvalidData {
                        reason: "JPEG restart marker missing",
                    });
                }
            }
        }
    }

    fn decode_huffman(&mut self, table: &HuffmanTable) -> Result<u8, ImageError> {
        let mut code: i32 = 0;
        for length in 1..=MAX_HUFFMAN_LEN {
            code = (code << 1) | self.get_bits(1)? as i32;
            let max = table.max_code[length];
            if max >= 0 && code <= max && code >= i32::from(table.min_code[length]) {
                let index =
                    table.value_index[length] + (code - i32::from(table.min_code[length])) as usize;
                return table
                    .values
                    .get(index)
                    .copied()
                    .ok_or(ImageError::InvalidData {
                        reason: "JPEG Huffman value index out of range",
                    });
            }
        }
        Err(ImageError::InvalidData {
            reason: "invalid JPEG Huffman code",
        })
    }
}

/// Receives the extended magnitude bits of a JPEG coefficient.
fn extend_coefficient(value: i32, bits: u32) -> i32 {
    if bits > 0 && value < (1 << (bits - 1)) {
        value - (1 << bits) + 1
    } else {
        value
    }
}

/// One frame component (`SOF0`).
#[derive(Clone)]
struct FrameComponent {
    id: u8,
    h: u32,
    v: u32,
    quant: usize,
    dc_table: usize,
    ac_table: usize,
    plane: Vec<u8>,
    plane_width: usize,
    plane_height: usize,
    blocks_x: usize,
    blocks_y: usize,
    /// Progressive JPEG coefficient accumulator (natural row-major order),
    /// `blocks_x * blocks_y` blocks; `None` for baseline decodes.
    coeffs: Option<Vec<[i32; BLOCK_LEN]>>,
    dc_pred: i32,
}

/// JPEG frame header data (SOF0 baseline or SOF2 progressive).
struct JpegFrame {
    width: u32,
    height: u32,
    components: Vec<FrameComponent>,
    max_h: usize,
    max_v: usize,
    progressive: bool,
}

/// Decodes a baseline JPEG stream into an [`RgbaImage`].
pub(crate) fn decode_jpeg(bytes: &[u8], limits: &PngLimits) -> Result<RgbaImage, ImageError> {
    decode_jpeg_with_scale(bytes, limits, 1)
}

/// Decodes a baseline JPEG with a reduced IDCT (scale 1/2/4/8) for `decode_scaled`.
///
/// `scale` is a power of two: 2 selects a 4x4 IDCT, 4 a 2x2 IDCT and 8 a 1x1
/// (DC-only) IDCT per 8x8 block; AC coefficients that cannot be represented in the
/// smaller block are discarded. Output dimensions are `ceil(width / scale) x
/// ceil(height / scale)`. Progressive JPEGs are handled by the caller (full decode +
/// post-subsample) and return a typed error here when `scale > 1`.
pub(crate) fn decode_jpeg_scaled(
    bytes: &[u8],
    limits: &PngLimits,
    max_dim: u32,
) -> Result<RgbaImage, ImageError> {
    let (width, height, progressive) = jpeg_dimensions(bytes, limits)?;
    let factor = width.max(height).div_ceil(max_dim.max(1)).max(1) as usize;
    let scale = factor.next_power_of_two().min(8);
    if scale <= 1 {
        return decode_jpeg_with_scale(bytes, limits, 1);
    }
    if progressive {
        return Err(ImageError::UnsupportedFeature {
            feature: "reduced-IDCT decode of progressive JPEG is not supported",
        });
    }
    decode_jpeg_with_scale(bytes, limits, scale)
}

/// Scans JPEG marker segments until the SOF header and returns its dimensions and
/// whether the stream is progressive (used to pick the reduced-IDCT scale before any
/// component planes are allocated).
pub(crate) fn jpeg_dimensions(
    bytes: &[u8],
    limits: &PngLimits,
) -> Result<(u32, u32, bool), ImageError> {
    if bytes.get(..2) != Some(&[0xFF, 0xD8]) {
        return Err(ImageError::InvalidSignature);
    }
    let mut index = 2usize;
    loop {
        let marker = next_marker(bytes, &mut index)?;
        match marker {
            0xC0 | 0xC2 => {
                let segment = read_segment(bytes, &mut index)?;
                if segment.len() < 6 {
                    return Err(ImageError::Truncated {
                        context: "JPEG SOF header",
                    });
                }
                let precision = segment[0];
                if precision != 8 {
                    return Err(ImageError::UnsupportedFeature {
                        feature: "JPEG sample precision other than 8 bits",
                    });
                }
                let height = u32::from(u16::from_be_bytes([segment[1], segment[2]]));
                let width = u32::from(u16::from_be_bytes([segment[3], segment[4]]));
                if width == 0 || height == 0 {
                    return Err(ImageError::InvalidDimensions { width, height });
                }
                if width > limits.max_dimension || height > limits.max_dimension {
                    return Err(ImageError::LimitExceeded {
                        what: "JPEG dimension",
                        limit: u64::from(limits.max_dimension),
                        actual: u64::from(width.max(height)),
                    });
                }
                return Ok((width, height, marker == 0xC2));
            }
            0xD9 => {
                return Err(ImageError::InvalidData {
                    reason: "JPEG ended before the frame header",
                });
            }
            0xD8 => {
                return Err(ImageError::InvalidData {
                    reason: "unexpected second JPEG SOI marker",
                });
            }
            _ => {
                let _ = read_segment(bytes, &mut index)?;
            }
        }
    }
}

fn decode_jpeg_with_scale(
    bytes: &[u8],
    limits: &PngLimits,
    scale: usize,
) -> Result<RgbaImage, ImageError> {
    let scale = scale.clamp(1, 8);
    let mut index = 0usize;
    if bytes.get(..2) != Some(&[0xFF, 0xD8]) {
        return Err(ImageError::InvalidSignature);
    }
    index += 2;

    let mut quant_tables: [Option<QuantTable>; 4] = [None, None, None, None];
    let mut dc_tables: [Option<HuffmanTable>; 4] = [None, None, None, None];
    let mut ac_tables: [Option<HuffmanTable>; 4] = [None, None, None, None];
    let mut frame: Option<JpegFrame> = None;
    let mut restart_interval = 0usize;
    let mut scan_count = 0usize;
    let mut adobe_transform: Option<u8> = None;

    loop {
        let marker = next_marker(bytes, &mut index)?;
        match marker {
            0xD8 => {
                return Err(ImageError::InvalidData {
                    reason: "unexpected second JPEG SOI marker",
                });
            }
            0xD9 => {
                if let Some(frame) = frame.take() {
                    if !frame.progressive {
                        return Err(ImageError::InvalidData {
                            reason: "JPEG ended before the baseline scan",
                        });
                    }
                    // Check that at least one scan was decoded.
                    if frame.components.iter().all(|c| c.coeffs.is_none()) {
                        return Err(ImageError::InvalidData {
                            reason: "progressive JPEG has no scans",
                        });
                    }
                    return render_progressive_components(frame, &quant_tables, adobe_transform);
                }
                break;
            }
            0xC0 | 0xC2 => {
                if frame.is_some() {
                    return Err(ImageError::InvalidData {
                        reason: "duplicate JPEG frame header",
                    });
                }
                let progressive = marker == 0xC2;
                frame = Some(parse_sof(bytes, &mut index, limits, progressive, scale)?);
            }
            0xC1 | 0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => {
                return Err(ImageError::UnsupportedFeature {
                    feature: "unsupported JPEG DCT mode (only baseline SOF0 is decoded)",
                });
            }
            0xC4 => parse_dht(bytes, &mut index, &mut dc_tables, &mut ac_tables)?,
            0xDB => parse_dqt(bytes, &mut index, &mut quant_tables)?,
            0xDD => {
                let segment = read_segment(bytes, &mut index)?;
                if segment.len() != 2 {
                    return Err(ImageError::InvalidData {
                        reason: "invalid JPEG DRI segment",
                    });
                }
                restart_interval = usize::from(u16::from_be_bytes([segment[0], segment[1]]));
            }
            0xEE => {
                let segment = read_segment(bytes, &mut index)?;
                if segment.starts_with(b"Adobe") && segment.len() >= 12 {
                    adobe_transform = segment.get(11).copied();
                }
            }
            0xE0..=0xEF | 0xFE => {
                // APPn / COM segments (JFIF, EXIF, ICC, comments) are skipped.
                read_segment(bytes, &mut index)?;
            }
            0xDA => {
                let Some(mut current) = frame.take() else {
                    return Err(ImageError::InvalidData {
                        reason: "JPEG scan before frame header",
                    });
                };
                let progressive = current.progressive;
                let scan = parse_sos(bytes, &mut index, &current.components, progressive)?;
                if !progressive {
                    let JpegFrame {
                        width,
                        height,
                        mut components,
                        max_h,
                        max_v,
                        ..
                    } = current;
                    decode_scan(
                        bytes,
                        &mut index,
                        width,
                        height,
                        &mut components,
                        max_h,
                        max_v,
                        &quant_tables,
                        &dc_tables,
                        &ac_tables,
                        &scan,
                        restart_interval,
                        scale,
                    )?;
                    let (out_width, out_height) = if scale <= 1 {
                        (width, height)
                    } else {
                        (width.div_ceil(scale as u32), height.div_ceil(scale as u32))
                    };
                    return render_components(
                        out_width,
                        out_height,
                        &components,
                        max_h,
                        max_v,
                        adobe_transform,
                    );
                }
                if scale > 1 {
                    return Err(ImageError::UnsupportedFeature {
                        feature: "reduced-IDCT decode of progressive JPEG is not supported",
                    });
                }
                scan_count += 1;
                if scan_count > 64 {
                    return Err(ImageError::LimitExceeded {
                        what: "progressive JPEG scan count",
                        limit: 64,
                        actual: scan_count as u64,
                    });
                }
                ensure_progressive_storage(&mut current);
                let mut reader = BitReader::new(bytes, index);
                decode_progressive_scan(
                    &mut reader,
                    &mut current,
                    &dc_tables,
                    &ac_tables,
                    &scan,
                    restart_interval,
                )?;
                index = skip_to_next_marker(bytes, reader.pos)?;
                frame = Some(current);
            }
            other => {
                return Err(ImageError::UnsupportedFeature {
                    feature: match other {
                        0xC8 => "JPEG reserved (JPG) marker",
                        0xF0..=0xFD => "JPEG arithmetic-coded segment",
                        _ => "unsupported JPEG marker",
                    },
                });
            }
        }
    }
    Err(ImageError::InvalidData {
        reason: "JPEG has no image scan (SOS)",
    })
}

/// Advances from the end of an entropy-coded segment to the next real marker.
///
/// Progressive scans can end before the last entropy byte is consumed (an
/// end-of-band run may cover the remaining blocks); the skipped bytes are
/// padding/stuffing of the current segment, never marker data.
fn skip_to_next_marker(bytes: &[u8], mut index: usize) -> Result<usize, ImageError> {
    while index < bytes.len() {
        if bytes[index] == 0xFF {
            let mut next = index + 1;
            while bytes.get(next) == Some(&0xFF) {
                next += 1;
            }
            match bytes.get(next) {
                Some(0x00) => {
                    // Stuffed data byte: skip and keep scanning.
                    index = next + 1;
                    continue;
                }
                Some(_) => return Ok(index),
                None => break,
            }
        }
        index += 1;
    }
    Err(ImageError::Truncated {
        context: "JPEG marker after progressive scan",
    })
}

/// Reads the next JPEG marker code (skipping `0xFF` fill bytes).
fn next_marker(bytes: &[u8], index: &mut usize) -> Result<u8, ImageError> {
    let byte = *bytes.get(*index).ok_or(ImageError::Truncated {
        context: "JPEG marker",
    })?;
    *index += 1;
    if byte != 0xFF {
        return Err(ImageError::InvalidData {
            reason: "expected JPEG marker prefix 0xFF",
        });
    }
    let mut marker = *bytes.get(*index).ok_or(ImageError::Truncated {
        context: "JPEG marker code",
    })?;
    *index += 1;
    while marker == 0xFF {
        marker = *bytes.get(*index).ok_or(ImageError::Truncated {
            context: "JPEG marker code",
        })?;
        *index += 1;
    }
    Ok(marker)
}

/// Reads the payload of a `length`-prefixed JPEG segment.
fn read_segment<'a>(bytes: &'a [u8], index: &mut usize) -> Result<&'a [u8], ImageError> {
    let raw = bytes.get(*index..*index + 2).ok_or(ImageError::Truncated {
        context: "JPEG segment length",
    })?;
    let length = usize::from(u16::from_be_bytes([raw[0], raw[1]]));
    if length < 2 {
        return Err(ImageError::InvalidData {
            reason: "JPEG segment length smaller than 2",
        });
    }
    *index += 2;
    let payload = bytes
        .get(*index..*index + (length - 2))
        .ok_or(ImageError::Truncated {
            context: "JPEG segment payload",
        })?;
    *index += length - 2;
    Ok(payload)
}

/// Parses `SOF0` (baseline) or `SOF2` (progressive) and allocates the component
/// planes/coefficient storage (dimension limits enforced here).
fn parse_sof(
    bytes: &[u8],
    index: &mut usize,
    limits: &PngLimits,
    progressive: bool,
    scale: usize,
) -> Result<JpegFrame, ImageError> {
    let segment = read_segment(bytes, index)?;
    if segment.len() < 6 {
        return Err(ImageError::Truncated {
            context: "JPEG SOF0 header",
        });
    }
    let precision = segment[0];
    if precision != 8 {
        return Err(ImageError::UnsupportedFeature {
            feature: "JPEG sample precision other than 8 bits",
        });
    }
    let height = u32::from(u16::from_be_bytes([segment[1], segment[2]]));
    let width = u32::from(u16::from_be_bytes([segment[3], segment[4]]));
    let count = usize::from(segment[5]);
    if width == 0 || height == 0 {
        return Err(ImageError::InvalidDimensions { width, height });
    }
    if width > limits.max_dimension || height > limits.max_dimension {
        return Err(ImageError::LimitExceeded {
            what: "JPEG dimension",
            limit: u64::from(limits.max_dimension),
            actual: u64::from(width.max(height)),
        });
    }
    let pixels = u64::from(width) * u64::from(height);
    if pixels > limits.max_pixels {
        return Err(ImageError::LimitExceeded {
            what: "JPEG pixel count",
            limit: limits.max_pixels,
            actual: pixels,
        });
    }
    if count == 0 || count > MAX_COMPONENTS {
        return Err(ImageError::InvalidData {
            reason: "JPEG component count out of range",
        });
    }
    if segment.len() < 6 + count * 3 {
        return Err(ImageError::Truncated {
            context: "JPEG SOF0 components",
        });
    }
    let mut components = Vec::with_capacity(count);
    let mut max_h = 1u32;
    let mut max_v = 1u32;
    let mut seen_ids = [false; 256];
    for component in 0..count {
        let base = 6 + component * 3;
        let id = segment[base];
        if seen_ids[usize::from(id)] {
            return Err(ImageError::InvalidData {
                reason: "duplicate JPEG component ID",
            });
        }
        seen_ids[usize::from(id)] = true;
        let sampling = segment[base + 1];
        let h = u32::from(sampling >> 4);
        let v = u32::from(sampling & 0x0F);
        let quant = usize::from(segment[base + 2]);
        if !(1..=4).contains(&h) || !(1..=4).contains(&v) || quant > 3 {
            return Err(ImageError::InvalidData {
                reason: "invalid JPEG component sampling/quantization index",
            });
        }
        max_h = max_h.max(h);
        max_v = max_v.max(v);
        components.push(FrameComponent {
            id,
            h,
            v,
            quant,
            dc_table: 0,
            ac_table: 0,
            plane: Vec::new(),
            plane_width: 0,
            plane_height: 0,
            blocks_x: 0,
            blocks_y: 0,
            coeffs: None,
            dc_pred: 0,
        });
    }
    // Allocate component planes now so a corrupt later segment cannot trigger huge work.
    // With a reduced IDCT each 8x8 block yields `sample x sample` samples (`sample =
    // 8 / scale`), so the planes and the output are correspondingly smaller.
    let sample = (8 / scale.clamp(1, 8)).max(1);
    for component in &mut components {
        let plane_width = div_ceil(width as usize * component.h as usize, max_h as usize);
        let plane_height = div_ceil(height as usize * component.v as usize, max_v as usize);
        let blocks_x = div_ceil(plane_width, 8);
        let blocks_y = div_ceil(plane_height, 8);
        component.blocks_y = blocks_y;
        let plane_w = blocks_x
            .checked_mul(sample)
            .ok_or(ImageError::LimitExceeded {
                what: "JPEG component plane",
                limit: limits.max_pixels,
                actual: pixels,
            })?;
        let plane_h = blocks_y
            .checked_mul(sample)
            .ok_or(ImageError::LimitExceeded {
                what: "JPEG component plane",
                limit: limits.max_pixels,
                actual: pixels,
            })?;
        let plane_len = plane_w
            .checked_mul(plane_h)
            .ok_or(ImageError::LimitExceeded {
                what: "JPEG component plane",
                limit: limits.max_pixels,
                actual: pixels,
            })?;
        // Planes are lazily written; only the full-resolution luma can be as large as the
        // image, chroma planes are smaller. Total allocation stays bounded by ~2x pixels.
        if plane_len as u64 > pixels.saturating_mul(4) {
            return Err(ImageError::InvalidData {
                reason: "JPEG component plane larger than expected",
            });
        }
        component.plane = vec![0u8; plane_len];
        component.plane_width = plane_w;
        component.plane_height = plane_h;
        component.blocks_x = blocks_x;
        component.dc_pred = 0;
    }
    Ok(JpegFrame {
        width,
        height,
        components,
        max_h: max_h as usize,
        max_v: max_v as usize,
        progressive,
    })
}

fn div_ceil(value: usize, divisor: usize) -> usize {
    value.div_ceil(divisor)
}

/// Parses `DQT` quantization tables.
fn parse_dqt(
    bytes: &[u8],
    index: &mut usize,
    tables: &mut [Option<QuantTable>; 4],
) -> Result<(), ImageError> {
    let segment = read_segment(bytes, index)?;
    let mut offset = 0usize;
    while offset < segment.len() {
        let byte = segment[offset];
        offset += 1;
        let precision = byte >> 4;
        let id = usize::from(byte & 0x0F);
        if id > 3 {
            return Err(ImageError::InvalidData {
                reason: "JPEG quantization table id out of range",
            });
        }
        let mut values = [0u16; BLOCK_LEN];
        for value in &mut values {
            if precision == 0 {
                let raw = *segment.get(offset).ok_or(ImageError::Truncated {
                    context: "JPEG DQT values",
                })?;
                offset += 1;
                *value = u16::from(raw);
            } else if precision == 1 {
                let raw = segment
                    .get(offset..offset + 2)
                    .ok_or(ImageError::Truncated {
                        context: "JPEG DQT values",
                    })?;
                offset += 2;
                *value = u16::from_be_bytes([raw[0], raw[1]]);
            } else {
                return Err(ImageError::InvalidData {
                    reason: "invalid JPEG quantization precision",
                });
            }
        }
        tables[id] = Some(QuantTable { values });
    }
    Ok(())
}

/// Parses `DHT` Huffman tables (DC class 0, AC class 1).
fn parse_dht(
    bytes: &[u8],
    index: &mut usize,
    dc_tables: &mut [Option<HuffmanTable>; 4],
    ac_tables: &mut [Option<HuffmanTable>; 4],
) -> Result<(), ImageError> {
    let segment = read_segment(bytes, index)?;
    let mut offset = 0usize;
    while offset < segment.len() {
        let header = *segment.get(offset).ok_or(ImageError::Truncated {
            context: "JPEG DHT header",
        })?;
        offset += 1;
        let class = header >> 4;
        let id = usize::from(header & 0x0F);
        if class > 1 || id > 3 {
            return Err(ImageError::InvalidData {
                reason: "JPEG Huffman table class/id out of range",
            });
        }
        let counts = segment
            .get(offset..offset + 16)
            .ok_or(ImageError::Truncated {
                context: "JPEG DHT counts",
            })?;
        offset += 16;
        let mut counts_array = [0u8; 16];
        counts_array.copy_from_slice(counts);
        let total: usize = counts_array.iter().map(|count| usize::from(*count)).sum();
        let values = segment
            .get(offset..offset + total)
            .ok_or(ImageError::Truncated {
                context: "JPEG DHT values",
            })?
            .to_vec();
        offset += total;
        let table = HuffmanTable::from_raw(&counts_array, values)?;
        if class == 0 {
            dc_tables[id] = Some(table);
        } else {
            ac_tables[id] = Some(table);
        }
    }
    Ok(())
}

/// Parsed `SOS` header: components in scan order, their table selectors and the
/// progressive spectral-selection/successive-approximation parameters.
struct ScanHeader {
    components: Vec<(u8, usize, usize)>,
    ss: u8,
    se: u8,
    ah: u8,
    al: u8,
}

fn parse_sos(
    bytes: &[u8],
    index: &mut usize,
    frame_components: &[FrameComponent],
    progressive: bool,
) -> Result<ScanHeader, ImageError> {
    let segment = read_segment(bytes, index)?;
    if segment.len() < 3 {
        return Err(ImageError::Truncated {
            context: "JPEG SOS header",
        });
    }
    let count = usize::from(segment[0]);
    if count == 0 || count > MAX_COMPONENTS || segment.len() < 1 + count * 2 + 3 {
        return Err(ImageError::InvalidData {
            reason: "invalid JPEG SOS component count",
        });
    }
    let mut components = Vec::with_capacity(count);
    let mut seen_ids = [false; 256];
    for scan_component in 0..count {
        let base = 1 + scan_component * 2;
        let id = segment[base];
        let tables = segment[base + 1];
        let dc = usize::from(tables >> 4);
        let ac = usize::from(tables & 0x0F);
        if dc > 3 || ac > 3 {
            return Err(ImageError::InvalidData {
                reason: "JPEG SOS table selector out of range",
            });
        }
        if seen_ids[usize::from(id)] {
            return Err(ImageError::InvalidData {
                reason: "JPEG SOS lists a component more than once",
            });
        }
        seen_ids[usize::from(id)] = true;
        let index = frame_components
            .iter()
            .position(|component| component.id == id)
            .ok_or(ImageError::InvalidData {
                reason: "JPEG SOS references an unknown component",
            })?;
        components.push((id, dc, ac));
        let _ = index;
    }
    let tail = &segment[1 + count * 2..];
    if tail.len() != 3 {
        return Err(ImageError::InvalidData {
            reason: "invalid JPEG SOS spectral selection bytes",
        });
    }
    let (ss, se) = (tail[0], tail[1]);
    let (ah, al) = (tail[2] >> 4, tail[2] & 0x0F);
    if !progressive {
        if ss != 0 || se != 63 || ah != 0 || al != 0 {
            return Err(ImageError::UnsupportedFeature {
                feature: "non-baseline JPEG spectral selection",
            });
        }
    } else {
        if ss > se || se > 63 || (ss == 0 && se != 0) || ah > 13 || al > 13 || (ah != 0 && al > ah)
        {
            return Err(ImageError::InvalidData {
                reason: "invalid progressive JPEG scan parameters",
            });
        }
        if components.len() > 1 && ss != 0 {
            return Err(ImageError::UnsupportedFeature {
                feature: "interleaved progressive JPEG AC scan",
            });
        }
    }
    Ok(ScanHeader {
        components,
        ss,
        se,
        ah,
        al,
    })
}

/// Per-scan component metadata resolved before the MCU loop.
struct ScanComponentMeta {
    index: usize,
    h: usize,
    v: usize,
    quant: usize,
    dc: usize,
    ac: usize,
}

/// Decodes the single baseline scan into the component planes.
#[allow(clippy::too_many_arguments)]
fn decode_scan(
    bytes: &[u8],
    index: &mut usize,
    width: u32,
    height: u32,
    components: &mut [FrameComponent],
    max_h: usize,
    max_v: usize,
    quant_tables: &[Option<QuantTable>; 4],
    dc_tables: &[Option<HuffmanTable>; 4],
    ac_tables: &[Option<HuffmanTable>; 4],
    scan: &ScanHeader,
    restart_interval: usize,
    scale: usize,
) -> Result<(), ImageError> {
    let scale = scale.clamp(1, 8);
    let sample = (8 / scale).max(1);
    let mcu_width = max_h * 8;
    let mcu_height = max_v * 8;
    let mcus_x = div_ceil(width as usize, mcu_width);
    let mcus_y = div_ceil(height as usize, mcu_height);
    let mut metas = Vec::with_capacity(scan.components.len());
    for (component_id, dc, ac) in &scan.components {
        let component_index = components
            .iter()
            .position(|component| component.id == *component_id)
            .ok_or(ImageError::InvalidData {
                reason: "JPEG scan component not found",
            })?;
        let component = &mut components[component_index];
        component.dc_table = *dc;
        component.ac_table = *ac;
        metas.push(ScanComponentMeta {
            index: component_index,
            h: component.h as usize,
            v: component.v as usize,
            quant: component.quant,
            dc: *dc,
            ac: *ac,
        });
    }
    let mut reader = BitReader::new(bytes, *index);
    let cosine = cosine_table();
    let reduced_cosine = (scale > 1).then(|| reduced_cosine_table(scale));
    let mut coefficients = [0i32; BLOCK_LEN];
    let mut blocks = [0u8; BLOCK_LEN];
    let mut mcu_count = 0usize;
    for mcu_y in 0..mcus_y {
        for mcu_x in 0..mcus_x {
            if restart_interval > 0 && mcu_count > 0 && mcu_count.is_multiple_of(restart_interval) {
                let expected = ((mcu_count / restart_interval - 1) % 8) as u8;
                reader.expect_restart(expected)?;
                for component in components.iter_mut() {
                    component.dc_pred = 0;
                }
            }
            for meta in &metas {
                let quant = quant_tables
                    .get(meta.quant)
                    .and_then(|table| table.as_ref())
                    .ok_or(ImageError::InvalidData {
                        reason: "JPEG missing quantization table",
                    })?;
                let dc_table = dc_tables
                    .get(meta.dc)
                    .and_then(|table| table.as_ref())
                    .ok_or(ImageError::InvalidData {
                        reason: "JPEG missing DC Huffman table",
                    })?;
                let ac_table = ac_tables
                    .get(meta.ac)
                    .and_then(|table| table.as_ref())
                    .ok_or(ImageError::InvalidData {
                        reason: "JPEG missing AC Huffman table",
                    })?;
                let blocks_x = components[meta.index].blocks_x;
                for block_y in 0..meta.v {
                    for block_x in 0..meta.h {
                        let block_col = mcu_x * meta.h + block_x;
                        let block_row = mcu_y * meta.v + block_y;
                        decode_block(
                            &mut reader,
                            dc_table,
                            ac_table,
                            &mut coefficients,
                            &mut components[meta.index],
                        )?;
                        if block_col >= blocks_x {
                            continue;
                        }
                        dequantize(&mut coefficients, quant);
                        if let Some(reduced_cosine) = reduced_cosine.as_ref() {
                            inverse_dct_reduced(&coefficients, reduced_cosine, sample, &mut blocks);
                            store_block_scaled(
                                &mut components[meta.index],
                                block_col,
                                block_row,
                                &blocks,
                                sample,
                            );
                        } else {
                            inverse_dct(&coefficients, &cosine, &mut blocks);
                            store_block(&mut components[meta.index], block_col, block_row, &blocks);
                        }
                    }
                }
            }
            mcu_count += 1;
        }
    }
    Ok(())
}

/// Decodes one 8x8 block of zig-zag coefficients (DC differential + AC run-length).
fn decode_block(
    reader: &mut BitReader<'_>,
    dc_table: &HuffmanTable,
    ac_table: &HuffmanTable,
    coefficients: &mut [i32; BLOCK_LEN],
    component: &mut FrameComponent,
) -> Result<(), ImageError> {
    coefficients.fill(0);
    let magnitude = reader.decode_huffman(dc_table)? as u32;
    if magnitude > 16 {
        return Err(ImageError::InvalidData {
            reason: "JPEG DC coefficient magnitude out of range",
        });
    }
    let difference = if magnitude == 0 {
        0
    } else {
        extend_coefficient(reader.get_bits(magnitude as u8)? as i32, magnitude)
    };
    component.dc_pred = component.dc_pred.wrapping_add(difference);
    coefficients[0] = component.dc_pred;
    let mut position = 1usize;
    while position < BLOCK_LEN {
        let symbol = reader.decode_huffman(ac_table)?;
        let run = usize::from(symbol >> 4);
        let size = u32::from(symbol & 0x0F);
        if size == 0 {
            if run == 15 {
                position += 16;
                continue;
            }
            break; // EOB
        }
        position += run;
        if position >= BLOCK_LEN {
            return Err(ImageError::InvalidData {
                reason: "JPEG AC run exceeds the block",
            });
        }
        let value = extend_coefficient(reader.get_bits(size as u8)? as i32, size);
        coefficients[ZIGZAG[position]] = value;
        position += 1;
    }
    Ok(())
}

fn dequantize(coefficients: &mut [i32; BLOCK_LEN], quant: &QuantTable) {
    for (coefficient, quant_value) in coefficients.iter_mut().zip(quant.values.iter()) {
        *coefficient *= i32::from(*quant_value);
    }
}

/// Naive separable 8x8 inverse DCT (float; deterministic, no unsafe).
fn inverse_dct(coefficients: &[i32; BLOCK_LEN], cosine: &[[f32; 8]; 8], out: &mut [u8; BLOCK_LEN]) {
    let mut intermediate = [0f32; BLOCK_LEN];
    // Rows: reconstruct vertically reversed? `coefficients` is in natural row-major order.
    for v in 0..8 {
        for x in 0..8 {
            let mut sum = 0f32;
            for u in 0..8 {
                sum += cosine[u][x] * coefficients[v * 8 + u] as f32;
            }
            intermediate[v * 8 + x] = sum;
        }
    }
    for y in 0..8 {
        for x in 0..8 {
            let mut sum = 0f32;
            for v in 0..8 {
                sum += cosine[v][y] * intermediate[v * 8 + x];
            }
            out[y * 8 + x] = (sum + 128.0).round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// Cosine basis for a reduced `sample x sample` IDCT: each output sample is evaluated
/// at the centre of its `8/sample` pixel group in the original 8x8 block (approximates
/// the box downsampling of libjpeg's scaled IDCT; tolerance documented in the crate).
fn reduced_cosine_table(scale: usize) -> Vec<[f32; 8]> {
    let sample = (8 / scale.clamp(1, 8)).max(1);
    let mut basis = Vec::with_capacity(sample);
    for out in 0..sample {
        let position = ((out as f32 + 0.5) * (8.0 / sample as f32)) - 0.5;
        let mut row = [0f32; 8];
        for (u, value) in row.iter_mut().enumerate() {
            let c = if u == 0 {
                std::f32::consts::FRAC_1_SQRT_2
            } else {
                1.0
            };
            let angle = (2.0 * position + 1.0) * (u as f32) * std::f32::consts::PI / 16.0;
            *value = c * angle.cos() / 2.0;
        }
        basis.push(row);
    }
    basis
}

/// Reduced separable IDCT: produces `sample x sample` samples from the 8x8
/// dequantized coefficients (higher AC coefficients are discarded by the basis).
fn inverse_dct_reduced(
    coefficients: &[i32; BLOCK_LEN],
    basis: &[[f32; 8]],
    sample: usize,
    out: &mut [u8; BLOCK_LEN],
) {
    let sample = sample.clamp(1, 8);
    let mut intermediate = [0f32; BLOCK_LEN];
    for v in 0..8 {
        for (x, row) in basis.iter().enumerate() {
            let mut sum = 0f32;
            for (u, weight) in row.iter().enumerate() {
                sum += weight * coefficients[v * 8 + u] as f32;
            }
            intermediate[v * 8 + x] = sum;
        }
    }
    for (y, row) in basis.iter().enumerate() {
        for x in 0..sample {
            let mut sum = 0f32;
            for (v, weight) in row.iter().enumerate() {
                sum += weight * intermediate[v * 8 + x];
            }
            out[y * sample + x] = (sum + 128.0).round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// Stores a reduced `sample x sample` block into the reduced component plane.
fn store_block_scaled(
    component: &mut FrameComponent,
    block_col: usize,
    block_row: usize,
    block: &[u8; BLOCK_LEN],
    sample: usize,
) {
    let sample = sample.clamp(1, 8);
    let base_x = block_col * sample;
    let base_y = block_row * sample;
    for y in 0..sample {
        let row = base_y + y;
        if row >= component.plane_height {
            break;
        }
        for x in 0..sample {
            let column = base_x + x;
            if column >= component.plane_width {
                break;
            }
            let value = block[y * sample + x];
            component.plane[row * component.plane_width + column] = value;
        }
    }
}

fn store_block(
    component: &mut FrameComponent,
    block_col: usize,
    block_row: usize,
    block: &[u8; BLOCK_LEN],
) {
    let base_x = block_col * 8;
    let base_y = block_row * 8;
    for y in 0..8 {
        let row = base_y + y;
        if row >= component.plane_height {
            break;
        }
        let row_start = row * component.plane_width + base_x;
        let row_slice = &mut component.plane[row_start..row_start + 8];
        row_slice.copy_from_slice(&block[y * 8..y * 8 + 8]);
    }
}

/// Converts component planes to RGB(A) with nearest-neighbour chroma upsampling.
fn render_components(
    width: u32,
    height: u32,
    components: &[FrameComponent],
    max_h: usize,
    max_v: usize,
    adobe_transform: Option<u8>,
) -> Result<RgbaImage, ImageError> {
    let mut data = vec![0u8; (width as usize) * (height as usize) * 4];
    let count = components.len();
    let (y_plane, cb_plane, cr_plane) = match count {
        1 => (&components[0], None, None),
        3 => (&components[0], Some(&components[1]), Some(&components[2])),
        4 => {
            return Err(ImageError::UnsupportedFeature {
                feature: "CMYK/YCCK JPEG (4 components) is not supported",
            });
        }
        _ => {
            return Err(ImageError::InvalidData {
                reason: "unsupported JPEG component count",
            });
        }
    };
    let srgb_direct = adobe_transform == Some(0) && count == 3;
    let sample = |plane: &FrameComponent, x: u32, y: u32| -> u8 {
        let sx = (x as usize * plane.h as usize / max_h).min(plane.plane_width.saturating_sub(1));
        let sy = (y as usize * plane.v as usize / max_v).min(plane.plane_height.saturating_sub(1));
        plane.plane[sy * plane.plane_width + sx]
    };
    for y in 0..height {
        for x in 0..width {
            let r;
            let g;
            let b;
            if let (Some(cb), Some(cr)) = (cb_plane, cr_plane) {
                let yy = f32::from(sample(y_plane, x, y));
                let cc_b = f32::from(sample(cb, x, y)) - 128.0;
                let cc_r = f32::from(sample(cr, x, y)) - 128.0;
                if srgb_direct {
                    r = sample(y_plane, x, y);
                    g = sample(cb, x, y);
                    b = sample(cr, x, y);
                } else {
                    r = (yy + 1.402 * cc_r).round().clamp(0.0, 255.0) as u8;
                    g = (yy - 0.344_136 * cc_b - 0.714_136 * cc_r)
                        .round()
                        .clamp(0.0, 255.0) as u8;
                    b = (yy + 1.772 * cc_b).round().clamp(0.0, 255.0) as u8;
                }
            } else {
                let yy = sample(y_plane, x, y);
                r = yy;
                g = yy;
                b = yy;
            }
            let offset = (y as usize * width as usize + x as usize) * 4;
            data[offset] = r;
            data[offset + 1] = g;
            data[offset + 2] = b;
            data[offset + 3] = 255;
        }
    }
    RgbaImage::new(width, height, data)
}

// ---------------------------------------------------------------------------
// Progressive JPEG (SOF2) coefficient accumulation and rendering
// ---------------------------------------------------------------------------

/// Allocates the per-component coefficient buffers used by progressive scans.
fn ensure_progressive_storage(frame: &mut JpegFrame) {
    for component in &mut frame.components {
        if component.coeffs.is_none() {
            let blocks = component.blocks_x.saturating_mul(component.blocks_y);
            component.coeffs = Some(vec![[0i32; BLOCK_LEN]; blocks]);
        }
    }
}

/// Decodes one progressive scan into the frame's coefficient buffers.
fn decode_progressive_scan(
    reader: &mut BitReader<'_>,
    frame: &mut JpegFrame,
    dc_tables: &[Option<HuffmanTable>; 4],
    ac_tables: &[Option<HuffmanTable>; 4],
    scan: &ScanHeader,
    restart_interval: usize,
) -> Result<(), ImageError> {
    let mut scan_components = Vec::with_capacity(scan.components.len());
    for (component_id, dc, ac) in &scan.components {
        let index = frame
            .components
            .iter()
            .position(|component| component.id == *component_id)
            .ok_or(ImageError::InvalidData {
                reason: "progressive JPEG scan references an unknown component",
            })?;
        let component = &mut frame.components[index];
        component.dc_table = *dc;
        component.ac_table = *ac;
        component.dc_pred = 0;
        scan_components.push(index);
    }

    let (mcus_x, mcus_y) = if scan_components.len() == 1 {
        let component = &frame.components[scan_components[0]];
        (component.blocks_x, component.blocks_y)
    } else {
        (
            div_ceil(frame.width as usize, frame.max_h * 8),
            div_ceil(frame.height as usize, frame.max_v * 8),
        )
    };

    let mut mcu_count = 0usize;
    let mut eob_run = 0u32;
    for mcu_y in 0..mcus_y {
        for mcu_x in 0..mcus_x {
            if restart_interval > 0 && mcu_count > 0 && mcu_count.is_multiple_of(restart_interval) {
                let expected = ((mcu_count / restart_interval - 1) % 8) as u8;
                reader.expect_restart(expected)?;
                for &index in &scan_components {
                    frame.components[index].dc_pred = 0;
                }
                eob_run = 0;
            }
            if scan_components.len() == 1 {
                let FrameComponent {
                    dc_pred,
                    dc_table,
                    ac_table,
                    blocks_x,
                    blocks_y,
                    coeffs,
                    ..
                } = &mut frame.components[scan_components[0]];
                let block_index = mcu_y * *blocks_x + mcu_x;
                if block_index >= *blocks_x * *blocks_y {
                    continue;
                }
                let coefficients = coeffs
                    .as_mut()
                    .and_then(|coeffs| coeffs.get_mut(block_index))
                    .ok_or(ImageError::InvalidData {
                        reason: "progressive JPEG coefficient buffer missing",
                    })?;
                decode_progressive_block(
                    reader,
                    dc_tables,
                    ac_tables,
                    dc_pred,
                    *dc_table,
                    *ac_table,
                    coefficients,
                    scan,
                    &mut eob_run,
                )?;
            } else {
                for &index in &scan_components {
                    let FrameComponent {
                        h,
                        v,
                        dc_table,
                        dc_pred,
                        blocks_x,
                        blocks_y,
                        coeffs,
                        ..
                    } = &mut frame.components[index];
                    let (h, v) = (*h as usize, *v as usize);
                    let dc_table = dc_tables
                        .get(*dc_table)
                        .and_then(|table| table.as_ref())
                        .ok_or(ImageError::InvalidData {
                            reason: "JPEG missing DC Huffman table",
                        })?;
                    for block_y in 0..v {
                        for block_x in 0..h {
                            let block_col = mcu_x * h + block_x;
                            let block_row = mcu_y * v + block_y;
                            if block_col >= *blocks_x || block_row >= *blocks_y {
                                continue;
                            }
                            let block_index = block_row * *blocks_x + block_col;
                            let coefficients = coeffs
                                .as_mut()
                                .and_then(|coeffs| coeffs.get_mut(block_index))
                                .ok_or(ImageError::InvalidData {
                                    reason: "progressive JPEG coefficient buffer missing",
                                })?;
                            if scan.ah == 0 {
                                decode_dc_first(reader, dc_table, coefficients, dc_pred, scan.al)?;
                            } else {
                                decode_dc_refine(reader, coefficients, scan.al)?;
                            }
                        }
                    }
                }
            }
            mcu_count += 1;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn decode_progressive_block(
    reader: &mut BitReader<'_>,
    dc_tables: &[Option<HuffmanTable>; 4],
    ac_tables: &[Option<HuffmanTable>; 4],
    dc_pred: &mut i32,
    dc_table_index: usize,
    ac_table_index: usize,
    coefficients: &mut [i32; BLOCK_LEN],
    scan: &ScanHeader,
    eob_run: &mut u32,
) -> Result<(), ImageError> {
    if scan.ss == 0 {
        if scan.ah == 0 {
            let table = dc_tables
                .get(dc_table_index)
                .and_then(|table| table.as_ref())
                .ok_or(ImageError::InvalidData {
                    reason: "JPEG missing DC Huffman table",
                })?;
            decode_dc_first(reader, table, coefficients, dc_pred, scan.al)
        } else {
            decode_dc_refine(reader, coefficients, scan.al)
        }
    } else {
        let table = ac_tables
            .get(ac_table_index)
            .and_then(|table| table.as_ref())
            .ok_or(ImageError::InvalidData {
                reason: "JPEG missing AC Huffman table",
            })?;
        if scan.ah == 0 {
            decode_ac_first(
                reader,
                table,
                coefficients,
                scan.ss,
                scan.se,
                scan.al,
                eob_run,
            )
        } else {
            decode_ac_refine(
                reader,
                table,
                coefficients,
                scan.ss,
                scan.se,
                scan.al,
                eob_run,
            )
        }
    }
}

fn decode_dc_first(
    reader: &mut BitReader<'_>,
    dc_table: &HuffmanTable,
    coefficients: &mut [i32; BLOCK_LEN],
    dc_pred: &mut i32,
    al: u8,
) -> Result<(), ImageError> {
    let magnitude = reader.decode_huffman(dc_table)? as u32;
    if magnitude > 16 {
        return Err(ImageError::InvalidData {
            reason: "JPEG DC coefficient magnitude out of range",
        });
    }
    let difference = if magnitude == 0 {
        0
    } else {
        extend_coefficient(reader.get_bits(magnitude as u8)? as i32, magnitude)
    };
    *dc_pred = dc_pred.wrapping_add(difference);
    coefficients[0] = *dc_pred << al;
    Ok(())
}

fn decode_dc_refine(
    reader: &mut BitReader<'_>,
    coefficients: &mut [i32; BLOCK_LEN],
    al: u8,
) -> Result<(), ImageError> {
    if reader.get_bits(1)? == 1 {
        coefficients[0] |= 1 << al;
    }
    Ok(())
}

fn decode_ac_first(
    reader: &mut BitReader<'_>,
    ac_table: &HuffmanTable,
    coefficients: &mut [i32; BLOCK_LEN],
    ss: u8,
    se: u8,
    al: u8,
    eob_run: &mut u32,
) -> Result<(), ImageError> {
    // Progressive AC-first scans may encode an end-of-band run covering several blocks
    // (ITU T.81 G.1.2.2 + libjpeg's EOBRUN handling).
    if *eob_run > 0 {
        *eob_run -= 1;
        return Ok(());
    }
    let mut k = usize::from(ss);
    let se = usize::from(se);
    while k <= se {
        let symbol = reader.decode_huffman(ac_table)?;
        let run = usize::from(symbol >> 4);
        let size = u32::from(symbol & 0x0F);
        if size == 0 {
            if run == 15 {
                k += 16;
                continue;
            }
            *eob_run = (1u32 << run) + reader.get_bits(run as u8)?;
            *eob_run -= 1;
            break;
        }
        k += run;
        if k > se {
            return Err(ImageError::InvalidData {
                reason: "progressive JPEG AC run exceeds the band",
            });
        }
        let value = extend_coefficient(reader.get_bits(size as u8)? as i32, size);
        coefficients[ZIGZAG[k]] = value << al;
        k += 1;
    }
    Ok(())
}

fn decode_ac_refine(
    reader: &mut BitReader<'_>,
    ac_table: &HuffmanTable,
    coefficients: &mut [i32; BLOCK_LEN],
    ss: u8,
    se: u8,
    al: u8,
    eob_run: &mut u32,
) -> Result<(), ImageError> {
    if *eob_run > 0 {
        *eob_run -= 1;
        return Ok(());
    }
    let p1 = 1i32 << al;
    let m1 = -p1;
    let mut k = usize::from(ss);
    let se = usize::from(se);
    while k <= se {
        let symbol = reader.decode_huffman(ac_table)?;
        let mut r = i32::from(symbol >> 4);
        let mut s = i32::from(symbol & 0x0F);
        if s == 0 {
            if r < 15 {
                *eob_run = (1u32 << r) + reader.get_bits(r as u8)?;
                *eob_run -= 1;
                return Ok(());
            }
        } else {
            if s != 1 {
                return Err(ImageError::InvalidData {
                    reason: "progressive JPEG AC refinement symbol size is not 1",
                });
            }
            s = if reader.get_bits(1)? == 1 { p1 } else { m1 };
        }
        // Advance over already-nonzero coefficients (appending correction bits)
        // until the run of still-zero coefficients is exhausted.
        while k <= se {
            let natural = ZIGZAG[k];
            if coefficients[natural] != 0 {
                if reader.get_bits(1)? == 1 {
                    if coefficients[natural] >= 0 {
                        coefficients[natural] += p1;
                    } else {
                        coefficients[natural] += m1;
                    }
                }
            } else if r == 0 {
                break;
            } else {
                r -= 1;
            }
            k += 1;
        }
        if k > se {
            break;
        }
        if s != 0 {
            coefficients[ZIGZAG[k]] = s;
        }
        k += 1;
    }
    Ok(())
}

fn render_progressive_components(
    mut frame: JpegFrame,
    quant_tables: &[Option<QuantTable>; 4],
    adobe_transform: Option<u8>,
) -> Result<RgbaImage, ImageError> {
    ensure_progressive_storage(&mut frame);
    let cosine = cosine_table();
    let mut block = [0u8; BLOCK_LEN];
    for component in &mut frame.components {
        let quant = quant_tables
            .get(component.quant)
            .and_then(|table| table.as_ref())
            .ok_or(ImageError::InvalidData {
                reason: "JPEG missing quantization table",
            })?;
        let coeffs = component.coeffs.take().ok_or(ImageError::InvalidData {
            reason: "progressive JPEG coefficient buffer missing",
        })?;
        for (block_index, stored) in coeffs.into_iter().enumerate() {
            let mut coefficients = stored;
            dequantize(&mut coefficients, quant);
            inverse_dct(&coefficients, &cosine, &mut block);
            let block_row = block_index / component.blocks_x.max(1);
            let block_col = block_index % component.blocks_x.max(1);
            store_block(component, block_col, block_row, &block);
        }
    }
    let JpegFrame {
        width,
        height,
        components,
        max_h,
        max_v,
        ..
    } = frame;
    render_components(width, height, &components, max_h, max_v, adobe_transform)
}
