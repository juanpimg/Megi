//! Browser-facing host helpers: computed-style serialization and media-query evaluation
//! for the script host (WP-6.5 / R96).
//!
//! Scripts run against a **snapshot of the computed styles taken before script execution**
//! (author + UA cascade, same cascade the render will use after scripts). `getComputedStyle`
//! exposes that snapshot through [`js::JsRuntime::set_computed_style_provider`]; layout and
//! paint are recomputed from scratch after scripts, so script-visible styles always describe
//! the pre-script DOM (documented limitation: re-querying after DOM mutations still returns
//! the snapshot).
//!
//! The serialization covers the agreed property list: `color`, `background-color`,
//! `font-size`/`font-family`/`font-weight`/`font-style`, `width`/`height`,
//! `margin-*`, `padding-*`, `display`, `position`, `text-align`, `line-height`,
//! `border-{top,right,bottom,left}-width/color/style` and `opacity`. Values are serialized in
//! CSS syntax (`rgb()`/`rgba()`, `px`/`%`/`auto`, keywords); widths/heights are the
//! *specified* computed values, not the used pixel sizes (documented simplification: no
//! containing-block resolution inside `js`).
//!
//! `matchMedia(query)` is evaluated with the same [`css::MediaContext`] used to load the
//! page's stylesheets (viewport from the render options, `screen` medium), so media queries
//! seen by scripts agree with the CSS cascade. `window.innerWidth`/`innerHeight`/
//! `devicePixelRatio` come from the same viewport (DPR is fixed at 1.0 by the pipeline).

use std::collections::HashMap;

use css::{BorderStyle, Color, ComputedStyle, Display, FontStyle, Length, Position, TextAlign};
use dom::{Document, NodeId};

/// Serializes one computed style into `(kebab-case property, CSS value)` pairs.
pub(crate) fn computed_style_pairs(style: &ComputedStyle) -> Vec<(String, String)> {
    let mut pairs = Vec::with_capacity(42);
    let mut push = |name: &str, value: String| pairs.push((name.to_owned(), value));

    push("color", color_css(&style.color));
    push("background-color", color_css(&style.background_color));
    push("font-size", px_css(style.font_size));
    push("font-family", style.font_family.clone());
    push("font-weight", style.font_weight.to_string());
    push("font-style", font_style_css(style.font_style).to_owned());
    push("width", length_css(style.width));
    push("height", length_css(style.height));
    for (name, value) in [
        ("margin-top", style.margin.top),
        ("margin-right", style.margin.right),
        ("margin-bottom", style.margin.bottom),
        ("margin-left", style.margin.left),
    ] {
        push(name, length_css(value));
    }
    for (name, value) in [
        ("padding-top", style.padding.top),
        ("padding-right", style.padding.right),
        ("padding-bottom", style.padding.bottom),
        ("padding-left", style.padding.left),
    ] {
        push(name, length_css(value));
    }
    push("display", display_css(style.display).to_owned());
    push("position", position_css(style.position).to_owned());
    push("text-align", text_align_css(style.text_align).to_owned());
    push("line-height", line_height_css(style));
    for (side, width, color, style_kind) in [
        (
            "top",
            style.border_width.top,
            style.border_color.top,
            style.border_style.top,
        ),
        (
            "right",
            style.border_width.right,
            style.border_color.right,
            style.border_style.right,
        ),
        (
            "bottom",
            style.border_width.bottom,
            style.border_color.bottom,
            style.border_style.bottom,
        ),
        (
            "left",
            style.border_width.left,
            style.border_color.left,
            style.border_style.left,
        ),
    ] {
        push(&format!("border-{side}-width"), px_css(width));
        push(&format!("border-{side}-color"), color_css(&color));
        push(
            &format!("border-{side}-style"),
            border_style_css(style_kind).to_owned(),
        );
    }
    push("opacity", format_f32(style.opacity));
    pairs
}

/// Returns the computed-style pairs for `node_id`, falling back to the inline `style`
/// declarations when the node has no computed style (e.g. non-elements or nodes created
/// after the snapshot).
pub(crate) fn computed_style_for(
    styles: &HashMap<NodeId, ComputedStyle>,
    document: &Document,
    node_id: NodeId,
) -> Vec<(String, String)> {
    if let Some(style) = styles.get(&node_id) {
        return computed_style_pairs(style);
    }
    document
        .try_node(node_id)
        .and_then(|node| node.as_element())
        .and_then(|element| element.get_attribute("style"))
        .map(|inline| {
            css::parse_list_of_declarations(inline)
                .into_iter()
                .map(|declaration| (declaration.name, declaration.raw_value))
                .collect()
        })
        .unwrap_or_default()
}

/// Evaluates a `matchMedia` query string against the render's media context.
pub(crate) fn media_matches(media: &css::MediaContext, query: &str) -> bool {
    let list = css::parse_media_query_list(query);
    css::evaluate_media(&list, media)
}

fn color_css(color: &Color) -> String {
    if color.a == 255 {
        format!("rgb({}, {}, {})", color.r, color.g, color.b)
    } else {
        format!(
            "rgba({}, {}, {}, {})",
            color.r,
            color.g,
            color.b,
            format_f32(f32::from(color.a) / 255.0)
        )
    }
}

fn length_css(length: Length) -> String {
    match length {
        Length::Px(px) => px_css(px),
        Length::Percent(pct) => format!("{}%", format_f32(pct)),
        Length::Auto => "auto".to_owned(),
    }
}

fn px_css(px: f32) -> String {
    format!("{}px", format_f32(px))
}

/// Formats a CSS number without trailing `.0` and without `-0`.
fn format_f32(value: f32) -> String {
    let rounded = (value * 10_000.0).round() / 10_000.0;
    if rounded == 0.0 {
        "0".to_owned()
    } else {
        format!("{rounded}")
    }
}

fn line_height_css(style: &ComputedStyle) -> String {
    let computed = style
        .line_height_factor
        .map(|factor| factor * style.font_size)
        .or(style.line_height);
    computed.map_or_else(|| "normal".to_owned(), px_css)
}

fn display_css(display: Display) -> &'static str {
    match display {
        Display::Inline => "inline",
        Display::Block => "block",
        Display::InlineBlock => "inline-block",
        Display::None => "none",
        Display::Flex => "flex",
        Display::Grid => "grid",
        Display::ListItem => "list-item",
        Display::Table => "table",
        Display::InlineTable => "inline-table",
        Display::TableRowGroup => "table-row-group",
        Display::TableHeaderGroup => "table-header-group",
        Display::TableFooterGroup => "table-footer-group",
        Display::TableRow => "table-row",
        Display::TableColumnGroup => "table-column-group",
        Display::TableColumn => "table-column",
        Display::TableCell => "table-cell",
        Display::TableCaption => "table-caption",
    }
}

fn position_css(position: Position) -> &'static str {
    match position {
        Position::Static => "static",
        Position::Relative => "relative",
        Position::Absolute => "absolute",
        Position::Fixed => "fixed",
        Position::Sticky => "sticky",
    }
}

fn text_align_css(text_align: TextAlign) -> &'static str {
    match text_align {
        TextAlign::Left => "left",
        TextAlign::Right => "right",
        TextAlign::Center => "center",
        TextAlign::Justify => "justify",
    }
}

fn font_style_css(font_style: FontStyle) -> &'static str {
    match font_style {
        FontStyle::Normal => "normal",
        FontStyle::Italic => "italic",
        FontStyle::Oblique => "oblique",
    }
}

fn border_style_css(border_style: BorderStyle) -> &'static str {
    match border_style {
        BorderStyle::None => "none",
        BorderStyle::Hidden => "hidden",
        BorderStyle::Solid => "solid",
        BorderStyle::Dotted => "dotted",
        BorderStyle::Dashed => "dashed",
        BorderStyle::Double => "double",
        BorderStyle::Groove => "groove",
        BorderStyle::Ridge => "ridge",
        BorderStyle::Inset => "inset",
        BorderStyle::Outset => "outset",
    }
}
