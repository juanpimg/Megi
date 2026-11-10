//! Baseline JPEG (ITU-T T.81 / JFIF) decoder — `std` only, no `unsafe`.
//!
//! ## Supported subset
//!
//! - **Baseline sequential DCT** (`SOF0`), Huffman entropy coding (`DHT`/`SOS`);
//! - quantization tables (`DQT`, 8- and 16-bit precision), 1-4 components;
//! - sampling factors **4:4:4, 4:2:2 and 4:2:0** (any `H/V` 1-4 within the MCU limits);
//! - restart intervals (`DRI` + `RSTn` markers, DC predictor reset);
//! - grayscale (`1` component) and `YCbCr` (3 components); Adobe `APP14` transform `0`
//!   (RGB) is honoured, `CMYK`/`YCCK` are rejected with a clear error;
//! - nearest-neighbour chroma upsampling (4:4:4 is exact; 4:2:2/4:2:0 are documented as
//!   blocky chroma — acceptable for photographs at the engine's current fidelity);
//! - EXIF `APP1`/JFIF `APP0` are skipped; EXIF **orientation is not applied** (documented).
//!
//! ## Rejected with a clear error (never a panic)
//!
//! - progressive (`SOF2`) and all other DCT modes (arithmetic/lossless/hierarchical);
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
                max_code[length] = code + count - 1;
                code += count;
                index += count as usize;
            }
            code <<= 1;
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
    dc_pred: i32,
}

/// Decodes a baseline JPEG stream into an [`RgbaImage`].
pub(crate) fn decode_jpeg(bytes: &[u8], limits: &PngLimits) -> Result<RgbaImage, ImageError> {
    let mut index = 0usize;
    if bytes.get(..2) != Some(&[0xFF, 0xD8]) {
        return Err(ImageError::InvalidSignature);
    }
    index += 2;

    let mut quant_tables: [Option<QuantTable>; 4] = [None, None, None, None];
    let mut dc_tables: [Option<HuffmanTable>; 4] = [None, None, None, None];
    let mut ac_tables: [Option<HuffmanTable>; 4] = [None, None, None, None];
    let mut frame: Option<(u32, u32, Vec<FrameComponent>, usize, usize)> = None;
    let mut restart_interval = 0usize;
    let mut adobe_transform: Option<u8> = None;

    loop {
        let marker = next_marker(bytes, &mut index)?;
        match marker {
            0xD8 => {
                return Err(ImageError::InvalidData {
                    reason: "unexpected second JPEG SOI marker",
                });
            }
            0xD9 => break,
            0xC0 => {
                if frame.is_some() {
                    return Err(ImageError::InvalidData {
                        reason: "duplicate JPEG frame header",
                    });
                }
                frame = Some(parse_sof0(bytes, &mut index, limits)?);
            }
            0xC2 => {
                return Err(ImageError::UnsupportedFeature {
                    feature: "progressive JPEG (SOF2) is not supported",
                });
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
                let Some((width, height, mut components, max_h, max_v)) = frame.take() else {
                    return Err(ImageError::InvalidData {
                        reason: "JPEG scan before frame header",
                    });
                };
                let scan = parse_sos(bytes, &mut index, &components)?;
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
                )?;
                return render_components(
                    width,
                    height,
                    &components,
                    max_h,
                    max_v,
                    adobe_transform,
                );
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

/// Parses `SOF0` and allocates the component planes (dimension limits enforced here).
fn parse_sof0(
    bytes: &[u8],
    index: &mut usize,
    limits: &PngLimits,
) -> Result<(u32, u32, Vec<FrameComponent>, usize, usize), ImageError> {
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
    for component in 0..count {
        let base = 6 + component * 3;
        let id = segment[base];
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
            dc_pred: 0,
        });
    }
    // Allocate component planes now so a corrupt later segment cannot trigger huge work.
    for component in &mut components {
        let plane_width = div_ceil(width as usize * component.h as usize, max_h as usize);
        let plane_height = div_ceil(height as usize * component.v as usize, max_v as usize);
        let blocks_x = div_ceil(plane_width, 8);
        let blocks_y = div_ceil(plane_height, 8);
        let plane_w = blocks_x.checked_mul(8).ok_or(ImageError::LimitExceeded {
            what: "JPEG component plane",
            limit: limits.max_pixels,
            actual: pixels,
        })?;
        let plane_h = blocks_y.checked_mul(8).ok_or(ImageError::LimitExceeded {
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
    Ok((width, height, components, max_h as usize, max_v as usize))
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

/// Parsed `SOS` header: component ids in scan order and their table selectors.
struct ScanHeader {
    components: Vec<(u8, usize, usize)>,
}

fn parse_sos(
    bytes: &[u8],
    index: &mut usize,
    frame_components: &[FrameComponent],
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
        let index = frame_components
            .iter()
            .position(|component| component.id == id)
            .ok_or(ImageError::InvalidData {
                reason: "JPEG SOS references an unknown component",
            })?;
        components.push((id, dc, ac));
        let _ = index;
    }
    // Baseline requires the spectral selection to be 0..=63 with successive approximation 0.
    let tail = &segment[1 + count * 2..];
    if tail.len() != 3 || tail[0] != 0 || tail[1] != 63 || tail[2] != 0 {
        return Err(ImageError::UnsupportedFeature {
            feature: "non-baseline JPEG spectral selection",
        });
    }
    Ok(ScanHeader { components })
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
) -> Result<(), ImageError> {
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
                        inverse_dct(&coefficients, &cosine, &mut blocks);
                        store_block(&mut components[meta.index], block_col, block_row, &blocks);
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
