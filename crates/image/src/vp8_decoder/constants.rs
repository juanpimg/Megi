#[allow(unused_imports)]
use super::*;

pub(crate) const BLOCK_TYPES: usize = 4;
pub(crate) const BANDS: usize = 8;
pub(crate) const CONTEXTS: usize = 3;
pub(crate) const NODES: usize = 11;
pub(crate) const MAX_MB_SEGMENTS: usize = 4;

/// `vp8_coef_bands` with the 16 sentinel (see libvpx detokenize.c).
pub(crate) const COEFF_BANDS: [usize; 17] = [0, 1, 2, 3, 6, 4, 5, 6, 6, 6, 6, 6, 6, 6, 6, 7, 0];
/// Zig-zag scan order for the 4x4 DCT.
pub(crate) const ZIGZAG: [usize; 16] = [0, 1, 4, 8, 5, 2, 3, 6, 9, 12, 13, 10, 7, 11, 14, 15];
/// Keyframe Y mode tree (B_PRED, DC, V, H, TM).
pub(crate) const KF_YMODE_TREE: [i32; 8] = [-4, 2, 4, 6, 0, -1, -2, -3];
/// UV mode tree (DC, V, H, TM).
pub(crate) const UV_MODE_TREE: [i32; 6] = [0, 2, -1, 4, -2, -3];
/// B_PRED mode tree (all ten 4x4 modes).
pub(crate) const BMODE_TREE: [i32; 18] = [
    -0, 2, -1, 4, -2, 6, 8, 12, -3, 10, -5, -6, -4, 14, -7, 16, -8, -9,
];
/// Extra-bit probabilities for DCT coefficient categories 1..=6.
pub(crate) const CAT_PROBS: [&[u8]; 4] = [
    &[173, 148, 140],
    &[176, 155, 140, 135],
    &[180, 157, 141, 134, 130],
    &[254, 254, 243, 230, 196, 177, 153, 140, 133, 130, 129],
];
pub(crate) const CAT1_PROB: u8 = 159;
pub(crate) const CAT2_PROBS: [u8; 2] = [165, 145];
