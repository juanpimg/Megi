#[allow(unused_imports)]
use super::*;

/// Decodes one interlace pass from `inflated` into `image`.
///
/// These helpers are force-inlined on purpose: the r26 scaled-decode path added a
/// second caller of `pixel_from_row`, which made the optimiser stop inlining the
/// per-pixel conversion into the full-resolution decode (+21.7 % regression,
/// `image/decode-png-800x600` 6.55 → 7.98 ms). `inline(always)` keeps the full path
/// at r25 cost while the scaled path shares the same conversion code.
#[inline(always)]
pub(crate) fn decode_pass(
    context: &Context<'_>,
    inflated: &[u8],
    offset: &mut usize,
    pass: [u32; 4],
    image: &mut [u8],
) -> Result<(), ImageError> {
    let [x0, y0, dx, dy] = pass;
    let width = pass_size(context.header.width, x0, dx);
    let height = pass_size(context.header.height, y0, dy);
    if width == 0 || height == 0 {
        return Ok(());
    }
    let row_length = row_bytes(width, context.header.bits_per_pixel())?;
    let bpp = context.header.filter_bpp();
    let mut previous = vec![0u8; row_length];
    let mut current = vec![0u8; row_length];

    for row_index in 0..height {
        let filter = *inflated.get(*offset).ok_or(ImageError::Truncated {
            context: "scanline filter byte",
        })?;
        *offset += 1;
        let end = offset
            .checked_add(row_length)
            .ok_or(ImageError::Truncated {
                context: "scanline data",
            })?;
        let raw = inflated.get(*offset..end).ok_or(ImageError::Truncated {
            context: "scanline data",
        })?;
        current.copy_from_slice(raw);
        *offset = end;

        unfilter(&mut current, &previous, filter, bpp)?;
        let y = y0 + row_index * dy;
        context.render_row(&current, width, x0, y, dx, image)?;
        std::mem::swap(&mut previous, &mut current);
    }
    Ok(())
}

/// Scaled variant of [`decode_pass`]: unfilters every scanline (required by the PNG
/// filters) but only converts the sampled grid (`x % factor == 0 && y % factor == 0`),
/// writing directly into the reduced RGBA buffer. This is the documented
/// "row/column subsampling after unfilter" of the scaled-decode API.
#[allow(clippy::too_many_arguments)]
pub(crate) fn decode_pass_scaled(
    context: &Context<'_>,
    inflated: &[u8],
    offset: &mut usize,
    pass: [u32; 4],
    factor: u32,
    out_width: u32,
    out_height: u32,
    data: &mut [u8],
) -> Result<(), ImageError> {
    let [x0, y0, dx, dy] = pass;
    let width = pass_size(context.header.width, x0, dx);
    let height = pass_size(context.header.height, y0, dy);
    if width == 0 || height == 0 {
        return Ok(());
    }
    let row_length = row_bytes(width, context.header.bits_per_pixel())?;
    let bpp = context.header.filter_bpp();
    let mut previous = vec![0u8; row_length];
    let mut current = vec![0u8; row_length];

    for row_index in 0..height {
        let filter = *inflated.get(*offset).ok_or(ImageError::Truncated {
            context: "scanline filter byte",
        })?;
        *offset += 1;
        let end = offset
            .checked_add(row_length)
            .ok_or(ImageError::Truncated {
                context: "scanline data",
            })?;
        let raw = inflated.get(*offset..end).ok_or(ImageError::Truncated {
            context: "scanline data",
        })?;
        current.copy_from_slice(raw);
        *offset = end;
        unfilter(&mut current, &previous, filter, bpp)?;
        let y = y0 + row_index * dy;
        if y % factor == 0 {
            let out_y = (y / factor) as usize;
            if out_y < out_height as usize {
                for pixel_index in 0..width {
                    let x = x0 + pixel_index * dx;
                    if x % factor != 0 {
                        continue;
                    }
                    let out_x = (x / factor) as usize;
                    if out_x >= out_width as usize {
                        continue;
                    }
                    let rgba = context.pixel_from_row(&current, pixel_index)?;
                    let target = (out_y * out_width as usize + out_x) * 4;
                    data[target..target + 4].copy_from_slice(&rgba);
                }
            }
        }
        std::mem::swap(&mut previous, &mut current);
    }
    Ok(())
}

/// Reverses PNG scanline filtering (types 0–4).
#[inline(always)]
pub(crate) fn unfilter(
    row: &mut [u8],
    previous: &[u8],
    filter: u8,
    bpp: usize,
) -> Result<(), ImageError> {
    match filter {
        0 => Ok(()),
        1 => {
            for index in bpp..row.len() {
                row[index] = row[index].wrapping_add(row[index - bpp]);
            }
            Ok(())
        }
        2 => {
            for (value, up) in row.iter_mut().zip(previous.iter()) {
                *value = value.wrapping_add(*up);
            }
            Ok(())
        }
        3 => {
            // The first `bpp` bytes have no left neighbour: average is `up / 2`.
            for index in 0..bpp.min(row.len()) {
                row[index] = row[index].wrapping_add(previous[index] / 2);
            }
            for index in bpp..row.len() {
                let left = row[index - bpp];
                let up = previous[index];
                let average = ((left as u16 + up as u16) / 2) as u8;
                row[index] = row[index].wrapping_add(average);
            }
            Ok(())
        }
        4 => {
            // With no left neighbour the Paeth predictor always selects `up`.
            for index in 0..bpp.min(row.len()) {
                row[index] = row[index].wrapping_add(previous[index]);
            }
            for index in bpp..row.len() {
                let left = row[index - bpp];
                let up = previous[index];
                let upper_left = previous[index - bpp];
                row[index] = row[index].wrapping_add(paeth(left, up, upper_left));
            }
            Ok(())
        }
        _ => Err(ImageError::InvalidData {
            reason: "invalid scanline filter type",
        }),
    }
}

/// PNG Paeth predictor (RFC 2083 §6.6).
pub(crate) fn paeth(left: u8, up: u8, upper_left: u8) -> u8 {
    let left = left as i32;
    let up = up as i32;
    let upper_left = upper_left as i32;
    let estimate = left + up - upper_left;
    let distance_left = (estimate - left).abs();
    let distance_up = (estimate - up).abs();
    let distance_upper_left = (estimate - upper_left).abs();
    if distance_left <= distance_up && distance_left <= distance_upper_left {
        left as u8
    } else if distance_up <= distance_upper_left {
        up as u8
    } else {
        upper_left as u8
    }
}
