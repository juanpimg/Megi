//! `@page` descriptors (CSS Paged Media 3 §4) for the PDF writer.
//!
//! The rendered raster keeps its original CSS-pixel coordinates (this engine
//! maps 1 CSS px to 1 PDF unit, ISO 32000-1 §7.7.3.5); `@page` changes the
//! `/MediaBox` and insets the selectable text layer by the page margins.
//!
//! Supported descriptors: `size` (one/two lengths, `A4`, `letter`, optional
//! `portrait`/`landscape`) and `margin` (1–4 value shorthand plus the four
//! longhands). Lengths use the CSS parser's absolute units (`px`, `pt`, `em`,
//! `rem`); percentages and `auto` are ignored. Only the **first** `@page` rule
//! found in document order across inline `<style>` elements is honored: named
//! pages, `@page` inside conditional at-rules and linked stylesheets are out of
//! scope, as the render output exposes the DOM but not the resolved cascade.

use std::fmt::Write;

use megi_webview::core::Document;
use megi_webview::css::{
    ComponentValue, CssToken, Declaration, Length, Rule, parse_list_of_declarations,
    parse_stylesheet,
};

/// A4 (210mm × 297mm) and US Letter (8.5in × 11in) in CSS px at 96 dpi.
const A4: (f32, f32) = (793.7008, 1122.5195);
const LETTER: (f32, f32) = (816.0, 1056.0);

/// Resolved page geometry in PDF units (`top, right, bottom, left` margins).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PageGeometry {
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) margin: [f32; 4],
}

impl PageGeometry {
    /// No `@page`: the PNG dimensions and a zero inset (byte-identical output).
    pub(super) fn from_raster(width: u32, height: u32) -> Self {
        Self {
            width: width as f32,
            height: height as f32,
            margin: [0.0; 4],
        }
    }

    /// Content area height used to paginate and place text baselines.
    pub(super) fn content_height(&self) -> f32 {
        (self.height - self.margin[0] - self.margin[2]).max(1.0)
    }
}

/// Finds the first `@page` rule in the document's inline stylesheets.
pub(super) fn geometry_from_document(document: &Document, width: u32, height: u32) -> PageGeometry {
    let root = document.root();
    for node in document.descendants(root) {
        let Some(element) = document.node(node).as_element() else {
            continue;
        };
        if !element.local_name.eq_ignore_ascii_case("style") {
            continue;
        }
        if let Some(geometry) = from_css(&document.text_content(node), width, height) {
            return geometry;
        }
    }
    PageGeometry::from_raster(width, height)
}

/// Parses one stylesheet and returns its first `@page` geometry (§4).
fn from_css(css: &str, width: u32, height: u32) -> Option<PageGeometry> {
    let sheet = parse_stylesheet(css);
    let at = sheet.rules.iter().find_map(|rule| match rule {
        Rule::At(at) if at.name.eq_ignore_ascii_case("page") => Some(at),
        _ => None,
    })?;
    let mut geometry = PageGeometry::from_raster(width, height);
    let block = at.block.as_ref()?;
    let source = serialize_values(&block.values);
    for declaration in parse_list_of_declarations(&source) {
        apply(&mut geometry, &declaration);
    }
    Some(geometry)
}

/// Applies one descriptor in source order: later longhands win over an earlier
/// shorthand, matching CSS cascade expansion (CSS Cascading 4 §7.5).
fn apply(geometry: &mut PageGeometry, declaration: &Declaration) {
    match declaration.name.as_str() {
        "size" => {
            if let Some((width, height)) = parse_size(&declaration.raw_value) {
                geometry.width = width;
                geometry.height = height;
            }
        }
        "margin" => {
            let values = lengths(&declaration.raw_value);
            if let Some(edges) = edges(&values) {
                geometry.margin = edges;
            }
        }
        "margin-top" => set_edge(&mut geometry.margin[0], &declaration.raw_value),
        "margin-right" => set_edge(&mut geometry.margin[1], &declaration.raw_value),
        "margin-bottom" => set_edge(&mut geometry.margin[2], &declaration.raw_value),
        "margin-left" => set_edge(&mut geometry.margin[3], &declaration.raw_value),
        _ => {}
    }
}

fn set_edge(edge: &mut f32, raw: &str) {
    let values = lengths(raw);
    if let Some(&value) = values.first() {
        *edge = value;
    }
}

/// CSS Paged Media 3 §4: `<length>{1,2} | auto | <page-size> || [portrait|landscape]`.
fn parse_size(raw: &str) -> Option<(f32, f32)> {
    let mut lengths = Vec::new();
    let mut named = None;
    let mut landscape = false;
    for token in raw.split_whitespace() {
        match token.to_ascii_lowercase().as_str() {
            "a4" => named = Some(A4),
            "letter" => named = Some(LETTER),
            "landscape" => landscape = true,
            "portrait" | "auto" => {}
            other => {
                if let Some(value) = absolute_length(other) {
                    lengths.push(value);
                }
            }
        }
    }
    let (width, height) = match (named, lengths.as_slice()) {
        (Some((w, h)), []) => (w, h),
        (None, [one]) => (*one, *one),
        (None, [w, h]) => (*w, *h),
        _ => return None,
    };
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    Some(if landscape {
        (height, width)
    } else {
        (width, height)
    })
}

/// CSS Values 4 §5: absolute lengths resolve to CSS px; `auto`/percent are
/// ignored for page descriptors (the page box has no percentage basis here).
fn absolute_length(token: &str) -> Option<f32> {
    match Length::parse(token, 16.0, 16.0)? {
        Length::Px(value) if value.is_finite() && value >= 0.0 => Some(value),
        _ => None,
    }
}

fn lengths(raw: &str) -> Vec<f32> {
    raw.split_whitespace()
        .filter_map(absolute_length)
        .take(4)
        .collect()
}

/// CSS Values 4 §5.2 four-value margin shorthand → `top right bottom left`.
fn edges(values: &[f32]) -> Option<[f32; 4]> {
    match values {
        [a] => Some([*a; 4]),
        [a, b] => Some([*a, *b, *a, *b]),
        [a, b, c] => Some([*a, *b, *c, *b]),
        [a, b, c, d] => Some([*a, *b, *c, *d]),
        _ => None,
    }
}

/// Minimal serialization of the at-rule block so the existing declaration
/// parser (`parse_list_of_declarations`, CSS Syntax §5.4.5) can consume it.
fn serialize_values(values: &[ComponentValue]) -> String {
    let mut out = String::new();
    for value in values {
        match value {
            ComponentValue::PreservedToken(token) => match token {
                CssToken::Ident(name) | CssToken::Function(name) | CssToken::AtKeyword(name) => {
                    out.push_str(name);
                }
                CssToken::Dimension { value, unit, .. } => {
                    let _ = write!(out, "{value}{unit}");
                }
                CssToken::Number { value, .. } => {
                    let _ = write!(out, "{value}");
                }
                CssToken::Percentage { value, .. } => {
                    let _ = write!(out, "{value}%");
                }
                CssToken::Delim(ch) => out.push(*ch),
                CssToken::Colon => out.push(':'),
                CssToken::Semicolon => out.push(';'),
                CssToken::Whitespace => out.push(' '),
                _ => {}
            },
            ComponentValue::Block(_) | ComponentValue::Function(_) => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_page_rule_with_margin_longhands_wins() {
        let css = "@page { size: A4 landscape; margin: 1px 2px; margin-left: 3pt } \
                   @page { size: 10px }";
        let page = from_css(css, 400, 300).expect("first @page");
        assert_eq!(page.width, 1122.5195);
        assert_eq!(page.height, 793.7008);
        assert_eq!(page.margin, [1.0, 2.0, 1.0, 4.0]);
    }

    #[test]
    fn absent_page_uses_raster_dimensions() {
        assert_eq!(from_css("p { color: red }", 400, 300), None);
        let fallback = PageGeometry::from_raster(400, 300);
        assert_eq!((fallback.width, fallback.height), (400.0, 300.0));
        assert_eq!(fallback.margin, [0.0; 4]);
    }
}
