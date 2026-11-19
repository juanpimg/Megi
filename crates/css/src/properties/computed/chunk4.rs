//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

impl ComputedStyle {
    /// Inherits properties from a parent style according to CSS cascading rules.
    pub fn inherit_from(&mut self, parent: &ComputedStyle) {
        self.color = parent.color;
        self.font_size = parent.font_size;
        self.font_weight = parent.font_weight;
        self.font_style = parent.font_style;
        self.font_family = parent.font_family.clone();
        self.line_height = parent.line_height;
        self.line_height_factor = parent.line_height_factor;
        self.text_align = parent.text_align;
        self.white_space = parent.white_space;
        self.text_transform = parent.text_transform;
        self.word_spacing = parent.word_spacing;
        self.letter_spacing = parent.letter_spacing;
        self.visibility = parent.visibility;
        self.list_style_type = parent.list_style_type;
        // `text-decoration` is *not* inherited (CSS Text Decoration 3 §2): decorations
        // propagate visually through descendant inline boxes but do not affect their
        // computed style, and layout/paint can walk the ancestor chain when needed.
        self.text_decoration = TextDecoration::NONE;
        // `text-shadow` is inherited (CSS Text Decoration 3 §3); `box-shadow` and
        // `border-radius` are not.
        self.text_shadows = parent.text_shadows.clone();
    }

    pub(crate) fn compute_font_size(val: &str, parent_size: f32, root_size: f32) -> f32 {
        let val = val.trim().to_ascii_lowercase();
        match val.as_str() {
            "xx-small" => 9.0,
            "x-small" => 10.0,
            "small" => 13.0,
            "medium" => 16.0,
            "large" => 18.0,
            "x-large" => 24.0,
            "xx-large" => 32.0,
            "smaller" => parent_size / 1.2,
            "larger" => parent_size * 1.2,
            _ => {
                if let Some(Length::Px(px)) = Length::parse(&val, parent_size, root_size) {
                    px
                } else if let Some(Length::Percent(pct)) =
                    Length::parse(&val, parent_size, root_size)
                {
                    (pct / 100.0) * parent_size
                } else {
                    parent_size
                }
            }
        }
    }

    pub(crate) fn parse_font_weight(val: &str) -> u16 {
        match val.trim().to_ascii_lowercase().as_str() {
            "normal" => 400,
            "bold" => 700,
            "bolder" => 700,
            "lighter" => 300,
            // CSS Fonts 4: numeric font-weight must be in 1..=1000 (#1676).
            other => other
                .parse::<u16>()
                .ok()
                .filter(|weight| (1..=1000).contains(weight))
                .unwrap_or(400),
        }
    }

    /// Returns the unitless `<number>` factor of a `line-height` declaration, if any.
    pub(crate) fn parse_line_height_factor(val: &str) -> Option<f32> {
        let val = val.trim();
        if val.eq_ignore_ascii_case("normal") || val.ends_with('%') {
            return None;
        }
        // #1671: reject NaN/infinity and negative numbers, not just `>= 0`.
        val.parse::<f32>()
            .ok()
            .filter(|factor| factor.is_finite() && *factor >= 0.0)
    }

    pub(crate) fn parse_line_height(val: &str, font_size: f32, root_font_size: f32) -> Option<f32> {
        let val = val.trim().to_ascii_lowercase();
        if val == "normal" {
            return Some(font_size * 1.2);
        }
        // #1671: a numeric line-height must be finite and non-negative; otherwise CSS falls
        // through to Length::parse, and an invalid value yields None (initial used).
        if let Ok(factor) = val.parse::<f32>()
            && factor.is_finite()
            && factor >= 0.0
        {
            return Some(factor * font_size);
        }
        // CSS Values 3 §5.1.2: `rem` is always the root element's font-size,
        // not the current element's (#1884: Wikipedia's `line-height: 3.3rem`).
        match Length::parse(&val, font_size, root_font_size) {
            Some(Length::Px(px)) => Some(px),
            // CSS 2.2 §10.8.1: percentage line-height computes against the element font-size.
            Some(Length::Percent(pct)) => Some(font_size * pct / 100.0),
            _ => None,
        }
    }

    pub(crate) fn compute_edges(
        entries: &[(&str, &Declaration)],
        names: &EdgeProperties,
        font_size: f32,
        root_font_size: f32,
    ) -> RectEdges<Length> {
        let mut edges = RectEdges::all(Length::ZERO);

        // 1. Shorthand (e.g. `margin: 8px` or `margin: 10px 20px`). Shorthands are normally
        //    expanded before the cascade; this keeps direct `compute` callers working.
        if let Some(decl) = find_declaration(entries, names.shorthand) {
            let mut parsed = [Length::ZERO; 4];
            let mut count = 0usize;
            for component in decl.raw_value.split_ascii_whitespace() {
                let Some(length) = Length::parse(component, font_size, root_font_size) else {
                    break;
                };
                if count < parsed.len() {
                    parsed[count] = length;
                }
                count += 1;
            }
            match count {
                1 => edges = RectEdges::all(parsed[0]),
                2 => {
                    edges.top = parsed[0];
                    edges.bottom = parsed[0];
                    edges.right = parsed[1];
                    edges.left = parsed[1];
                }
                3 => {
                    edges.top = parsed[0];
                    edges.right = parsed[1];
                    edges.left = parsed[1];
                    edges.bottom = parsed[2];
                }
                4 => {
                    edges.top = parsed[0];
                    edges.right = parsed[1];
                    edges.bottom = parsed[2];
                    edges.left = parsed[3];
                }
                _ => {}
            }
        }

        // 2. Logical block/inline properties (e.g. `margin-block: 1em`).
        if let Some(decl) = find_declaration(entries, names.block)
            && let Some(length) = Length::parse(&decl.raw_value, font_size, root_font_size)
        {
            edges.top = length;
            edges.bottom = length;
        }
        if let Some(decl) = find_declaration(entries, names.inline)
            && let Some(length) = Length::parse(&decl.raw_value, font_size, root_font_size)
        {
            edges.left = length;
            edges.right = length;
        }

        // 3. Specific longhands (e.g. `margin-top`), via a static name table.
        let mut sides = [edges.top, edges.right, edges.bottom, edges.left];
        for (index, name) in names.longhands.iter().enumerate() {
            if let Some(decl) = find_declaration(entries, name)
                && let Some(length) = Length::parse(&decl.raw_value, font_size, root_font_size)
            {
                sides[index] = length;
            }
        }

        RectEdges {
            top: sides[0],
            right: sides[1],
            bottom: sides[2],
            left: sides[3],
        }
    }

    /// Computes `filter` (with the `-webkit-filter` alias) and `mix-blend-mode`.
    ///
    /// Neither property is inherited (Filter Effects 1 §2 / Compositing 1 §5).
    /// Invalid values fall back to the initial values (`none`, `normal`).
    pub(crate) fn compute_effects(
        entries: &[(&str, &Declaration)],
        font_size: f32,
        root_font_size: f32,
        current_color: Color,
    ) -> (Vec<FilterFunction>, MixBlendMode, Vec<FilterFunction>) {
        // One linear scan over the cascaded declarations instead of five
        // `find_declaration` lookups per element (R173).
        let mut filter_decl = None;
        let mut webkit_filter_decl = None;
        let mut backdrop_decl = None;
        let mut webkit_backdrop_decl = None;
        let mut mix_decl = None;
        for (name, declaration) in entries {
            match *name {
                "filter" => filter_decl = Some(*declaration),
                "-webkit-filter" => webkit_filter_decl = Some(*declaration),
                "backdrop-filter" => backdrop_decl = Some(*declaration),
                "-webkit-backdrop-filter" => webkit_backdrop_decl = Some(*declaration),
                "mix-blend-mode" => mix_decl = Some(*declaration),
                _ => {}
            }
        }
        let parse_list = |declaration: Option<&Declaration>| {
            declaration
                .map(|decl| {
                    parse_filter_list(&decl.raw_value, font_size, root_font_size, current_color)
                })
                .unwrap_or_default()
        };
        let filter = parse_list(filter_decl.or(webkit_filter_decl));
        let backdrop_filter = {
            let parsed = parse_list(backdrop_decl);
            if parsed.is_empty() {
                parse_list(webkit_backdrop_decl)
            } else {
                parsed
            }
        };
        let mix_blend_mode = mix_decl
            .and_then(|decl| MixBlendMode::parse(&decl.raw_value))
            .unwrap_or_default();
        (filter, mix_blend_mode, backdrop_filter)
    }

    /// Computes `aspect-ratio`, `object-fit` and `object-position` (CSS Box Sizing
    /// 4 §5 / CSS Images 3 §5.5). None of them is inherited.
    pub(crate) fn compute_replaced_properties(
        entries: &[(&str, &Declaration)],
        font_size: f32,
        root_font_size: f32,
    ) -> (Option<AspectRatio>, ObjectFit, BackgroundPosition) {
        let aspect_ratio = find_declaration(entries, "aspect-ratio")
            .and_then(|decl| parse_aspect_ratio(&decl.raw_value));
        let object_fit = find_declaration(entries, "object-fit")
            .and_then(|decl| ObjectFit::parse(&decl.raw_value))
            .unwrap_or_default();
        let object_position = find_declaration(entries, "object-position")
            .and_then(|decl| BackgroundPosition::parse(&decl.raw_value, font_size, root_font_size))
            .unwrap_or(BackgroundPosition {
                x: Length::Percent(50.0),
                y: Length::Percent(50.0),
            });
        (aspect_ratio, object_fit, object_position)
    }

    /// Computes `border-radius`, `box-shadow` and `text-shadow` (CSS Backgrounds 3
    /// §5 and CSS Text Decoration 3 §3).
    ///
    /// Shorthands are expanded before the cascade; longhands take precedence over
    /// the `border-radius` shorthand for direct [`ComputedStyle::compute`] callers.
    /// Invalid components fall back to the initial value (`0`, `none`). The lists
    /// are capped at [`MAX_BOX_SHADOWS`]/[`MAX_TEXT_SHADOWS`] layers (documented
    /// paint limits) and blur radii are clamped at paint time.
    pub(crate) fn compute_decorations(
        entries: &[(&str, &Declaration)],
        font_size: f32,
        root_font_size: f32,
        current_color: Color,
        inherited_text_shadows: &[TextShadow],
    ) -> (BorderRadii, Vec<BoxShadow>, Vec<TextShadow>) {
        let shorthand_radius = find_declaration(entries, "border-radius").and_then(|decl| {
            parse_border_radius_shorthand(&decl.raw_value, font_size, root_font_size)
        });
        let mut radii = shorthand_radius.unwrap_or_default();
        for (corner, names) in [
            (&mut radii.top_left, ["border-top-left-radius"]),
            (&mut radii.top_right, ["border-top-right-radius"]),
            (&mut radii.bottom_right, ["border-bottom-right-radius"]),
            (&mut radii.bottom_left, ["border-bottom-left-radius"]),
        ] {
            if let Some(decl) = find_declaration(entries, names[0])
                && let Some(radius) = Radius::parse(&decl.raw_value, font_size, root_font_size)
            {
                *corner = radius;
            }
        }
        let box_shadows = find_declaration(entries, "box-shadow")
            .and_then(|decl| {
                parse_box_shadows(&decl.raw_value, font_size, root_font_size, current_color)
            })
            .unwrap_or_default();
        let text_shadows = find_declaration(entries, "text-shadow")
            .and_then(|decl| {
                parse_text_shadows(&decl.raw_value, font_size, root_font_size, current_color)
            })
            .unwrap_or_else(|| inherited_text_shadows.to_vec());
        (radii, box_shadows, text_shadows)
    }
}
