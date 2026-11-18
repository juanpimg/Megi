//! RGBA color value and its CSS parser.

/// RGBA Color representation (sRGB).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const TRANSPARENT: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
    pub const BLACK: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    pub const RED: Self = Self {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };
    pub const GREEN: Self = Self {
        r: 0,
        g: 128,
        b: 0,
        a: 255,
    };
    pub const BLUE: Self = Self {
        r: 0,
        g: 0,
        b: 255,
        a: 255,
    };

    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Parses a CSS color string (#rgb, #rrggbb, #rgba, #rrggbbaa, rgb(), named colors).
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }

        if let Some(hex) = s.strip_prefix('#') {
            return Self::from_hex(hex);
        }

        // `get(..n)` prevents a panic when byte n falls inside a multi-byte code point
        // of hostile input (e.g. "aééx"); the comparison is ASCII-only.
        if s.len() >= 5
            && s.get(..4)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("rgb("))
            && s.ends_with(')')
        {
            return Self::from_rgb_fn(&s[4..s.len() - 1]);
        }
        if s.len() >= 6
            && s.get(..5)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("rgba("))
            && s.ends_with(')')
        {
            return Self::from_rgb_fn(&s[5..s.len() - 1]);
        }

        if let Some(color) = crate::color::parse_modern(s) {
            return Some(color);
        }

        Self::from_named(s)
    }

    fn from_hex(hex: &str) -> Option<Self> {
        // Hex digits are ASCII (CSS Color 4 §5.2); checking before slicing keeps a value
        // like `aé` from splitting a UTF-8 code point in the byte ranges below.
        if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
                let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
                let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
                Some(Self::new(r, g, b, 255))
            }
            4 => {
                let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
                let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
                let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
                let a = u8::from_str_radix(&hex[3..4].repeat(2), 16).ok()?;
                Some(Self::new(r, g, b, a))
            }
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some(Self::new(r, g, b, 255))
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                Some(Self::new(r, g, b, a))
            }
            _ => None,
        }
    }

    fn from_rgb_fn(inner: &str) -> Option<Self> {
        let (rgb_parts, alpha_part): (Vec<&str>, Option<&str>) = if inner.contains(',') {
            if inner.contains('/') {
                return None;
            }
            let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
            match parts.len() {
                3 => (parts, None),
                4 => (parts[..3].to_vec(), Some(parts[3])),
                _ => return None,
            }
        } else if let Some((left, right)) = inner.split_once('/') {
            if right.contains('/') {
                return None;
            }
            let rgb: Vec<&str> = left.split_ascii_whitespace().collect();
            let alpha: Vec<&str> = right.split_ascii_whitespace().collect();
            if rgb.len() != 3 || alpha.len() != 1 {
                return None;
            }
            (rgb, Some(alpha[0]))
        } else {
            let rgb: Vec<&str> = inner.split_ascii_whitespace().collect();
            if rgb.len() != 3 {
                return None;
            }
            (rgb, None)
        };

        let parse_channel = |p: &str| -> Option<u8> {
            if let Some(pct) = p.strip_suffix('%') {
                let val: f32 = pct.trim().parse().ok()?;
                if !val.is_finite() {
                    return None;
                }
                Some((val.clamp(0.0, 100.0) / 100.0 * 255.0).round() as u8)
            } else {
                let val: f32 = p.parse().ok()?;
                if !val.is_finite() {
                    return None;
                }
                Some(val.clamp(0.0, 255.0).round() as u8)
            }
        };

        let r = parse_channel(rgb_parts[0])?;
        let g = parse_channel(rgb_parts[1])?;
        let b = parse_channel(rgb_parts[2])?;

        let a = if let Some(p) = alpha_part {
            if let Some(pct) = p.strip_suffix('%') {
                let val: f32 = pct.trim().parse().ok()?;
                if !val.is_finite() {
                    return None;
                }
                (val.clamp(0.0, 100.0) / 100.0 * 255.0).round() as u8
            } else {
                let val: f32 = p.parse().ok()?;
                if !val.is_finite() {
                    return None;
                }
                (val.clamp(0.0, 1.0) * 255.0).round() as u8
            }
        } else {
            255
        };

        Some(Self::new(r, g, b, a))
    }

    fn from_named(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "aliceblue" => Some(Self::new(240, 248, 255, 255)),
            "antiquewhite" => Some(Self::new(250, 235, 215, 255)),
            "aqua" => Some(Self::new(0, 255, 255, 255)),
            "aquamarine" => Some(Self::new(127, 255, 212, 255)),
            "azure" => Some(Self::new(240, 255, 255, 255)),
            "beige" => Some(Self::new(245, 245, 220, 255)),
            "bisque" => Some(Self::new(255, 228, 196, 255)),
            "black" => Some(Self::new(0, 0, 0, 255)),
            "blanchedalmond" => Some(Self::new(255, 235, 205, 255)),
            "blue" => Some(Self::new(0, 0, 255, 255)),
            "blueviolet" => Some(Self::new(138, 43, 226, 255)),
            "brown" => Some(Self::new(165, 42, 42, 255)),
            "burlywood" => Some(Self::new(222, 184, 135, 255)),
            "cadetblue" => Some(Self::new(95, 158, 160, 255)),
            "chartreuse" => Some(Self::new(127, 255, 0, 255)),
            "chocolate" => Some(Self::new(210, 105, 30, 255)),
            "coral" => Some(Self::new(255, 127, 80, 255)),
            "cornflowerblue" => Some(Self::new(100, 149, 237, 255)),
            "cornsilk" => Some(Self::new(255, 248, 220, 255)),
            "crimson" => Some(Self::new(220, 20, 60, 255)),
            "cyan" => Some(Self::new(0, 255, 255, 255)),
            "darkblue" => Some(Self::new(0, 0, 139, 255)),
            "darkcyan" => Some(Self::new(0, 139, 139, 255)),
            "darkgoldenrod" => Some(Self::new(184, 134, 11, 255)),
            "darkgray" => Some(Self::new(169, 169, 169, 255)),
            "darkgreen" => Some(Self::new(0, 100, 0, 255)),
            "darkgrey" => Some(Self::new(169, 169, 169, 255)),
            "darkkhaki" => Some(Self::new(189, 183, 107, 255)),
            "darkmagenta" => Some(Self::new(139, 0, 139, 255)),
            "darkolivegreen" => Some(Self::new(85, 107, 47, 255)),
            "darkorange" => Some(Self::new(255, 140, 0, 255)),
            "darkorchid" => Some(Self::new(153, 50, 204, 255)),
            "darkred" => Some(Self::new(139, 0, 0, 255)),
            "darksalmon" => Some(Self::new(233, 150, 122, 255)),
            "darkseagreen" => Some(Self::new(143, 188, 143, 255)),
            "darkslateblue" => Some(Self::new(72, 61, 139, 255)),
            "darkslategray" => Some(Self::new(47, 79, 79, 255)),
            "darkslategrey" => Some(Self::new(47, 79, 79, 255)),
            "darkturquoise" => Some(Self::new(0, 206, 209, 255)),
            "darkviolet" => Some(Self::new(148, 0, 211, 255)),
            "deeppink" => Some(Self::new(255, 20, 147, 255)),
            "deepskyblue" => Some(Self::new(0, 191, 255, 255)),
            "dimgray" => Some(Self::new(105, 105, 105, 255)),
            "dimgrey" => Some(Self::new(105, 105, 105, 255)),
            "dodgerblue" => Some(Self::new(30, 144, 255, 255)),
            "firebrick" => Some(Self::new(178, 34, 34, 255)),
            "floralwhite" => Some(Self::new(255, 250, 240, 255)),
            "forestgreen" => Some(Self::new(34, 139, 34, 255)),
            "fuchsia" => Some(Self::new(255, 0, 255, 255)),
            "gainsboro" => Some(Self::new(220, 220, 220, 255)),
            "ghostwhite" => Some(Self::new(248, 248, 255, 255)),
            "gold" => Some(Self::new(255, 215, 0, 255)),
            "goldenrod" => Some(Self::new(218, 165, 32, 255)),
            "gray" => Some(Self::new(128, 128, 128, 255)),
            "green" => Some(Self::new(0, 128, 0, 255)),
            "greenyellow" => Some(Self::new(173, 255, 47, 255)),
            "grey" => Some(Self::new(128, 128, 128, 255)),
            "honeydew" => Some(Self::new(240, 255, 240, 255)),
            "hotpink" => Some(Self::new(255, 105, 180, 255)),
            "indianred" => Some(Self::new(205, 92, 92, 255)),
            "indigo" => Some(Self::new(75, 0, 130, 255)),
            "ivory" => Some(Self::new(255, 255, 240, 255)),
            "khaki" => Some(Self::new(240, 230, 140, 255)),
            "lavender" => Some(Self::new(230, 230, 250, 255)),
            "lavenderblush" => Some(Self::new(255, 240, 245, 255)),
            "lawngreen" => Some(Self::new(124, 252, 0, 255)),
            "lemonchiffon" => Some(Self::new(255, 250, 205, 255)),
            "lightblue" => Some(Self::new(173, 216, 230, 255)),
            "lightcoral" => Some(Self::new(240, 128, 128, 255)),
            "lightcyan" => Some(Self::new(224, 255, 255, 255)),
            "lightgoldenrodyellow" => Some(Self::new(250, 250, 210, 255)),
            "lightgray" => Some(Self::new(211, 211, 211, 255)),
            "lightgreen" => Some(Self::new(144, 238, 144, 255)),
            "lightgrey" => Some(Self::new(211, 211, 211, 255)),
            "lightpink" => Some(Self::new(255, 182, 193, 255)),
            "lightsalmon" => Some(Self::new(255, 160, 122, 255)),
            "lightseagreen" => Some(Self::new(32, 178, 170, 255)),
            "lightskyblue" => Some(Self::new(135, 206, 250, 255)),
            "lightslategray" => Some(Self::new(119, 136, 153, 255)),
            "lightslategrey" => Some(Self::new(119, 136, 153, 255)),
            "lightsteelblue" => Some(Self::new(176, 196, 222, 255)),
            "lightyellow" => Some(Self::new(255, 255, 224, 255)),
            "lime" => Some(Self::new(0, 255, 0, 255)),
            "limegreen" => Some(Self::new(50, 205, 50, 255)),
            "linen" => Some(Self::new(250, 240, 230, 255)),
            "magenta" => Some(Self::new(255, 0, 255, 255)),
            "maroon" => Some(Self::new(128, 0, 0, 255)),
            "mediumaquamarine" => Some(Self::new(102, 205, 170, 255)),
            "mediumblue" => Some(Self::new(0, 0, 205, 255)),
            "mediumorchid" => Some(Self::new(186, 85, 211, 255)),
            "mediumpurple" => Some(Self::new(147, 112, 219, 255)),
            "mediumseagreen" => Some(Self::new(60, 179, 113, 255)),
            "mediumslateblue" => Some(Self::new(123, 104, 238, 255)),
            "mediumspringgreen" => Some(Self::new(0, 250, 154, 255)),
            "mediumturquoise" => Some(Self::new(72, 209, 204, 255)),
            "mediumvioletred" => Some(Self::new(199, 21, 133, 255)),
            "midnightblue" => Some(Self::new(25, 25, 112, 255)),
            "mintcream" => Some(Self::new(245, 255, 250, 255)),
            "mistyrose" => Some(Self::new(255, 228, 225, 255)),
            "moccasin" => Some(Self::new(255, 228, 181, 255)),
            "navajowhite" => Some(Self::new(255, 222, 173, 255)),
            "navy" => Some(Self::new(0, 0, 128, 255)),
            "oldlace" => Some(Self::new(253, 245, 230, 255)),
            "olive" => Some(Self::new(128, 128, 0, 255)),
            "olivedrab" => Some(Self::new(107, 142, 35, 255)),
            "orange" => Some(Self::new(255, 165, 0, 255)),
            "orangered" => Some(Self::new(255, 69, 0, 255)),
            "orchid" => Some(Self::new(218, 112, 214, 255)),
            "palegoldenrod" => Some(Self::new(238, 232, 170, 255)),
            "palegreen" => Some(Self::new(152, 251, 152, 255)),
            "paleturquoise" => Some(Self::new(175, 238, 238, 255)),
            "palevioletred" => Some(Self::new(219, 112, 147, 255)),
            "papayawhip" => Some(Self::new(255, 239, 213, 255)),
            "peachpuff" => Some(Self::new(255, 218, 185, 255)),
            "peru" => Some(Self::new(205, 133, 63, 255)),
            "pink" => Some(Self::new(255, 192, 203, 255)),
            "plum" => Some(Self::new(221, 160, 221, 255)),
            "powderblue" => Some(Self::new(176, 224, 230, 255)),
            "purple" => Some(Self::new(128, 0, 128, 255)),
            "rebeccapurple" => Some(Self::new(102, 51, 153, 255)),
            "red" => Some(Self::new(255, 0, 0, 255)),
            "rosybrown" => Some(Self::new(188, 143, 143, 255)),
            "royalblue" => Some(Self::new(65, 105, 225, 255)),
            "saddlebrown" => Some(Self::new(139, 69, 19, 255)),
            "salmon" => Some(Self::new(250, 128, 114, 255)),
            "sandybrown" => Some(Self::new(244, 164, 96, 255)),
            "seagreen" => Some(Self::new(46, 139, 87, 255)),
            "seashell" => Some(Self::new(255, 245, 238, 255)),
            "sienna" => Some(Self::new(160, 82, 45, 255)),
            "silver" => Some(Self::new(192, 192, 192, 255)),
            "skyblue" => Some(Self::new(135, 206, 235, 255)),
            "slateblue" => Some(Self::new(106, 90, 205, 255)),
            "slategray" => Some(Self::new(112, 128, 144, 255)),
            "slategrey" => Some(Self::new(112, 128, 144, 255)),
            "snow" => Some(Self::new(255, 250, 250, 255)),
            "springgreen" => Some(Self::new(0, 255, 127, 255)),
            "steelblue" => Some(Self::new(70, 130, 180, 255)),
            "tan" => Some(Self::new(210, 180, 140, 255)),
            "teal" => Some(Self::new(0, 128, 128, 255)),
            "thistle" => Some(Self::new(216, 191, 216, 255)),
            "tomato" => Some(Self::new(255, 99, 71, 255)),
            "turquoise" => Some(Self::new(64, 224, 208, 255)),
            "violet" => Some(Self::new(238, 130, 238, 255)),
            "webgray" => Some(Self::new(128, 128, 128, 255)),
            "webgreen" => Some(Self::new(0, 128, 0, 255)),
            "webmaroon" => Some(Self::new(128, 0, 0, 255)),
            "webpurple" => Some(Self::new(128, 0, 128, 255)),
            "wheat" => Some(Self::new(245, 222, 179, 255)),
            "white" => Some(Self::new(255, 255, 255, 255)),
            "whitesmoke" => Some(Self::new(240, 240, 240, 255)),
            "yellow" => Some(Self::new(255, 255, 0, 255)),
            "yellowgreen" => Some(Self::new(154, 205, 50, 255)),
            "transparent" => Some(Self::TRANSPARENT),
            _ => None,
        }
    }
}
