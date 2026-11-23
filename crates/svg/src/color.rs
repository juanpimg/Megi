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
        // #1656: CSS Color 3 functional hsl()/hsla() colors.
        "hsl" | "hsla" => parse_hsl_function(arguments, function == "hsla"),
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

/// Parses `hsl(h, s%, l%)` / `hsla(h, s%, l%, a)` (CSS Color 3 §4.2.4), with legacy
/// comma syntax and the modern space syntax. Hue wraps modulo 360; percentages clamp (#1656).
fn parse_hsl_function(arguments: &str, with_alpha: bool) -> Option<Color> {
    let parts: Vec<&str> = arguments
        .split(|c: char| c == ',' || c.is_ascii_whitespace())
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() != if with_alpha { 4 } else { 3 } {
        return None;
    }
    let parse_angle = |raw: &str| -> Option<f32> {
        let raw = raw.trim().to_ascii_lowercase();
        let angle = if let Some(deg) = raw.strip_suffix("deg") {
            deg.trim().parse::<f32>().ok()?
        } else if let Some(rad) = raw.strip_suffix("rad") {
            rad.trim().parse::<f32>().ok()?.to_degrees()
        } else if let Some(turn) = raw.strip_suffix("turn") {
            turn.trim().parse::<f32>().ok()? * 360.0
        } else {
            raw.parse::<f32>().ok()?
        };
        angle.is_finite().then_some(angle)
    };
    let hue = parse_angle(parts[0])?.rem_euclid(360.0);
    let saturation = parts[1]
        .strip_suffix('%')?
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|v| v.is_finite())?
        .clamp(0.0, 100.0)
        / 100.0;
    let lightness = parts[2]
        .strip_suffix('%')?
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|v| v.is_finite())?
        .clamp(0.0, 100.0)
        / 100.0;
    let alpha = if with_alpha {
        parts[3]
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite())?
            .clamp(0.0, 1.0)
    } else {
        1.0
    };
    // HSL -> RGB (CSS Color 3 algorithm).
    let c = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let m = lightness - c / 2.0;
    let (r, g, b) = match hue as u32 / 60 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let byte = |value: f32| ((value + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Some(Color::rgba(
        byte(r),
        byte(g),
        byte(b),
        (alpha * 255.0).round().clamp(0.0, 255.0) as u8,
    ))
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
        // `currentcolor` is handled by parse_paint, which resolves it against the
        // inherited `color` property (#1276).
        "currentcolor" => return None,
        // Remaining CSS Color Module Level 4 named colors (#1655). Values are the normative
        // sRGB hex definitions; synonyms map to the same color.
        other => return parse_extended_named_color(other),
    };
    Some(color)
}

/// Parses the remaining CSS named colors (all 148 definitions) from a static name->hex table.
fn parse_extended_named_color(name: &str) -> Option<Color> {
    let hex = match name {
        "aliceblue" => "f0f8ff",
        "antiquewhite" => "faebd7",
        "aquamarine" => "7fffd4",
        "azure" => "f0ffff",
        "beige" => "f5f5dc",
        "bisque" => "ffe4c4",
        "blanchedalmond" => "ffebcd",
        "blueviolet" => "8a2be2",
        "brown" => "a52a2a",
        "burlywood" => "deb887",
        "cadetblue" => "5f9ea0",
        "chartreuse" => "7fff00",
        "chocolate" => "d2691e",
        "coral" => "ff7f50",
        "cornflowerblue" => "6495ed",
        "cornsilk" => "fff8dc",
        "crimson" => "dc143c",
        "darkcyan" => "008b8b",
        "darkblue" => "00008b",
        "darkgoldenrod" => "b8860b",
        "darkgray" | "darkgrey" => "a9a9a9",
        "darkgreen" => "006400",
        "darkkhaki" => "bdb76b",
        "darkmagenta" => "8b008b",
        "darkolivegreen" => "556b2f",
        "darkorange" => "ff8c00",
        "darkorchid" => "9932cc",
        "darkred" => "8b0000",
        "darksalmon" => "e9967a",
        "darkseagreen" => "8fbc8f",
        "darkslateblue" => "483d8b",
        "darkslategray" | "darkslategrey" => "2f4f4f",
        "darkturquoise" => "00ced1",
        "darkviolet" => "9400d3",
        "deeppink" => "ff1493",
        "deepskyblue" => "00bfff",
        "dimgray" | "dimgrey" => "696969",
        "dodgerblue" => "1e90ff",
        "firebrick" => "b22222",
        "floralwhite" => "fffaf0",
        "forestgreen" => "228b22",
        "gainsboro" => "dcdcdc",
        "ghostwhite" => "f8f8ff",
        "gold" => "ffd700",
        "goldenrod" => "daa520",
        "greenyellow" => "adff2f",
        "honeydew" => "f0fff0",
        "hotpink" => "ff69b4",
        "indianred" => "cd5c5c",
        "indigo" => "4b0082",
        "ivory" => "fffff0",
        "khaki" => "f0e68c",
        "lavender" => "e6e6fa",
        "lavenderblush" => "fff0f5",
        "lawngreen" => "7cfc00",
        "lemonchiffon" => "fffacd",
        "lightblue" => "add8e6",
        "lightcoral" => "f08080",
        "lightcyan" => "e0ffff",
        "lightgoldenrodyellow" => "fafad2",
        "lightgreen" => "90ee90",
        "lightgray" | "lightgrey" => "d3d3d3",
        "lightpink" => "ffb6c1",
        "lightsalmon" => "ffa07a",
        "lightseagreen" => "20b2aa",
        "lightskyblue" => "87cefa",
        "lightslategray" | "lightslategrey" => "778899",
        "lightsteelblue" => "b0c4de",
        "lightyellow" => "ffffe0",
        "limegreen" => "32cd32",
        "linen" => "faf0e6",
        "mediumaquamarine" => "66cdaa",
        "mediumblue" => "0000cd",
        "mediumorchid" => "ba55d3",
        "mediumpurple" => "9370db",
        "mediumseagreen" => "3cb371",
        "mediumslateblue" => "7b68ee",
        "mediumspringgreen" => "00fa9a",
        "mediumturquoise" => "48d1cc",
        "mediumvioletred" => "c71585",
        "midnightblue" => "191970",
        "mintcream" => "f5fffa",
        "mistyrose" => "ffe4e1",
        "moccasin" => "ffe4b5",
        "navajowhite" => "ffdead",
        "oldlace" => "fdf5e6",
        "olivedrab" => "6b8e23",
        "orangered" => "ff4500",
        "orchid" => "da70d6",
        "palegoldenrod" => "eee8aa",
        "palegreen" => "98fb98",
        "paleturquoise" => "afeeee",
        "palevioletred" => "db7093",
        "papayawhip" => "ffefd5",
        "peachpuff" => "ffdab9",
        "peru" => "cd853f",
        "pink" => "ffc0cb",
        "plum" => "dda0dd",
        "powderblue" => "b0e0e6",
        "rebeccapurple" => "663399",
        "rosybrown" => "bc8f8f",
        "royalblue" => "4169e1",
        "saddlebrown" => "8b4513",
        "salmon" => "fa8072",
        "sandybrown" => "f4a460",
        "seagreen" => "2e8b57",
        "seashell" => "fff5ee",
        "sienna" => "a0522d",
        "skyblue" => "87ceeb",
        "slateblue" => "6a5acd",
        "slategray" | "slategrey" => "708090",
        "snow" => "fffafa",
        "springgreen" => "00ff7f",
        "steelblue" => "4682b4",
        "tan" => "d2b48c",
        "thistle" => "d8bfd8",
        "tomato" => "ff6347",
        "turquoise" => "40e0d0",
        "violet" => "ee82ee",
        "wheat" => "f5deb3",
        "whitesmoke" => "f5f5f5",
        "yellowgreen" => "9acd32",
        _ => return None,
    };
    let byte = |a: usize, b: usize| u8::from_str_radix(&hex[a..b], 16).ok();
    Some(Color::rgba(byte(0, 2)?, byte(2, 4)?, byte(4, 6)?, 255))
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

    #[test]
    fn test_hsl_rejects_non_finite() {
        assert_eq!(parse_color("hsl(NaN, 50%, 50%)"), None);
        assert_eq!(parse_color("hsl(inf, 50%, 50%)"), None);
        assert_eq!(parse_color("hsl(0, NaN%, 50%)"), None);
        assert_eq!(parse_color("hsl(0, 50%, inf%)"), None);
        assert_eq!(parse_color("hsla(0, 50%, 50%, NaN)"), None);
        assert_eq!(parse_color("hsla(0, 50%, 50%, inf)"), None);
    }
}
