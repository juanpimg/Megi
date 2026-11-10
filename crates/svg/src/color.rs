//! Colour parsing for SVG paints: hex, `rgb()`/`rgba()`, named colours.

/// An 8-bit straight-alpha RGBA colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    /// Red channel (0-255).
    pub r: u8,
    /// Green channel (0-255).
    pub g: u8,
    /// Blue channel (0-255).
    pub b: u8,
    /// Alpha channel (0-255).
    pub a: u8,
}

impl Color {
    /// Fully opaque black.
    pub const BLACK: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    /// Fully transparent.
    pub const TRANSPARENT: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };

    /// Creates a colour from its channels.
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Multiplies the colour alpha by `factor` (0.0-1.0), clamping the result.
    pub fn with_opacity(self, factor: f32) -> Self {
        let alpha = (f32::from(self.a) * factor.clamp(0.0, 1.0)).round();
        Self {
            a: alpha.clamp(0.0, 255.0) as u8,
            ..self
        }
    }
}

/// Parses an SVG paint colour. Returns `None` for `none`/`url(#…)`/unparsable
/// values, which the caller treats as "no paint".
pub(crate) fn parse_color(raw: &str) -> Option<Color> {
    let value = raw.trim();
    if value.is_empty() {
        return None;
    }
    if let Some(hex) = value.strip_prefix('#') {
        return parse_hex(hex);
    }
    let lower = value.to_ascii_lowercase();
    if let Some(color) = named_color(&lower) {
        return Some(color);
    }
    let open = lower.find('(')?;
    let close = lower.rfind(')')?;
    if close <= open {
        return None;
    }
    let function = lower[..open].trim();
    let arguments = &lower[open + 1..close];
    match function {
        "rgb" | "rgba" => parse_rgb_function(arguments, function == "rgba"),
        _ => None,
    }
}

fn parse_hex(hex: &str) -> Option<Color> {
    let digits = hex.len();
    let short = |index: usize| -> Option<u8> {
        let ch = hex.chars().nth(index)?;
        let value = ch.to_digit(16)?;
        Some((value * 17) as u8)
    };
    let pair = |index: usize| -> Option<u8> {
        let slice = hex.get(index..index + 2)?;
        u8::from_str_radix(slice, 16).ok()
    };
    match digits {
        3 => Some(Color::rgba(short(0)?, short(1)?, short(2)?, 255)),
        4 => Some(Color::rgba(short(0)?, short(1)?, short(2)?, short(3)?)),
        6 => Some(Color::rgba(pair(0)?, pair(2)?, pair(4)?, 255)),
        8 => Some(Color::rgba(pair(0)?, pair(2)?, pair(4)?, pair(6)?)),
        _ => None,
    }
}

fn parse_rgb_function(arguments: &str, with_alpha: bool) -> Option<Color> {
    let mut parts: Vec<&str> = Vec::with_capacity(4);
    // Both `rgb(1, 2, 3)` and `rgb(1 2 3)` are valid in SVG 2; tolerate `/` for alpha.
    for part in arguments.split(|ch: char| ch == ',' || ch == '/' || ch.is_whitespace()) {
        if !part.is_empty() {
            parts.push(part);
        }
    }
    if parts.len() != if with_alpha { 4 } else { 3 } {
        return None;
    }
    let channel = |raw: &str| -> Option<u8> {
        let value = if let Some(percent) = raw.strip_suffix('%') {
            percent.trim().parse::<f32>().ok()? * 2.55
        } else {
            raw.parse::<f32>().ok()?
        };
        if !value.is_finite() {
            return None;
        }
        Some(value.clamp(0.0, 255.0).round() as u8)
    };
    let alpha = if with_alpha {
        let raw = parts[3];
        let value = if let Some(percent) = raw.strip_suffix('%') {
            percent.trim().parse::<f32>().ok()? / 100.0
        } else {
            raw.parse::<f32>().ok()?
        };
        if !value.is_finite() {
            return None;
        }
        (value.clamp(0.0, 1.0) * 255.0).round() as u8
    } else {
        255
    };
    Some(Color::rgba(
        channel(parts[0])?,
        channel(parts[1])?,
        channel(parts[2])?,
        alpha,
    ))
}

fn named_color(name: &str) -> Option<Color> {
    let color = match name {
        "black" => Color::rgba(0, 0, 0, 255),
        "silver" => Color::rgba(192, 192, 192, 255),
        "gray" | "grey" => Color::rgba(128, 128, 128, 255),
        "white" => Color::rgba(255, 255, 255, 255),
        "maroon" => Color::rgba(128, 0, 0, 255),
        "red" => Color::rgba(255, 0, 0, 255),
        "purple" => Color::rgba(128, 0, 128, 255),
        "fuchsia" | "magenta" => Color::rgba(255, 0, 255, 255),
        "green" => Color::rgba(0, 128, 0, 255),
        "lime" => Color::rgba(0, 255, 0, 255),
        "olive" => Color::rgba(128, 128, 0, 255),
        "yellow" => Color::rgba(255, 255, 0, 255),
        "navy" => Color::rgba(0, 0, 128, 255),
        "blue" => Color::rgba(0, 0, 255, 255),
        "teal" => Color::rgba(0, 128, 128, 255),
        "aqua" | "cyan" => Color::rgba(0, 255, 255, 255),
        "orange" => Color::rgba(255, 165, 0, 255),
        "transparent" => Color::TRANSPARENT,
        "currentcolor" => Color::BLACK,
        _ => return None,
    };
    Some(color)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_and_named_colours() {
        assert_eq!(parse_color("#f00"), Some(Color::rgba(255, 0, 0, 255)));
        assert_eq!(parse_color("#ff0000"), Some(Color::rgba(255, 0, 0, 255)));
        assert_eq!(parse_color("#ff000080"), Some(Color::rgba(255, 0, 0, 128)));
        assert_eq!(parse_color("blue"), Some(Color::rgba(0, 0, 255, 255)));
        assert_eq!(parse_color("none"), None);
        assert_eq!(parse_color("url(#grad)"), None);
    }

    #[test]
    fn test_rgb_functions() {
        assert_eq!(
            parse_color("rgb(255, 0, 0)"),
            Some(Color::rgba(255, 0, 0, 255))
        );
        assert_eq!(
            parse_color("rgb(100%, 0%, 0%)"),
            Some(Color::rgba(255, 0, 0, 255))
        );
        let half = parse_color("rgba(0, 0, 255, 0.5)").expect("rgba");
        assert_eq!((half.b, half.a), (255, 128));
    }
}
