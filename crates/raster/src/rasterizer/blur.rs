//! Deterministic blur, filters, blend modes and effect groups.
//! Split out of `rasterizer.rs` (Task 1.2).

use super::*;

/// One separable box-blur pass over a scalar mask (deterministic f32 arithmetic).
/// One box-blur pass over `input`, writing to `output`.
///
/// The loop is split into left/middle/right regions so the middle (the vast
/// majority of pixels) has no per-pixel bounds branches. The floating-point
/// accumulation order is exactly the same as the previous single-loop version,
/// so the resulting values — and therefore every rendered byte — are unchanged
/// (R181).
pub(super) fn blur_pass_into(
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

pub(super) fn blur_mask_pass(
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
pub(super) fn blur_mask(mask: &mut [f32], width: usize, height: usize, blur: f32) {
    let radius = ((blur * 0.5).round() as usize).min(MAX_SHADOW_BLUR as usize);
    for _ in 0..3 {
        blur_mask_pass(mask, width, height, true, radius);
        blur_mask_pass(mask, width, height, false, radius);
    }
}

/// Expands (or shrinks) every corner radius by `spread` for box-shadow silhouettes.
pub(super) fn expand_radii(radii: BorderRadii, spread: f32) -> BorderRadii {
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

// ---------------------------------------------------------------------------
// CSS filter / mix-blend-mode effect groups (R137)
// ---------------------------------------------------------------------------

pub(super) fn clamp_channel(value: f32) -> u8 {
    (value * 255.0).round().clamp(0.0, 255.0) as u8
}

pub(super) fn luma(color: Color) -> f32 {
    (0.2126 * f32::from(color.r) + 0.7152 * f32::from(color.g) + 0.0722 * f32::from(color.b))
        / 255.0
}

pub(super) fn mix(a: f32, b: f32, amount: f32) -> f32 {
    a + (b - a) * amount
}

/// Mix-blend-mode channel blend function (Compositing 1 §5): `B(Cb, Cs)`.
pub(super) fn blend_channel(mode: megi_display::MixBlendMode, backdrop: f32, source: f32) -> f32 {
    match mode {
        megi_display::MixBlendMode::Normal => source,
        megi_display::MixBlendMode::Multiply => backdrop * source,
        megi_display::MixBlendMode::Screen => 1.0 - (1.0 - backdrop) * (1.0 - source),
        megi_display::MixBlendMode::Overlay => {
            if backdrop <= 0.5 {
                2.0 * backdrop * source
            } else {
                1.0 - 2.0 * (1.0 - backdrop) * (1.0 - source)
            }
        }
        megi_display::MixBlendMode::Darken => backdrop.min(source),
        megi_display::MixBlendMode::Lighten => backdrop.max(source),
    }
}

/// Whether the filter only changes color/alpha per pixel (no spatial kernel).
pub(super) fn is_color_filter(filter: &megi_display::FilterFunction) -> bool {
    !matches!(
        filter,
        megi_display::FilterFunction::Blur(_) | megi_display::FilterFunction::DropShadow { .. }
    )
}

pub(super) fn apply_color_filter(pixel: &mut Color, filter: &megi_display::FilterFunction) {
    match *filter {
        megi_display::FilterFunction::Brightness(k) => {
            pixel.r = clamp_channel(f32::from(pixel.r) / 255.0 * k);
            pixel.g = clamp_channel(f32::from(pixel.g) / 255.0 * k);
            pixel.b = clamp_channel(f32::from(pixel.b) / 255.0 * k);
        }
        megi_display::FilterFunction::Contrast(k) => {
            let apply = |channel: u8| clamp_channel((f32::from(channel) / 255.0 - 0.5) * k + 0.5);
            pixel.r = apply(pixel.r);
            pixel.g = apply(pixel.g);
            pixel.b = apply(pixel.b);
        }
        megi_display::FilterFunction::Grayscale(amount) => {
            let amount = amount.clamp(0.0, 1.0);
            let gray = luma(*pixel);
            pixel.r = clamp_channel(mix(f32::from(pixel.r) / 255.0, gray, amount));
            pixel.g = clamp_channel(mix(f32::from(pixel.g) / 255.0, gray, amount));
            pixel.b = clamp_channel(mix(f32::from(pixel.b) / 255.0, gray, amount));
        }
        megi_display::FilterFunction::Invert(amount) => {
            let amount = amount.clamp(0.0, 1.0);
            let invert = |channel: u8| {
                clamp_channel(mix(
                    f32::from(channel) / 255.0,
                    1.0 - f32::from(channel) / 255.0,
                    amount,
                ))
            };
            pixel.r = invert(pixel.r);
            pixel.g = invert(pixel.g);
            pixel.b = invert(pixel.b);
        }
        megi_display::FilterFunction::Opacity(amount) => {
            let amount = amount.clamp(0.0, 1.0);
            pixel.a = (f32::from(pixel.a) * amount).round().clamp(0.0, 255.0) as u8;
        }
        megi_display::FilterFunction::Sepia(amount) => {
            let amount = amount.clamp(0.0, 1.0);
            let (r, g, b) = (
                f32::from(pixel.r) / 255.0,
                f32::from(pixel.g) / 255.0,
                f32::from(pixel.b) / 255.0,
            );
            let sepia_r = 0.393 * r + 0.769 * g + 0.189 * b;
            let sepia_g = 0.349 * r + 0.686 * g + 0.168 * b;
            let sepia_b = 0.272 * r + 0.534 * g + 0.131 * b;
            pixel.r = clamp_channel(mix(r, sepia_r, amount));
            pixel.g = clamp_channel(mix(g, sepia_g, amount));
            pixel.b = clamp_channel(mix(b, sepia_b, amount));
        }
        megi_display::FilterFunction::Saturate(amount) => {
            let amount = amount.max(0.0);
            let (r, g, b) = (
                f32::from(pixel.r) / 255.0,
                f32::from(pixel.g) / 255.0,
                f32::from(pixel.b) / 255.0,
            );
            let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            let matrix = |channel: f32| lum + amount * (channel - lum);
            pixel.r = clamp_channel(matrix(r));
            pixel.g = clamp_channel(matrix(g));
            pixel.b = clamp_channel(matrix(b));
        }
        megi_display::FilterFunction::HueRotate(degrees) => {
            let radians = degrees.to_radians();
            let cos = radians.cos();
            let sin = radians.sin();
            let matrix = [
                [
                    0.213 + cos * 0.787 - sin * 0.213,
                    0.715 - cos * 0.715 - sin * 0.715,
                    0.072 - cos * 0.072 + sin * 0.928,
                ],
                [
                    0.213 - cos * 0.213 + sin * 0.143,
                    0.715 + cos * 0.285 + sin * 0.140,
                    0.072 - cos * 0.072 - sin * 0.283,
                ],
                [
                    0.213 - cos * 0.213 - sin * 0.787,
                    0.715 - cos * 0.715 + sin * 0.715,
                    0.072 + cos * 0.928 + sin * 0.072,
                ],
            ];
            let (r, g, b) = (
                f32::from(pixel.r) / 255.0,
                f32::from(pixel.g) / 255.0,
                f32::from(pixel.b) / 255.0,
            );
            pixel.r = clamp_channel(matrix[0][0] * r + matrix[0][1] * g + matrix[0][2] * b);
            pixel.g = clamp_channel(matrix[1][0] * r + matrix[1][1] * g + matrix[1][2] * b);
            pixel.b = clamp_channel(matrix[2][0] * r + matrix[2][1] * g + matrix[2][2] * b);
        }
        megi_display::FilterFunction::Blur(_) | megi_display::FilterFunction::DropShadow { .. } => {
        }
    }
}

/// Applies a chain of color-only filters to a single solid color.
///
/// Used by the painter's inline fast path for boxes whose filtered subtree is
/// just a solid background (avoids an offscreen `EffectGroup`, R166).
pub fn filter_solid_color(color: Color, filters: &[megi_display::FilterFunction]) -> Color {
    let mut color = color;
    for filter in filters.iter().take(megi_display::MAX_FILTER_FUNCTIONS) {
        apply_color_filter(&mut color, filter);
    }
    color
}

/// Applies one `filter` function to every pixel of the offscreen buffer.
pub(super) fn apply_filter(buf: &mut Pixmap, filter: &megi_display::FilterFunction) {
    match *filter {
        megi_display::FilterFunction::Blur(radius) => blur_rgba(buf, radius),
        megi_display::FilterFunction::Brightness(k) => {
            for pixel in &mut buf.pixels {
                pixel.r = clamp_channel(f32::from(pixel.r) / 255.0 * k);
                pixel.g = clamp_channel(f32::from(pixel.g) / 255.0 * k);
                pixel.b = clamp_channel(f32::from(pixel.b) / 255.0 * k);
            }
        }
        megi_display::FilterFunction::Contrast(k) => {
            for pixel in &mut buf.pixels {
                let apply =
                    |channel: u8| clamp_channel((f32::from(channel) / 255.0 - 0.5) * k + 0.5);
                pixel.r = apply(pixel.r);
                pixel.g = apply(pixel.g);
                pixel.b = apply(pixel.b);
            }
        }
        megi_display::FilterFunction::Grayscale(amount) => {
            let amount = amount.clamp(0.0, 1.0);
            for pixel in &mut buf.pixels {
                let gray = luma(*pixel);
                pixel.r = clamp_channel(mix(f32::from(pixel.r) / 255.0, gray, amount));
                pixel.g = clamp_channel(mix(f32::from(pixel.g) / 255.0, gray, amount));
                pixel.b = clamp_channel(mix(f32::from(pixel.b) / 255.0, gray, amount));
            }
        }
        megi_display::FilterFunction::Invert(amount) => {
            let amount = amount.clamp(0.0, 1.0);
            for pixel in &mut buf.pixels {
                let invert = |channel: u8| {
                    clamp_channel(mix(
                        f32::from(channel) / 255.0,
                        1.0 - f32::from(channel) / 255.0,
                        amount,
                    ))
                };
                pixel.r = invert(pixel.r);
                pixel.g = invert(pixel.g);
                pixel.b = invert(pixel.b);
            }
        }
        megi_display::FilterFunction::Opacity(amount) => {
            let amount = amount.clamp(0.0, 1.0);
            for pixel in &mut buf.pixels {
                pixel.a = (f32::from(pixel.a) * amount).round().clamp(0.0, 255.0) as u8;
            }
        }
        megi_display::FilterFunction::Sepia(amount) => {
            let amount = amount.clamp(0.0, 1.0);
            for pixel in &mut buf.pixels {
                let (r, g, b) = (
                    f32::from(pixel.r) / 255.0,
                    f32::from(pixel.g) / 255.0,
                    f32::from(pixel.b) / 255.0,
                );
                let sepia_r = 0.393 * r + 0.769 * g + 0.189 * b;
                let sepia_g = 0.349 * r + 0.686 * g + 0.168 * b;
                let sepia_b = 0.272 * r + 0.534 * g + 0.131 * b;
                pixel.r = clamp_channel(mix(r, sepia_r, amount));
                pixel.g = clamp_channel(mix(g, sepia_g, amount));
                pixel.b = clamp_channel(mix(b, sepia_b, amount));
            }
        }
        megi_display::FilterFunction::Saturate(amount) => {
            let amount = amount.max(0.0);
            for pixel in &mut buf.pixels {
                let (r, g, b) = (
                    f32::from(pixel.r) / 255.0,
                    f32::from(pixel.g) / 255.0,
                    f32::from(pixel.b) / 255.0,
                );
                let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                let matrix = |channel: f32| lum + amount * (channel - lum);
                pixel.r = clamp_channel(matrix(r));
                pixel.g = clamp_channel(matrix(g));
                pixel.b = clamp_channel(matrix(b));
            }
        }
        megi_display::FilterFunction::HueRotate(degrees) => {
            let radians = degrees.to_radians();
            let cos = radians.cos();
            let sin = radians.sin();
            // CSS Filter Effects 1 §8.3 hue-rotate matrix.
            let matrix = [
                [
                    0.213 + cos * 0.787 - sin * 0.213,
                    0.715 - cos * 0.715 - sin * 0.715,
                    0.072 - cos * 0.072 + sin * 0.928,
                ],
                [
                    0.213 - cos * 0.213 + sin * 0.143,
                    0.715 + cos * 0.285 + sin * 0.140,
                    0.072 - cos * 0.072 - sin * 0.283,
                ],
                [
                    0.213 - cos * 0.213 - sin * 0.787,
                    0.715 - cos * 0.715 + sin * 0.715,
                    0.072 + cos * 0.928 + sin * 0.072,
                ],
            ];
            for pixel in &mut buf.pixels {
                let (r, g, b) = (
                    f32::from(pixel.r) / 255.0,
                    f32::from(pixel.g) / 255.0,
                    f32::from(pixel.b) / 255.0,
                );
                pixel.r = clamp_channel(matrix[0][0] * r + matrix[0][1] * g + matrix[0][2] * b);
                pixel.g = clamp_channel(matrix[1][0] * r + matrix[1][1] * g + matrix[1][2] * b);
                pixel.b = clamp_channel(matrix[2][0] * r + matrix[2][1] * g + matrix[2][2] * b);
            }
        }
        megi_display::FilterFunction::DropShadow {
            offset_x,
            offset_y,
            blur,
            color,
        } => {
            apply_drop_shadow(buf, offset_x, offset_y, blur, color);
        }
    }
}

/// Deterministic separable box blur over the RGBA channels (3 passes).
///
/// Planar scratch buffers are reused across calls (thread-local) so filtered
/// elements do not pay four fresh `Vec` allocations each (R166); the arithmetic
/// is unchanged and deterministic.
pub(super) fn blur_rgba(buf: &mut Pixmap, blur: f32) {
    let radius = ((blur * 0.5).round() as usize).min(64);
    if radius == 0 || buf.width == 0 || buf.height == 0 {
        return;
    }
    let width = buf.width as usize;
    let height = buf.height as usize;
    BLUR_PLANES.with(|planes| {
        let mut planes = planes.borrow_mut();
        for plane in planes.iter_mut() {
            plane.clear();
        }
        for index in 0..3 {
            planes[index].resize(width * height, 0.0);
        }
        // Fill RGB and detect uniform alpha in the same pass (common backdrop
        // case: fully opaque region); only non-uniform regions get a fourth
        // planar pass. The planes and arithmetic are unchanged (R231).
        let mut first_alpha = None;
        let mut uniform_alpha = true;
        for (index, pixel) in buf.pixels.iter().enumerate() {
            planes[0][index] = f32::from(pixel.r);
            planes[1][index] = f32::from(pixel.g);
            planes[2][index] = f32::from(pixel.b);
            match first_alpha {
                None => first_alpha = Some(pixel.a),
                Some(alpha) if alpha != pixel.a => uniform_alpha = false,
                Some(_) => {}
            }
        }
        let channels = if uniform_alpha { 3 } else { 4 };
        if channels == 4 {
            planes[3].clear();
            planes[3].resize(width * height, 0.0);
            for (index, pixel) in buf.pixels.iter().enumerate() {
                planes[3][index] = f32::from(pixel.a);
            }
        }
        let uniform_alpha = uniform_alpha.then_some(first_alpha.unwrap_or(255));
        let mut scratch = BLUR_OUTPUT.with(|slot| std::mem::take(&mut *slot.borrow_mut()));
        scratch.clear();
        scratch.resize(width * height, 0.0);
        for plane in planes.iter_mut().take(channels) {
            for _ in 0..3 {
                blur_pass_into(plane, &mut scratch, width, height, true, radius);
                blur_pass_into(&scratch, plane, width, height, false, radius);
            }
        }
        const MAX_RETAINED_F32S: usize = 1024 * 1024;
        if scratch.capacity() > MAX_RETAINED_F32S {
            scratch = Vec::new();
        }
        BLUR_OUTPUT.with(|slot| *slot.borrow_mut() = scratch);
        for (index, pixel) in buf.pixels.iter_mut().enumerate() {
            pixel.r = planes[0][index].round().clamp(0.0, 255.0) as u8;
            pixel.g = planes[1][index].round().clamp(0.0, 255.0) as u8;
            pixel.b = planes[2][index].round().clamp(0.0, 255.0) as u8;
            if let Some(alpha) = uniform_alpha {
                pixel.a = alpha;
            } else {
                pixel.a = planes[3][index].round().clamp(0.0, 255.0) as u8;
            }
        }
        for plane in planes.iter_mut() {
            if plane.capacity() > MAX_RETAINED_F32S {
                *plane = Vec::new();
            }
        }
    });
}

/// Paints a drop-shadow layer behind the current offscreen content.
pub(super) fn apply_drop_shadow(
    buf: &mut Pixmap,
    offset_x: f32,
    offset_y: f32,
    blur: f32,
    color: Color,
) {
    if color.a == 0 || buf.width == 0 || buf.height == 0 {
        return;
    }
    let width = buf.width as usize;
    let height = buf.height as usize;
    let mut mask = DROP_SHADOW_MASK.with(|slot| std::mem::take(&mut *slot.borrow_mut()));
    mask.clear();
    mask.resize(width * height, 0.0);
    let dx = offset_x.round() as i64;
    let dy = offset_y.round() as i64;
    for y in 0..height as i64 {
        for x in 0..width as i64 {
            let source_x = x - dx;
            let source_y = y - dy;
            if source_x < 0 || source_y < 0 || source_x >= width as i64 || source_y >= height as i64
            {
                continue;
            }
            let alpha = buf.pixels[(source_y as usize) * width + source_x as usize].a;
            mask[(y as usize) * width + x as usize] = f32::from(alpha) / 255.0;
        }
    }
    let radius = ((blur * 0.5).round() as usize).min(64);
    if radius > 0 {
        for _ in 0..3 {
            crate::rasterizer::blur_mask_pass(&mut mask, width, height, true, radius);
            crate::rasterizer::blur_mask_pass(&mut mask, width, height, false, radius);
        }
    }
    for (index, pixel) in buf.pixels.iter_mut().enumerate() {
        let shadow_alpha = (mask[index].clamp(0.0, 1.0) * f32::from(color.a)).round() as u8;
        if shadow_alpha == 0 {
            continue;
        }
        let shadow = Color::new(color.r, color.g, color.b, shadow_alpha);
        *pixel = blend_over(shadow, *pixel);
    }
    const MAX_RETAINED_F32S: usize = 1024 * 1024;
    if mask.capacity() > MAX_RETAINED_F32S {
        mask = Vec::new();
    }
    DROP_SHADOW_MASK.with(|slot| *slot.borrow_mut() = mask);
}

/// Composites an effect-group buffer over the destination using `blend`.
pub(super) fn composite_effect(
    pixmap: &mut Pixmap,
    buffer: &Pixmap,
    bx0: i64,
    by0: i64,
    blend: megi_display::MixBlendMode,
) {
    let stride = pixmap.width as usize;
    for y in 0..buffer.height {
        for x in 0..buffer.width {
            let src = buffer.pixels[(y * buffer.width + x) as usize];
            if src.a == 0 {
                continue;
            }
            let dest_x = bx0 + i64::from(x);
            let dest_y = by0 + i64::from(y);
            if dest_x < 0
                || dest_y < 0
                || dest_x >= i64::from(pixmap.width)
                || dest_y >= i64::from(pixmap.height)
            {
                continue;
            }
            let index = dest_y as usize * stride + dest_x as usize;
            let dest = pixmap.pixels[index];
            let dest_alpha = f32::from(dest.a) / 255.0;
            let source_alpha = f32::from(src.a) / 255.0;
            let blend_component = |backdrop: u8, source: u8| -> f32 {
                let cb = f32::from(backdrop) / 255.0;
                let cs = f32::from(source) / 255.0;
                let blended = blend_channel(blend, cb, cs);
                // Co = (1 - αb) * Cs + αb * B(Cb, Cs)
                (1.0 - dest_alpha) * cs + dest_alpha * blended
            };
            // Porter-Duff source-over (Compositing and Blending 1 §5):
            //   Co = αs·Cs' + αb·Cb·(1 − αs),  αo = αs + αb·(1 − αs),  C = Co/αo.
            // The backdrop term was not multiplied by `αb` and the result was not divided by
            // `αo`, so only an opaque backdrop came out right.
            let alpha = source_alpha + dest_alpha * (1.0 - source_alpha);
            let composite = |backdrop: u8, source: u8| -> u8 {
                if alpha <= 0.0 {
                    return 0;
                }
                let backdrop_channel = f32::from(backdrop) / 255.0;
                let premultiplied = blend_component(backdrop, source) * source_alpha
                    + backdrop_channel * dest_alpha * (1.0 - source_alpha);
                clamp_channel(premultiplied / alpha)
            };
            pixmap.pixels[index] = Color::new(
                composite(dest.r, src.r),
                composite(dest.g, src.g),
                composite(dest.b, src.b),
                clamp_channel(alpha),
            );
        }
    }
}

/// Renders an `EffectGroup`: offscreen buffer → filters → blend composite.
#[allow(clippy::too_many_arguments)]
pub(super) fn paint_effect_group(
    pixmap: &mut Pixmap,
    rect: Rect,
    filters: &[megi_display::FilterFunction],
    blend: megi_display::MixBlendMode,
    items: &[DisplayItem],
    cache: &mut TextCache,
    font_database: &FontDatabase,
    dx: f32,
    dy: f32,
    budget: &mut PaintBudget,
) {
    if items.is_empty() {
        return;
    }
    let rect = shift(rect, dx, dy);
    let Some(clip) = pixmap.clip_rect() else {
        return;
    };
    let x0 = rect.x.max(clip.x).max(0.0).floor() as i64;
    let y0 = rect.y.max(clip.y).max(0.0).floor() as i64;
    let x1 = ((rect.x + rect.width)
        .min(clip.x + clip.width)
        .min(pixmap.width as f32))
    .ceil() as i64;
    let y1 = ((rect.y + rect.height)
        .min(clip.y + clip.height)
        .min(pixmap.height as f32))
    .ceil() as i64;
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let width = (x1 - x0) as u32;
    let height = (y1 - y0) as u32;
    if !budget.enter(width, height) {
        // Depth/total-pixel cap: paint the contents unfiltered (documented DoS bound).
        budget.with_flattened(|budget| {
            paint_items(pixmap, items, cache, font_database, dx, dy, budget)
        });
        return;
    }
    let mut buffer = Pixmap::new(width, height);
    paint_items(
        &mut buffer,
        items,
        cache,
        font_database,
        dx - x0 as f32,
        dy - y0 as f32,
        budget,
    );
    let filters: Vec<megi_display::FilterFunction> = filters
        .iter()
        .take(megi_display::MAX_FILTER_FUNCTIONS)
        .copied()
        .collect();
    for filter in &filters {
        apply_filter(&mut buffer, filter);
    }
    composite_effect(pixmap, &buffer, x0, y0, blend);
    budget.leave(width, height);
}

// ---------------------------------------------------------------------------
// CSS backdrop-filter (R149)
// ---------------------------------------------------------------------------

/// Applies a `backdrop-filter` to the already-painted destination inside `rect`.
///
/// Color-only filter chains are applied **in place** (no copy or offscreen
/// buffer); spatial filters (blur/drop-shadow) use a reused thread-local pixmap
/// for the intersected region only (R166). Groups larger than the documented
/// area cap are left untouched (DoS bound).
pub(super) fn paint_backdrop_filter(
    pixmap: &mut Pixmap,
    rect: Rect,
    filters: &[megi_display::FilterFunction],
) {
    if filters.is_empty() {
        return;
    }
    let Some(clip) = pixmap.clip_rect() else {
        return;
    };
    let x0 = rect.x.max(clip.x).max(0.0).floor() as u32;
    let y0 = rect.y.max(clip.y).max(0.0).floor() as u32;
    let x1 = ((rect.x + rect.width)
        .min(clip.x + clip.width)
        .min(pixmap.width as f32))
    .ceil()
    .clamp(0.0, pixmap.width as f32) as u32;
    let y1 = ((rect.y + rect.height)
        .min(clip.y + clip.height)
        .min(pixmap.height as f32))
    .ceil()
    .clamp(0.0, pixmap.height as f32) as u32;
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let width = x1 - x0;
    let height = y1 - y0;
    if (width as usize).saturating_mul(height as usize) > MAX_ROUNDED_GROUP_PIXELS {
        return;
    }
    let filters: Vec<megi_display::FilterFunction> = filters
        .iter()
        .take(megi_display::MAX_FILTER_FUNCTIONS)
        .copied()
        .collect();
    let stride = pixmap.width as usize;

    // Fast path: color-only chains mutate the destination region in place.
    if filters.iter().all(is_color_filter) {
        for y in y0..y1 {
            let row = y as usize * stride;
            for x in x0..x1 {
                let mut pixel = pixmap.pixels[row + x as usize];
                for filter in &filters {
                    apply_color_filter(&mut pixel, filter);
                }
                pixmap.pixels[row + x as usize] = pixel;
            }
        }
        return;
    }

    // Spatial filters need an offscreen copy of the intersected backdrop.
    let mut buffer = BACKDROP_BUFFER.with(|slot| {
        slot.borrow_mut()
            .take()
            .unwrap_or_else(|| Pixmap::new(width, height))
    });
    buffer.width = width;
    buffer.height = height;
    buffer.pixels.clear();
    buffer.pixels.reserve((width * height) as usize);
    for y in y0..y1 {
        let row = y as usize * stride;
        for x in x0..x1 {
            buffer.pixels.push(pixmap.pixels[row + x as usize]);
        }
    }
    for filter in &filters {
        apply_filter(&mut buffer, filter);
    }
    let buffer_stride = buffer.width as usize;
    for y in 0..height {
        let dst_row = (y0 + y) as usize * stride;
        let src_row = y as usize * buffer_stride;
        for x in 0..width {
            let filtered = buffer.pixels[src_row + x as usize];
            let index = dst_row + (x0 + x) as usize;
            pixmap.pixels[index] = if filtered.a == 255 {
                filtered
            } else {
                blend_over(pixmap.pixels[index], filtered)
            };
        }
    }
    // Shrink the stashed buffer if it is much larger than the current region.
    if buffer.pixels.capacity() > (width * height * 4) as usize * 4 {
        buffer.pixels.shrink_to((width * height * 4) as usize);
    }
    BACKDROP_BUFFER.with(|slot| *slot.borrow_mut() = Some(buffer));
}
