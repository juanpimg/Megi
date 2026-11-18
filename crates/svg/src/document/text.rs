//! Submodule of [`super`]: text extracted from the document model.

use super::*;

impl<'a> Builder<'a> {
    /// Parses a `<text>` element: direct text content plus `<tspan>` text (tspan-specific
    /// positioning is ignored, documented limitation).
    pub(crate) fn push_text(
        &mut self,
        element: &XmlElement,
        transform: Transform,
        style: &Style,
        group_opacity: f32,
    ) -> Result<(), SvgError> {
        if self.texts.len() >= MAX_TEXTS {
            return Err(SvgError::Limit("too many <text> elements".to_owned()));
        }
        let mut text_path = None;
        let text_path_child = element.children.iter().find_map(|child| {
            let XmlNode::Element(child) = child else {
                return None;
            };
            (child.local_name_lower() == "textpath").then_some(child)
        });
        let mut text = String::new();
        if let Some(path_child) = text_path_child {
            collect_text(path_child, &mut text);
            if let Some(href) = path_child
                .attribute("href")
                .or_else(|| path_child.attribute("xlink:href"))
            {
                let id = href.trim().trim_start_matches('#');
                if let Some(path_element) = self.defs.get(id)
                    && let Some(data) = path_element.attribute("d")
                {
                    let limits = self.limits;
                    let subpaths = parse_path_data(data, limits, &mut self.segments_used)?;
                    let raw = path_child.attribute("startOffset").unwrap_or("0").trim();
                    let (start_offset, percent) = match raw.strip_suffix('%') {
                        Some(value) => (value.trim().parse::<f32>().unwrap_or(0.0) / 100.0, true),
                        None => (raw.parse::<f32>().unwrap_or(0.0), false),
                    };
                    text_path = Some(crate::text::TextPathSpec {
                        subpaths,
                        start_offset: if start_offset.is_finite() {
                            start_offset
                        } else {
                            0.0
                        },
                        percent,
                        side_right: path_child
                            .attribute("side")
                            .is_some_and(|value| value.trim().eq_ignore_ascii_case("right")),
                    });
                }
            }
        } else {
            collect_text(element, &mut text);
        }
        if text.is_empty() {
            return Ok(());
        }
        if text.chars().count() > MAX_TEXT_CHARS {
            return Err(SvgError::Limit("SVG text is too long".to_owned()));
        }
        let x = first_number(element.attribute("x"));
        let y = first_number(element.attribute("y"));
        let dx = first_number(element.attribute("dx"));
        let dy = first_number(element.attribute("dy"));
        let anchor = TextAnchor::parse(
            style
                .text_anchor
                .as_deref()
                .or_else(|| element.attribute("text-anchor")),
        );
        let font_family = style
            .font_family
            .clone()
            .or_else(|| element.attribute("font-family").map(str::to_owned))
            .unwrap_or_else(|| "sans-serif".to_owned());
        let font_size = style
            .font_size
            .or_else(|| {
                parse_length(element.attribute("font-size"), 16.0)
                    .filter(|size| size.is_finite() && *size > 0.0)
            })
            .unwrap_or(16.0);
        let font_weight = style
            .font_weight
            .or_else(|| {
                match element
                    .attribute("font-weight")
                    .map(|value| value.trim().to_ascii_lowercase())
                    .as_deref()
                {
                    Some("bold") => Some(700),
                    Some("normal") => Some(400),
                    Some(value) => value.parse::<u16>().ok(),
                    None => None,
                }
            })
            .unwrap_or(400);
        let italic = style
            .font_style
            .as_deref()
            .or_else(|| element.attribute("font-style"))
            .is_some_and(|value| {
                value.eq_ignore_ascii_case("italic") || value.eq_ignore_ascii_case("oblique")
            });
        let letter_spacing = style
            .letter_spacing
            .or_else(|| parse_length(element.attribute("letter-spacing"), 0.0))
            .unwrap_or(0.0);
        let text_length = parse_length(element.attribute("textLength"), 0.0)
            .filter(|value| value.is_finite() && *value >= 0.0);
        let spacing_and_glyphs = element
            .attribute("lengthAdjust")
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("spacingAndGlyphs"));
        let opacity = group_opacity * parse_opacity_attribute(element, "opacity").unwrap_or(1.0);
        let mut spec = TextSpec {
            text,
            x,
            y,
            dx,
            dy,
            anchor,
            font_family,
            font_size,
            font_weight,
            italic,
            letter_spacing,
            fill: style.fill,
            fill_opacity: style.fill_opacity,
            stroke: style.stroke,
            stroke_opacity: style.stroke_opacity,
            stroke_width: style.stroke_width,
            opacity: opacity.clamp(0.0, 1.0),
            transform,
            bbox: (0.0, 0.0, 0.0, 0.0),
            text_path,
            text_length,
            spacing_and_glyphs,
        };
        spec.bbox = spec.local_bbox();
        self.texts.push(spec);
        Ok(())
    }
}

/// Concatenates the text content of an element (including nested `<tspan>`s).
fn collect_text(element: &XmlElement, out: &mut String) {
    for child in &element.children {
        match child {
            XmlNode::Text(text) => out.push_str(text),
            XmlNode::Element(child_element) => collect_text(child_element, out),
        }
    }
}
