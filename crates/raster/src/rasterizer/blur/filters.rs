//! CSS filter and mix-blend-mode helpers.

#[allow(unused_imports)]
use super::*;

// ---------------------------------------------------------------------------
// CSS filter / mix-blend-mode effect groups (R137)
// ---------------------------------------------------------------------------

pub(crate) fn clamp_channel(value: f32) -> u8 {
    (value * 255.0).round().clamp(0.0, 255.0) as u8
}

pub(crate) fn luma(color: Color) -> f32 {
    (0.2126 * f32::from(color.r) + 0.7152 * f32::from(color.g) + 0.0722 * f32::from(color.b))
        / 255.0
}

pub(crate) fn mix(a: f32, b: f32, amount: f32) -> f32 {
    a + (b - a) * amount
}

/// Mix-blend-mode channel blend function (Compositing 1 §5): `B(Cb, Cs)`.
pub(crate) fn blend_channel(mode: megi_display::MixBlendMode, backdrop: f32, source: f32) -> f32 {
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
pub(crate) fn is_color_filter(filter: &megi_display::FilterFunction) -> bool {
    !matches!(
        filter,
        megi_display::FilterFunction::Blur(_) | megi_display::FilterFunction::DropShadow { .. }
    )
}

pub(crate) fn apply_color_filter(pixel: &mut Color, filter: &megi_display::FilterFunction) {
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
pub(crate) fn apply_filter(buf: &mut Pixmap, filter: &megi_display::FilterFunction) {
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
