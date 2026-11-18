#[allow(unused_imports)]
use super::*;

/// Fancy chroma upsampling + YUV->RGB for one output row (libwebp
/// `UpsampleRgbaLinePair_C` + `VP8YuvToRgba`), writing RGBA into `dst`.
///
/// `top_*` is the upper chroma row and `cur_*` the lower one; `bottom` selects the
/// 3:1 vertical weighting direction (libwebp `EmitFancyRGB` row mapping).
#[allow(clippy::too_many_arguments)]
pub(crate) fn yuv_row_rgba(
    y_row: &[u8],
    top_u: &[u8],
    top_v: &[u8],
    cur_u: &[u8],
    cur_v: &[u8],
    len: usize,
    bottom: bool,
    dst: &mut [u8],
) {
    if len == 0 {
        return;
    }
    let mut tl_u = i32::from(top_u[0]);
    let mut tl_v = i32::from(top_v[0]);
    let mut l_u = i32::from(cur_u[0]);
    let mut l_v = i32::from(cur_v[0]);
    let (first_u, first_v) = if bottom {
        ((3 * l_u + tl_u + 2) >> 2, (3 * l_v + tl_v + 2) >> 2)
    } else {
        ((3 * tl_u + l_u + 2) >> 2, (3 * tl_v + l_v + 2) >> 2)
    };
    write_rgba(dst, 0, y_row[0], first_u as u8, first_v as u8);
    let last_pair = (len - 1) >> 1;
    for x in 1..=last_pair {
        let t_u = i32::from(top_u[x]);
        let t_v = i32::from(top_v[x]);
        let uv_u = i32::from(cur_u[x]);
        let uv_v = i32::from(cur_v[x]);
        let avg_u = tl_u + t_u + l_u + uv_u + 8;
        let avg_v = tl_v + t_v + l_v + uv_v + 8;
        let diag12_u = (avg_u + 2 * (t_u + l_u)) >> 3;
        let diag12_v = (avg_v + 2 * (t_v + l_v)) >> 3;
        let diag03_u = (avg_u + 2 * (tl_u + uv_u)) >> 3;
        let diag03_v = (avg_v + 2 * (tl_v + uv_v)) >> 3;
        let odd = 2 * x - 1;
        if bottom {
            write_rgba(
                dst,
                odd,
                y_row[odd],
                ((diag03_u + l_u) >> 1) as u8,
                ((diag03_v + l_v) >> 1) as u8,
            );
            write_rgba(
                dst,
                odd + 1,
                y_row[odd + 1],
                ((diag12_u + uv_u) >> 1) as u8,
                ((diag12_v + uv_v) >> 1) as u8,
            );
        } else {
            write_rgba(
                dst,
                odd,
                y_row[odd],
                ((diag12_u + tl_u) >> 1) as u8,
                ((diag12_v + tl_v) >> 1) as u8,
            );
            write_rgba(
                dst,
                odd + 1,
                y_row[odd + 1],
                ((diag03_u + t_u) >> 1) as u8,
                ((diag03_v + t_v) >> 1) as u8,
            );
        }
        tl_u = t_u;
        tl_v = t_v;
        l_u = uv_u;
        l_v = uv_v;
    }
    if len.is_multiple_of(2) {
        let last = len - 1;
        let (u, v) = if bottom {
            ((3 * l_u + tl_u + 2) >> 2, (3 * l_v + tl_v + 2) >> 2)
        } else {
            ((3 * tl_u + l_u + 2) >> 2, (3 * tl_v + l_v + 2) >> 2)
        };
        write_rgba(dst, last, y_row[last], u as u8, v as u8);
    }
}

/// Writes the RGBA8 pixel for output column `x` into `dst`.
#[inline]
pub(crate) fn write_rgba(dst: &mut [u8], x: usize, y: u8, u: u8, v: u8) {
    let pixel = yuv_pixel(y, u, v);
    dst[x * 4..x * 4 + 4].copy_from_slice(&pixel);
}

#[inline]
pub(crate) fn mult_hi(value: i32, coefficient: i32) -> i32 {
    (value * coefficient) >> 8
}

#[inline]
pub(crate) fn clip8(value: i32) -> u8 {
    if value < 0 {
        0
    } else if value > (256 << 6) - 1 {
        255
    } else {
        (value >> 6) as u8
    }
}

/// Converts one YUV420 pixel to RGBA8 (libwebp fixed-point BT.601).
#[inline]
pub(crate) fn yuv_pixel(y: u8, u: u8, v: u8) -> [u8; 4] {
    let yy = i32::from(y);
    let uu = i32::from(u);
    let vv = i32::from(v);
    let r = clip8(mult_hi(yy, 19077) + mult_hi(vv, 26149) - 14234);
    let g = clip8(mult_hi(yy, 19077) - mult_hi(uu, 6419) - mult_hi(vv, 13320) + 8708);
    let b = clip8(mult_hi(yy, 19077) + mult_hi(uu, 33050) - 17685);
    [r, g, b, 255]
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn yuv420_to_rgba(
    y_plane: &[u8],
    y_stride: usize,
    u_plane: &[u8],
    v_plane: &[u8],
    c_stride: usize,
    width: usize,
    height: usize,
    chroma_width: usize,
    chroma_height: usize,
) -> Result<RgbaImage, ImageError> {
    let cw = width.div_ceil(2).min(chroma_width);
    let last_chroma = chroma_height.saturating_sub(1);
    let mut data = vec![0u8; width * height * 4];
    // Row mapping of libwebp's fancy upsampler (`EmitFancyRGB`): output row 0
    // mirrors chroma row 0; odd rows 2k+1 mix chroma rows k (3/4) and k+1 (1/4);
    // even rows 2k mix chroma rows k-1 (1/4) and k (3/4).
    for y in 0..height {
        let (top_row, cur_row, bottom) = if y == 0 {
            (0, 0, false)
        } else if y % 2 == 1 {
            let k = y / 2;
            (k.min(last_chroma), (k + 1).min(last_chroma), false)
        } else {
            let k = y / 2;
            ((k - 1).min(last_chroma), k.min(last_chroma), true)
        };
        let top_offset = plane_offset(c_stride, 0, top_row);
        let cur_offset = plane_offset(c_stride, 0, cur_row);
        let y_offset = plane_offset(y_stride, 0, y);
        let row = &mut data[y * width * 4..(y + 1) * width * 4];
        yuv_row_rgba(
            &y_plane[y_offset..y_offset + width],
            &u_plane[top_offset..top_offset + cw],
            &v_plane[top_offset..top_offset + cw],
            &u_plane[cur_offset..cur_offset + cw],
            &v_plane[cur_offset..cur_offset + cw],
            width,
            bottom,
            row,
        );
    }
    RgbaImage::new(width as u32, height as u32, data)
}
