//! Submodule of [`super`]: style extracted from the document model.

use super::*;

/// A paint value: solid colour, gradient reference (index into
/// [`Document::gradients`]) or explicit `none`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Paint {
    None,
    Solid(Color),
    /// Resolved to the inherited `color` once the whole style is applied (#1276).
    CurrentColor,
    Gradient(u32),
    Pattern(u32),
}

impl Default for Style {
    fn default() -> Self {
        Self {
            color: Color::BLACK,
            display: true,
            visibility: true,
            fill: Paint::Solid(Color::BLACK),
            stroke: Paint::None,
            fill_opacity: 1.0,
            stroke_opacity: 1.0,
            stroke_width: 1.0,
            linecap: LineCap::Butt,
            linejoin: LineJoin::Miter,
            miter_limit: 4.0,
            fill_rule: FillRule::NonZero,
            clip_path: None,
            mask: None,
            filter: None,
            font_family: None,
            font_size: None,
            font_weight: None,
            font_style: None,
            letter_spacing: None,
            text_anchor: None,
        }
    }
}

impl Style {
    /// Resolves `currentColor` paints against the inherited `color` property (#1276).
    pub(crate) fn resolve_current_color(&mut self) {
        if self.fill == Paint::CurrentColor {
            self.fill = Paint::Solid(self.color);
        }
        if self.stroke == Paint::CurrentColor {
            self.stroke = Paint::Solid(self.color);
        }
    }

    /// Applies, in cascade order, the element's presentation attributes, the
    /// matching `<style>` rules (specificity then source order) and finally the
    /// inline `style="..."` attribute (highest priority).
    pub(crate) fn apply(
        &mut self,
        element: &XmlElement,
        rules: &[StyleRule],
        ancestors: &[&XmlElement],
        gradients: &HashMap<String, u32>,
        patterns: &HashMap<String, u32>,
        selector_budget: &mut SelectorBudget,
    ) -> Result<(), SvgError> {
        for name in PRESENTATION_PROPERTIES {
            if let Some(value) = element.attribute(name) {
                self.apply_declaration(name, value, gradients, patterns);
            }
        }
        for declaration in matching_declarations(rules, element, ancestors, selector_budget)? {
            self.apply_declaration(declaration.name, declaration.value, gradients, patterns);
        }
        if let Some(inline) = element.attribute("style") {
            for declaration in parse_declarations(inline) {
                self.apply_declaration(&declaration.name, &declaration.value, gradients, patterns);
            }
        }
        Ok(())
    }

    /// Applies one presentation/CSS declaration to this style.
    fn apply_declaration(
        &mut self,
        name: &str,
        value: &str,
        gradients: &HashMap<String, u32>,
        patterns: &HashMap<String, u32>,
    ) {
        match name {
            "color" => {
                if let Some(color) = parse_color(value) {
                    self.color = color;
                }
            }
            "display" => self.display = !value.trim().eq_ignore_ascii_case("none"),
            "visibility" => {
                let value = value.trim().to_ascii_lowercase();
                // #1657: `inherit` keeps the parent's value (the child style was cloned from it).
                match value.as_str() {
                    "inherit" => {}
                    "visible" => self.visibility = true,
                    _ => self.visibility = false, // hidden | collapse
                }
            }
            "fill" => self.fill = parse_paint(value, gradients, patterns),
            "stroke" => self.stroke = parse_paint(value, gradients, patterns),
            "fill-opacity" => {
                if let Some(opacity) = parse_opacity(value) {
                    self.fill_opacity = opacity;
                }
            }
            "stroke-opacity" => {
                if let Some(opacity) = parse_opacity(value) {
                    self.stroke_opacity = opacity;
                }
            }
            "stroke-width" => {
                if let Some(width) = parse_length(Some(value), 0.0)
                    && width.is_finite()
                    && width >= 0.0
                {
                    self.stroke_width = width;
                }
            }
            "stroke-linecap" => {
                self.linecap = match value.to_ascii_lowercase().as_str() {
                    "round" => LineCap::Round,
                    "square" => LineCap::Square,
                    _ => LineCap::Butt,
                };
            }
            "stroke-linejoin" => {
                self.linejoin = match value.to_ascii_lowercase().as_str() {
                    "round" => LineJoin::Round,
                    "bevel" => LineJoin::Bevel,
                    _ => LineJoin::Miter,
                };
            }
            "stroke-miterlimit" => {
                if let Ok(limit) = value.trim().parse::<f32>()
                    && limit.is_finite()
                    && limit >= 1.0
                {
                    self.miter_limit = limit;
                }
            }
            "fill-rule" => {
                self.fill_rule = if value.eq_ignore_ascii_case("evenodd") {
                    FillRule::EvenOdd
                } else {
                    FillRule::NonZero
                };
            }
            "clip-path" | "mask" | "filter" => {
                let reference = parse_url_reference(value);
                match name {
                    "clip-path" => self.clip_path = reference,
                    "mask" => self.mask = reference,
                    _ => self.filter = reference,
                }
            }
            "font-family" => {
                self.font_family = Some(value.trim().to_owned());
            }
            "font-size" => {
                if let Some(size) = parse_length(Some(value), 16.0)
                    && size.is_finite()
                    && size > 0.0
                {
                    self.font_size = Some(size);
                }
            }
            "font-weight" => {
                let weight = match value.trim().to_ascii_lowercase().as_str() {
                    "bold" => 700,
                    "normal" => 400,
                    other => other.parse::<u16>().unwrap_or(400),
                };
                self.font_weight = Some(weight);
            }
            "font-style" => {
                self.font_style = Some(value.trim().to_owned());
            }
            "letter-spacing" => {
                if let Some(spacing) = parse_length(Some(value), 0.0)
                    && spacing.is_finite()
                {
                    self.letter_spacing = Some(spacing);
                }
            }
            "text-anchor" => {
                self.text_anchor = Some(value.trim().to_owned());
            }
            _ => {}
        }
    }
}

/// Presentation attributes applied (with specificity 0) before `<style>` rules.
const PRESENTATION_PROPERTIES: &[&str] = &[
    // `color` must be applied before `fill`/`stroke` so `currentColor` can resolve.
    "color",
    "display",
    "visibility",
    "fill",
    "stroke",
    "fill-opacity",
    "stroke-opacity",
    "stroke-width",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "fill-rule",
    "clip-path",
    "mask",
    "filter",
    "font-family",
    "font-size",
    "font-weight",
    "font-style",
    "letter-spacing",
    "text-anchor",
];

/// Concatenates the text of every `<style>` element (CSS subset supported by
/// [`crate::css`]); bounded by [`crate::css::MAX_STYLESHEET_BYTES`].
pub(crate) fn collect_style_text(root: &XmlElement) -> String {
    let mut text = String::new();
    let mut stack = vec![root];
    while let Some(element) = stack.pop() {
        if local_name(&element.name).eq_ignore_ascii_case("style") {
            let is_css = element.attribute("type").is_none_or(|kind| {
                let kind = kind.trim();
                kind.is_empty() || kind.eq_ignore_ascii_case("text/css")
            });
            if is_css {
                for child in &element.children {
                    if let XmlNode::Text(block) = child {
                        if text.len() + block.len() + 1 > crate::css::MAX_STYLESHEET_BYTES {
                            return text;
                        }
                        text.push_str(block);
                        text.push('\n');
                    }
                }
            }
        }
        for child in element.children.iter().rev() {
            if let XmlNode::Element(child) = child {
                stack.push(child);
            }
        }
    }
    text
}
