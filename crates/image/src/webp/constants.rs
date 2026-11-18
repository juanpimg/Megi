#[allow(unused_imports)]
use super::*;

/// VP8L chunk signature byte.
pub(crate) const VP8L_SIGNATURE: u8 = 0x2f;
/// `RIFF` header + `WEBP` form type.
pub(crate) const RIFF_HEADER_LEN: usize = 12;
/// Hard caps (format + defensive allocation bounds).
pub(crate) const MAX_TRANSFORMS: usize = 4;
pub(crate) const MAX_HUFFMAN_GROUPS: usize = 256;
pub(crate) const MAX_HUFFMAN_CODE_LEN: u8 = 15;
pub(crate) const MAX_CACHE_BITS: u32 = 11;
pub(crate) const NUM_LENGTH_CODES: u16 = 24;
pub(crate) const NUM_LITERAL_CODES: u16 = 256;
pub(crate) const NUM_DISTANCE_CODES: u16 = 40;
pub(crate) const CODE_TO_PLANE_CODES: usize = 120;
