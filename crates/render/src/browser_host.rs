//! Browser-facing host helpers: computed-style serialization and media-query evaluation
//! for the script host (WP-6.5 / R96).
//!
//! Scripts run against a **snapshot of the computed styles taken before script execution**
//! (author + UA cascade, same cascade the render will use after scripts). `getComputedStyle`
//! exposes that snapshot through [`megi_js::JsRuntime::set_computed_style_provider`]; layout and
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
//! `matchMedia(query)` is evaluated with the same [`megi_css::MediaContext`] used to load the
//! page's stylesheets (viewport from the render options, `screen` medium), so media queries
//! seen by scripts agree with the CSS cascade. `window.innerWidth`/`innerHeight`/
//! `devicePixelRatio` come from the same viewport, with DPR taken from the host's
//! [`crate::RenderOptions::device_pixel_ratio`] (1.0 for headless renders).

use std::collections::HashMap;
use std::sync::Arc;

use megi_core::{Document, NodeId};
use megi_css::{
    BorderStyle, Color, ComputedStyle, Display, FontStyle, Length, Position, TextAlign,
};

/// Serializes one computed style into `(kebab-case property, CSS value)` pairs.
///
/// CSSOM §6 (`getComputedStyle`): the CSS declaration block exposes computed values in
/// CSS syntax. Public so protocol bridges (DevTools `CSS.getComputedStyle`) can reuse the
/// exact same serialization as the page's `window.getComputedStyle`.
pub fn computed_style_pairs(style: &ComputedStyle) -> Vec<(String, String)> {
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
    // Animation/transition/effect properties: the deterministic renderer does
    // not animate, but CSSOM must still expose their initial values (#1980).
    for (name, value) in [
        ("animation-name", "none"),
        ("animation-duration", "0s"),
        ("transition-property", "all"),
        ("transition-duration", "0s"),
        ("mix-blend-mode", "normal"),
        ("filter", "none"),
        ("backdrop-filter", "none"),
        ("contain", "none"),
        ("content-visibility", "visible"),
    ] {
        push(name, value.to_owned());
    }
    pairs
}

/// Returns the computed-style pairs for `node_id`, falling back to the inline `style`
/// declarations when the node has no computed style (e.g. non-elements or nodes created
/// after the snapshot).
pub(crate) fn computed_style_for(
    styles: &HashMap<NodeId, ComputedStyle>,
    document: &Document,
    node_id: NodeId,
    layout: Option<&HashMap<NodeId, megi_js::host::LayoutRect>>,
) -> Vec<(String, String)> {
    let mut pairs = if let Some(style) = styles.get(&node_id) {
        computed_style_pairs(style)
    } else {
        return fallback_style_pairs(document, node_id, layout);
    };
    // CSSOM §6.4: getComputedStyle returns *used* values for width/height. The pre-script
    // layout snapshot has them even though the style snapshot only has specified values (#1963).
    if let Some(rect) = layout.and_then(|rects| rects.get(&node_id)) {
        let (width, height) = (rect.content_box.2, rect.content_box.3);
        if width.is_finite() && width >= 0.0 {
            replace_pair(&mut pairs, "width", px_css(width));
        }
        if height.is_finite() && height >= 0.0 {
            replace_pair(&mut pairs, "height", px_css(height));
        }
    }
    pairs
}

/// Caches serialized computed-style pairs per DOM node (F6-GETCOMPUTEDSTYLE-PERF / #1868).
///
/// The pre-script style snapshot is immutable for a render, so each node's 42 CSS pairs
/// only need to be built once. Repeated `getComputedStyle` reads then reuse the cached
/// vector instead of allocating and formatting every property again. Nodes absent from
/// the snapshot (created by scripts) use the inline-style fallback and are cached too.
pub(crate) struct ComputedStyleCache {
    styles: Arc<HashMap<NodeId, ComputedStyle>>,
    layout: Option<Arc<HashMap<NodeId, megi_js::host::LayoutRect>>>,
    cached: std::sync::Mutex<HashMap<NodeId, Vec<(String, String)>>>,
    serializations: std::sync::atomic::AtomicUsize,
}

impl ComputedStyleCache {
    pub(crate) fn new(
        styles: Arc<HashMap<NodeId, ComputedStyle>>,
        layout: Option<Arc<HashMap<NodeId, megi_js::host::LayoutRect>>>,
    ) -> Self {
        Self {
            styles,
            layout,
            cached: std::sync::Mutex::new(HashMap::new()),
            serializations: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Returns the cached pairs for `node_id`, serializing on the first read only.
    pub(crate) fn get(&self, document: &Document, node_id: NodeId) -> Vec<(String, String)> {
        if let Some(pairs) = self
            .cached
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&node_id)
        {
            return pairs.clone();
        }
        let pairs = computed_style_for(&self.styles, document, node_id, self.layout.as_deref());
        self.serializations
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.cached
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(node_id, pairs.clone());
        pairs
    }

    /// Number of nodes serialized so far (diagnostics/tests).
    #[cfg(test)]
    pub(crate) fn serializations(&self) -> usize {
        self.serializations
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}

fn replace_pair(pairs: &mut Vec<(String, String)>, name: &str, value: String) {
    if let Some(pair) = pairs.iter_mut().find(|pair| pair.0 == name) {
        pair.1 = value;
    } else {
        pairs.push((name.to_owned(), value));
    }
}

fn fallback_style_pairs(
    document: &Document,
    node_id: NodeId,
    layout: Option<&HashMap<NodeId, megi_js::host::LayoutRect>>,
) -> Vec<(String, String)> {
    let mut pairs = document
        .try_node(node_id)
        .and_then(|node| node.as_element())
        .and_then(|element| element.get_attribute("style"))
        .map(|inline| {
            megi_css::parse_inline_declarations(inline)
                .into_iter()
                .map(|declaration| (declaration.name, declaration.raw_value))
                .collect()
        })
        .unwrap_or_default();
    if let Some(rect) = layout.and_then(|rects| rects.get(&node_id)) {
        replace_pair(&mut pairs, "width", px_css(rect.content_box.2));
        replace_pair(&mut pairs, "height", px_css(rect.content_box.3));
    }
    pairs
}

/// Evaluates a `matchMedia` query string against the render's media context.
pub(crate) fn media_matches(media: &megi_css::MediaContext, query: &str) -> bool {
    let list = megi_css::parse_media_query_list(query);
    megi_css::evaluate_media(&list, media)
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
        Display::Contents => "contents",
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

#[cfg(test)]
mod tests {
    use super::*;
    use megi_core::{Attribute, Namespace};

    /// CSSOM must expose animation/transition/effect properties with their
    /// initial values so feature-detection sees a string, not undefined (#1980).
    #[test]
    fn computed_style_pairs_include_effect_properties() {
        let pairs = computed_style_pairs(&ComputedStyle::initial());
        let expected = [
            ("animation-name", "none"),
            ("animation-duration", "0s"),
            ("transition-property", "all"),
            ("transition-duration", "0s"),
            ("mix-blend-mode", "normal"),
            ("filter", "none"),
            ("backdrop-filter", "none"),
            ("contain", "none"),
            ("content-visibility", "visible"),
        ];
        for (name, value) in expected {
            assert_eq!(
                pairs
                    .iter()
                    .find(|(n, _)| n == name)
                    .map(|(_, v)| v.as_str()),
                Some(value),
                "missing CSSOM property {name} (#1980)"
            );
        }
    }

    #[test]
    fn computed_style_for_bounds_oversized_inline_style() {
        let prefix = "color: rgb(1, 2, 3); ";
        let mut style = String::from(prefix);
        style.push_str(&" ".repeat(megi_css::MAX_INLINE_STYLE_BYTES - prefix.len() - 1));
        style.push('🦀');
        style.push_str("; background-color: rgb(9, 9, 9);");
        assert!(!style.is_char_boundary(megi_css::MAX_INLINE_STYLE_BYTES));

        let mut doc = Document::new();
        let div = doc.create_element(Namespace::Html, "div", vec![Attribute::new("style", style)]);
        doc.append_child(doc.root(), div);

        let pairs = computed_style_for(&HashMap::new(), &doc, div, None);
        assert_eq!(pairs, vec![("color".to_owned(), "rgb(1, 2, 3)".to_owned())]);
    }
}

#[cfg(test)]
mod used_value_tests {
    use super::*;
    use megi_js::host::LayoutRect;

    #[test]
    fn computed_style_uses_layout_dimensions_for_width_and_height() {
        // #1963: getComputedStyle must report used px sizes, not `auto`.
        let mut doc = Document::new();
        let div = doc.create_element(megi_core::Namespace::Html, "div", Vec::new());
        doc.append_child(doc.root(), div);
        let mut rects = HashMap::new();
        rects.insert(
            div,
            LayoutRect {
                border_box: (0.0, 0.0, 100.0, 20.0),
                content_box: (0.0, 0.0, 96.0, 16.0),
            },
        );
        let pairs = computed_style_for(&HashMap::new(), &doc, div, Some(&rects));
        let get = |name: &str| {
            pairs
                .iter()
                .find(|pair| pair.0 == name)
                .map(|pair| pair.1.clone())
        };
        assert_eq!(get("width").as_deref(), Some("96px"));
        assert_eq!(get("height").as_deref(), Some("16px"));
    }
}

#[cfg(test)]
mod style_cache_tests {
    use super::*;
    use megi_core::{Attribute, Namespace};
    use std::sync::Arc;

    #[test]
    fn computed_style_cache_serializes_each_node_only_once() {
        // #1868: getComputedStyle used to build 42 String pairs on every call. The cache
        // must serialize a node once and reuse the result for repeated reads.
        let mut doc = Document::new();
        let html = doc.create_element(Namespace::Html, "html", vec![]);
        let div = doc.create_element(Namespace::Html, "div", vec![Attribute::new("id", "x")]);
        assert!(doc.append_child(doc.root(), html));
        assert!(doc.append_child(html, div));
        let mut styles = HashMap::new();
        styles.insert(div, ComputedStyle::initial());

        let cache = ComputedStyleCache::new(Arc::new(styles), None);
        let first = cache.get(&doc, div);
        let second = cache.get(&doc, div);
        assert_eq!(first, second);
        assert_eq!(
            cache.serializations(),
            1,
            "the node must be serialized once"
        );
        assert!(first.iter().any(|(name, _)| name == "color"));

        // A different node serializes once more (no cross-node contamination).
        cache.get(&doc, html);
        assert_eq!(cache.serializations(), 2);
    }

    #[test]
    fn computed_style_cache_falls_back_for_nodes_outside_the_snapshot() {
        let mut doc = Document::new();
        let div = doc.create_element(
            Namespace::Html,
            "div",
            vec![Attribute::new("style", "color:red")],
        );
        assert!(doc.append_child(doc.root(), div));
        let cache = ComputedStyleCache::new(Arc::new(HashMap::new()), None);
        let pairs = cache.get(&doc, div);
        assert!(pairs.iter().any(|(n, v)| n == "color" && v == "red"));
    }
}
