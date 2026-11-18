#[allow(unused_imports)]
use super::*;

/// Adam7 pass geometry: `(x_start, y_start, x_step, y_step)`.
pub(crate) const ADAM7: [[u32; 4]; 7] = [
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
pub(crate) struct Header {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) bit_depth: u8,
    pub(crate) color_type: u8,
    pub(crate) interlace: u8,
}

impl Header {
    /// Samples per pixel for the colour type.
    pub(crate) fn channels(self) -> u32 {
        match self.color_type {
            0 | 3 => 1,
            2 => 3,
            4 => 2,
            _ => 4,
        }
    }

    /// Bits per pixel (`channels * bit_depth`).
    pub(crate) fn bits_per_pixel(self) -> u64 {
        self.channels() as u64 * self.bit_depth as u64
    }

    /// Bytes per pixel rounded up to a whole byte (filter offset).
    pub(crate) fn filter_bpp(self) -> usize {
        self.bits_per_pixel().div_ceil(8).max(1) as usize
    }
}

/// Transparency from a `tRNS` chunk.
#[derive(Clone, Debug)]
pub(crate) enum Transparency {
    /// Transparent gray sample (colour type 0).
    Gray(u16),
    /// Transparent RGB triple (colour type 2).
    Rgb(u16, u16, u16),
    /// Per-palette-index alpha (colour type 3).
    Palette(Vec<u8>),
}

/// Decoder state shared by the per-pass row renderer.
pub(crate) struct Context<'a> {
    pub(crate) header: Header,
    pub(crate) palette: Option<&'a [[u8; 3]]>,
    pub(crate) transparency: Option<&'a Transparency>,
}
