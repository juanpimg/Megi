//! Computed `color` inheritance for inline SVGs.
//!
//! An inline `<svg>` is rasterized as a standalone SVG document, so the host element's
//! computed `color` is not in scope there: `fill="currentColor"` would otherwise resolve to
//! the SVG document's default black (the "black square" bug on MDN/Google). This module
//! resolves the host `color` once per document and returns a presentation-attribute value
//! that [`super::svg_markup`] injects into the serialized root `<svg>`.
//!
//! The cascade uses `megi_css`'s public API. When the pipeline supplies its already-built
//! [`megi_css::StyleCollection`], that pool includes linked stylesheets and `@import`s (fetched
//! once by the resource loader, with the usual CSP/budget checks); otherwise this module falls
//! back to the UA stylesheet plus author `<style>` elements. CSS Variables 1 §3 substitutions
//! (`color: var(--icon-color)`) are already applied by the cascade, and CSS Color 4 §5 defines
//! the resulting computed `color` as the value of `currentColor`.

use std::collections::HashMap;

use megi_core::{Document, NodeId};
use megi_css::Origin;

/// Upper bound on author CSS text collected from `<style>` elements (defensive, non-fatal).
const MAX_STYLE_TEXT_BYTES: usize = 256 * 1024;
/// Upper bound on the number of `<style>` elements collected (defensive, non-fatal).
const MAX_STYLE_ELEMENTS: usize = 128;

/// Maps each inline SVG to the inherited `color` value to inject as a presentation attribute.
///
/// `collection` is the pipeline's cascade pool when available: it already contains external
/// stylesheets (`<link>`/`@import`) and inline declarations. Passing `None` keeps the
/// standalone fallback (UA stylesheet + `<style>` elements) for direct callers.
///
/// Returns an empty map when the document has no usable computed colors, in which case the
/// SVG rasterizer keeps its default (black) and behavior is unchanged.
pub(super) fn resolve(
    document: &Document,
    svg_ids: &[NodeId],
    collection: Option<&megi_css::StyleCollection>,
) -> HashMap<NodeId, String> {
    let inherited = HashMap::new();
    if svg_ids.is_empty() {
        return inherited;
    }

    if let Some(collection) = collection {
        return resolve_from_collection(document, svg_ids, collection);
    }

    let mut author_css = String::new();
    let mut style_elements = 0usize;
    for node_id in document.descendants(document.root()) {
        let Some(element) = document.node(node_id).as_element() else {
            continue;
        };
        if element.namespace != megi_core::Namespace::Html || element.local_name != "style" {
            continue;
        }
        style_elements += 1;
        if style_elements > MAX_STYLE_ELEMENTS {
            break;
        }
        for child in document.children(node_id) {
            let Some(text) = document.node(child).as_text() else {
                continue;
            };
            let remaining = MAX_STYLE_TEXT_BYTES.saturating_sub(author_css.len());
            author_css.push_str(&text[..text.len().min(remaining)]);
            if author_css.len() >= MAX_STYLE_TEXT_BYTES {
                break;
            }
        }
        if author_css.len() >= MAX_STYLE_TEXT_BYTES {
            break;
        }
    }

    let mut fallback = megi_css::create_user_agent_style_collection();
    if !author_css.is_empty() {
        fallback.add_css(&author_css, Origin::Author);
    }
    resolve_from_collection(document, svg_ids, &fallback)
}

/// Shared extraction of computed host colors from any ready cascade collection.
fn resolve_from_collection(
    document: &Document,
    svg_ids: &[NodeId],
    collection: &megi_css::StyleCollection,
) -> HashMap<NodeId, String> {
    let (styles, _) = megi_css::resolve_document_styles_from_collection(document, collection);
    let mut inherited = HashMap::new();
    for &svg_id in svg_ids {
        // CSS `color` inherits from the parent box; the computed style already includes it.
        let Some(parent) = document.node(svg_id).parent else {
            continue;
        };
        let Some(style) = styles.get(&parent) else {
            continue;
        };
        // The default UA color is already the SVG rasterizer's default; avoid touching markup.
        if style.color == megi_css::Color::BLACK {
            continue;
        }
        // An author-specified `color` on the SVG itself outranks inherited values.
        if has_own_color(document, svg_id) {
            continue;
        }
        inherited.insert(svg_id, format_color(style.color));
    }
    inherited
}

/// True when the SVG root explicitly declares `color` as an attribute or inline declaration.
fn has_own_color(document: &Document, svg_id: NodeId) -> bool {
    let Some(element) = document.node(svg_id).as_element() else {
        return true;
    };
    if element.get_attribute("color").is_some() {
        return true;
    }
    element
        .get_attribute("style")
        .is_some_and(style_declares_color)
}

/// Detects a `color:` declaration without matching `background-color:` or `currentColor`.
fn style_declares_color(style: &str) -> bool {
    let lower = style.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    for (index, _) in lower.match_indices("color") {
        if index > 0 {
            let previous = bytes[index - 1];
            if previous == b'-' || previous.is_ascii_alphanumeric() {
                continue;
            }
        }
        let mut cursor = index + "color".len();
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if bytes.get(cursor) == Some(&b':') {
            return true;
        }
    }
    false
}

/// Formats a CSS color as a hex presentation-attribute value understood by `megi_svg`.
fn format_color(color: megi_css::Color) -> String {
    if color.a == 255 {
        format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
    } else {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            color.r, color.g, color.b, color.a
        )
    }
}
