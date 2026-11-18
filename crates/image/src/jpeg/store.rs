#[allow(unused_imports)]
use super::*;

/// Stores a reduced `sample x sample` block into the reduced component plane.
pub(crate) fn store_block_scaled(
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

pub(crate) fn store_block(
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
pub(crate) fn render_components(
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
pub(crate) fn ensure_progressive_storage(frame: &mut JpegFrame) {
    for component in &mut frame.components {
        if component.coeffs.is_none() {
            let blocks = component.blocks_x.saturating_mul(component.blocks_y);
            component.coeffs = Some(vec![[0i32; BLOCK_LEN]; blocks]);
        }
    }
}
