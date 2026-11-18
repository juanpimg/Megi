#[allow(unused_imports)]
use super::*;

/// Maximum components accepted (Y, Cb, Cr, K).
pub(crate) const MAX_COMPONENTS: usize = 4;
/// Maximum Huffman code length (T.81 Table B.5).
pub(crate) const MAX_HUFFMAN_LEN: usize = 16;
/// Number of coefficients in an 8x8 DCT block.
pub(crate) const BLOCK_LEN: usize = 64;

/// Zig-zag index -> natural (row-major) index for coefficient dequantization.
pub(crate) const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

/// Cosine basis table `COS[u][x] = cos((2x+1) u pi / 16)` scaled by `1/sqrt(2)` for `u=0`.
pub(crate) fn cosine_table() -> [[f32; 8]; 8] {
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
