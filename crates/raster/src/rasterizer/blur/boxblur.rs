//! Separable box blur over scalar masks.

#[allow(unused_imports)]
use super::*;

/// One separable box-blur pass over a scalar mask (deterministic f32 arithmetic).
/// One box-blur pass over `input`, writing to `output`.
///
/// The loop is split into left/middle/right regions so the middle (the vast
/// majority of pixels) has no per-pixel bounds branches. The floating-point
/// accumulation order is exactly the same as the previous single-loop version,
/// so the resulting values — and therefore every rendered byte — are unchanged
/// (R181).
pub(crate) fn blur_pass_into(
    input: &[f32],
    output: &mut [f32],
    width: usize,
    height: usize,
    horizontal: bool,
    radius: usize,
) {
    if radius == 0 || width == 0 || height == 0 {
        output.copy_from_slice(input);
        return;
    }
    let window = 2 * radius + 1;
    let divisor = window as f32;
    if horizontal {
        let left_end = (radius + 1).min(width);
        let middle_end = width.saturating_sub(radius);
        for y in 0..height {
            let row = y * width;
            let mut sum = 0.0f32;
            for x in 0..width.min(radius) {
                sum += input[row + x];
            }
            // x <= radius: no subtraction yet.
            for x in 0..left_end {
                let add = x + radius;
                if add < width {
                    sum += input[row + add];
                }
                output[row + x] = sum / divisor;
            }
            // Both neighbours inside the row: no branches.
            for x in left_end..middle_end.max(left_end) {
                sum += input[row + x + radius];
                sum -= input[row + x - radius - 1];
                output[row + x] = sum / divisor;
            }
            // Right border: the addition falls off the row.
            for x in middle_end.max(left_end)..width {
                if x > radius {
                    sum -= input[row + x - radius - 1];
                }
                output[row + x] = sum / divisor;
            }
        }
    } else {
        let left_end = (radius + 1).min(height);
        let middle_end = height.saturating_sub(radius);
        for x in 0..width {
            let mut sum = 0.0f32;
            for y in 0..height.min(radius) {
                sum += input[y * width + x];
            }
            for y in 0..left_end {
                let add = y + radius;
                if add < height {
                    sum += input[add * width + x];
                }
                output[y * width + x] = sum / divisor;
            }
            for y in left_end..middle_end.max(left_end) {
                sum += input[(y + radius) * width + x];
                sum -= input[(y - radius - 1) * width + x];
                output[y * width + x] = sum / divisor;
            }
            for y in middle_end.max(left_end)..height {
                if y > radius {
                    sum -= input[(y - radius - 1) * width + x];
                }
                output[y * width + x] = sum / divisor;
            }
        }
    }
}

pub(crate) fn blur_mask_pass(
    mask: &mut [f32],
    width: usize,
    height: usize,
    horizontal: bool,
    radius: usize,
) {
    if radius == 0 || width == 0 || height == 0 {
        return;
    }
    let mut output = BLUR_OUTPUT.with(|slot| std::mem::take(&mut *slot.borrow_mut()));
    output.clear();
    output.resize(mask.len(), 0.0);
    blur_pass_into(mask, &mut output, width, height, horizontal, radius);
    mask.copy_from_slice(&output);
    const MAX_RETAINED_F32S: usize = 1024 * 1024;
    if output.capacity() > MAX_RETAINED_F32S {
        output = Vec::new();
    }
    BLUR_OUTPUT.with(|slot| *slot.borrow_mut() = output);
}

/// Three-pass separable box blur (Gaussian approximation) with radius `r`.
pub(crate) fn blur_mask(mask: &mut [f32], width: usize, height: usize, blur: f32) {
    let radius = ((blur * 0.5).round() as usize).min(MAX_SHADOW_BLUR as usize);
    for _ in 0..3 {
        blur_mask_pass(mask, width, height, true, radius);
        blur_mask_pass(mask, width, height, false, radius);
    }
}

/// Expands (or shrinks) every corner radius by `spread` for box-shadow silhouettes.
pub(crate) fn expand_radii(radii: BorderRadii, spread: f32) -> BorderRadii {
    let expand = |radius: megi_display::Radius| megi_display::Radius {
        x: match radius.x {
            megi_display::Length::Px(px) => megi_display::Length::Px((px + spread).max(0.0)),
            other => other,
        },
        y: match radius.y {
            megi_display::Length::Px(px) => megi_display::Length::Px((px + spread).max(0.0)),
            other => other,
        },
    };
    BorderRadii {
        top_left: expand(radii.top_left),
        top_right: expand(radii.top_right),
        bottom_right: expand(radii.bottom_right),
        bottom_left: expand(radii.bottom_left),
    }
}

/// Padding needed around a shadow mask for the three-pass blur (#1716).
pub(crate) fn shadow_padding(blur: f32) -> i64 {
    (blur * 1.5).ceil() as i64 + 2
}

#[cfg(test)]
mod shadow_padding_tests {
    use super::*;

    #[test]
    fn shadow_padding_covers_three_pass_support() {
        // blur_mask uses three passes with radius round(blur * 0.5), so the
        // support is ~1.5 * blur, not 1 * blur.
        assert_eq!(shadow_padding(0.0), 2);
        assert_eq!(shadow_padding(64.0), 98);
    }
}
