//! Gradient color spaces and stop interpolation.

#[allow(unused_imports)]
use super::*;

// ---------------------------------------------------------------------------
// CSS gradients (R113)
// ---------------------------------------------------------------------------

/// Hard cap on the stops evaluated by the rasterizer (mirrors the CSS cap).
pub(crate) const MAX_GRADIENT_STOPS: usize = 64;

pub(crate) fn srgb_to_linear(channel: u8) -> f32 {
    let value = f32::from(channel) / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

pub(crate) fn linear_to_srgb_channel(value: f32) -> u8 {
    let value = value.clamp(0.0, 1.0);
    let encoded = if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Basic sRGB → OKLab conversion (Björn Ottosson's matrices, CSS Images 4 §12.1).
pub(crate) fn srgb_to_oklab(color: Color) -> [f32; 3] {
    let r = srgb_to_linear(color.r);
    let g = srgb_to_linear(color.g);
    let b = srgb_to_linear(color.b);
    let l = 0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b;
    let m = 0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b;
    let s = 0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b;
    let l_ = l.cbrt();
    let m_ = m.cbrt();
    let s_ = s.cbrt();
    [
        0.210_454_26 * l_ + 0.793_617_8 * m_ - 0.004_072_047 * s_,
        1.977_998_5 * l_ - 2.428_592_2 * m_ + 0.450_593_7 * s_,
        0.025_904_037 * l_ + 0.782_771_77 * m_ - 0.808_675_77 * s_,
    ]
}

pub(crate) fn oklab_to_srgb(lab: [f32; 3]) -> Color {
    let [lightness, a, b] = lab;
    let l_ = lightness + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = lightness - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = lightness - 0.089_484_18 * a - 1.291_485_5 * b;
    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;
    let r = 4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s;
    let g = -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s;
    let b = -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s;
    Color::new(
        linear_to_srgb_channel(r),
        linear_to_srgb_channel(g),
        linear_to_srgb_channel(b),
        0,
    )
}

/// Interpolates two colors in the requested gradient color space.
///
/// Alpha is always interpolated linearly (documented approximation: CSS
/// interpolates premultiplied colors; the default `srgb` path keeps the legacy
/// non-premultiplied channels byte-for-byte).
pub(crate) fn interpolate_color(
    start: Color,
    end: Color,
    factor: f32,
    space: megi_display::InterpolationSpace,
) -> Color {
    let factor = factor.clamp(0.0, 1.0);
    let alpha = (f32::from(start.a) + (f32::from(end.a) - f32::from(start.a)) * factor)
        .round()
        .clamp(0.0, 255.0) as u8;
    match space {
        megi_display::InterpolationSpace::Srgb => {
            let channel = |a: u8, b: u8| -> u8 {
                (f32::from(a) + (f32::from(b) - f32::from(a)) * factor)
                    .round()
                    .clamp(0.0, 255.0) as u8
            };
            Color::new(
                channel(start.r, end.r),
                channel(start.g, end.g),
                channel(start.b, end.b),
                alpha,
            )
        }
        megi_display::InterpolationSpace::SrgbLinear => {
            let channel = |a: u8, b: u8| -> u8 {
                let start = srgb_to_linear(a);
                let end = srgb_to_linear(b);
                linear_to_srgb_channel(start + (end - start) * factor)
            };
            Color::new(
                channel(start.r, end.r),
                channel(start.g, end.g),
                channel(start.b, end.b),
                alpha,
            )
        }
        megi_display::InterpolationSpace::Oklab => {
            let start_lab = srgb_to_oklab(start);
            let end_lab = srgb_to_oklab(end);
            let mixed = [
                start_lab[0] + (end_lab[0] - start_lab[0]) * factor,
                start_lab[1] + (end_lab[1] - start_lab[1]) * factor,
                start_lab[2] + (end_lab[2] - start_lab[2]) * factor,
            ];
            let mut color = oklab_to_srgb(mixed);
            color.a = alpha;
            color
        }
    }
}
