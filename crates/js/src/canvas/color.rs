//! Color serialization helper.

use super::*;

/// Serializes a color back to a CSS hex string (used by the `fillStyle`/`strokeStyle` getters).
pub fn rgba_to_css(color: Rgba) -> String {
    if color.a == 255 {
        format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
    } else {
        format!(
            "rgba({},{},{},{})",
            color.r,
            color.g,
            color.b,
            color.a as f32 / 255.0
        )
    }
}
