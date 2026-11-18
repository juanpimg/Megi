#[allow(unused_imports)]
use super::*;

/// One frame component (`SOF0`).
#[derive(Clone)]
pub(crate) struct FrameComponent {
    pub(crate) id: u8,
    pub(crate) h: u32,
    pub(crate) v: u32,
    pub(crate) quant: usize,
    pub(crate) dc_table: usize,
    pub(crate) ac_table: usize,
    pub(crate) plane: Vec<u8>,
    pub(crate) plane_width: usize,
    pub(crate) plane_height: usize,
    pub(crate) blocks_x: usize,
    pub(crate) blocks_y: usize,
    /// Progressive JPEG coefficient accumulator (natural row-major order),
    /// `blocks_x * blocks_y` blocks; `None` for baseline decodes.
    pub(crate) coeffs: Option<Vec<[i32; BLOCK_LEN]>>,
    pub(crate) dc_pred: i32,
}

/// JPEG frame header data (SOF0 baseline or SOF2 progressive).
pub(crate) struct JpegFrame {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) components: Vec<FrameComponent>,
    pub(crate) max_h: usize,
    pub(crate) max_v: usize,
    pub(crate) progressive: bool,
}
