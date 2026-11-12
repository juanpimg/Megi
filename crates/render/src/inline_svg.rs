//! Inline `<svg>` pre-processing: serialize foreign-content SVG elements and replace them
//! with synthetic `<img src="data:image/svg+xml;base64,…">` elements (WP-5.11).
//!
//! The HTML parser inserts inline `<svg>` subtrees as SVG-namespace foreign content, which
//! the box tree/layout/paint pipeline cannot paint directly. Instead of teaching every
//! stage about SVG, this module runs **after scripts** (so script-mutated DOM is honored)
//! and **before style resolution/layout**:
//!
//! 1. every top-level inline `<svg>` (SVG namespace, not nested in another `<svg>`) is
//!    serialized with the HTML serializer plus a repaired `xmlns`/`xmlns:xlink` declaration;
//! 2. the markup is embedded as a `data:image/svg+xml;base64,…` URL and the node is replaced
//!    by an HTML `<img>` that keeps `class`, `id`, `style`, `width`, `height`, `alt`,
//!    `role`, `title`, `aria-*` and `data-*` attributes;
//! 3. the regular SVG image pipeline ([`crate::images`]) supplies the intrinsic size
//!    (`width`/`height`/`viewBox`, 300×150 default), the 16 Mpx raster cap, warnings and the
//!    `svgs_rendered` stat, exactly like `<img src="*.svg">`.
//!
//! ## Defensive limits (non-fatal)
//!
//! * at most [`MAX_INLINE_SVGS`] inline SVGs per document;
//! * at most [`MAX_INLINE_SVG_BYTES`] serialized bytes per SVG and
//!   [`MAX_TOTAL_INLINE_SVG_BYTES`] per document;
//! * at most [`MAX_INLINE_SVG_DEPTH`] nested nodes per SVG subtree.
//!
//! Anything over a limit produces a warning and leaves the original `<svg>` node untouched
//! (it becomes an empty box, like today), never an error/panic. A corrupt inline SVG is
//! replaced but its synthetic `<img>` fails during decoding: warning + placeholder, the rest
//! of the page still renders.
//!
//! ## Documented simplifications
//!
//! * CSS selectors that target `svg`/SVG-internal elements do not reach the rasterized
//!   image; only the attributes copied to the synthetic `<img>` participate in CSS.
//! * In-SVG CSS/`<style>`, gradients/patterns, filters, `<text>`, animation and external
//!   references remain unsupported (svg crate limitations); links/`<a>` inside the SVG are
//!   lost (the synthetic image is not interactive).
//! * The rasterized intrinsic size may be fractional; it is rounded up by <1 CSS px.

use dom::{Attribute, Document, Namespace, NodeId};

/// Maximum number of inline SVG elements converted per document.
pub const MAX_INLINE_SVGS: usize = 256;
/// Maximum serialized SVG markup accepted for a single inline SVG (1 MiB).
pub const MAX_INLINE_SVG_BYTES: usize = 1024 * 1024;
/// Maximum serialized SVG markup accepted per document (8 MiB).
pub const MAX_TOTAL_INLINE_SVG_BYTES: usize = 8 * 1024 * 1024;
/// Maximum nested DOM depth of one inline SVG subtree.
pub const MAX_INLINE_SVG_DEPTH: usize = 256;
/// Maximum number of DOM nodes in one inline SVG subtree (breadth bomb guard: the byte cap
/// alone is checked after serialization, which would allocate the whole string first).
pub const MAX_INLINE_SVG_NODES: usize = 4096;

/// Outcome of the inline-SVG pre-processing pass.
#[derive(Default)]
pub struct InlineSvgReport {
    /// Inline SVGs replaced by synthetic `<img>` elements.
    pub replaced: usize,
    /// Inline SVGs left untouched because a defensive limit fired.
    pub skipped: usize,
    /// Serialized SVG markup bytes embedded as data URLs.
    pub serialized_bytes: usize,
    /// Non-fatal warnings (limits).
    pub warnings: Vec<String>,
}

/// Attributes copied from the inline `<svg>` to the synthetic `<img>`.
const COPIED_ATTRIBUTES: [&str; 9] = [
    "class",
    "id",
    "style",
    "width",
    "height",
    "alt",
    "role",
    "title",
    "aria-label",
];

/// Replaces every top-level inline SVG of `document` with a synthetic `<img>` data URL.
pub fn replace_inline_svgs(document: &mut Document) -> InlineSvgReport {
    let mut report = InlineSvgReport {
        replaced: 0,
        skipped: 0,
        serialized_bytes: 0,
        warnings: Vec::new(),
    };
    let candidates = collect_inline_svgs(document);
    let candidate_count = candidates.len();
    let mut total_bytes = 0usize;
    let mut limit_warned = false;
    for (index, node_id) in candidates.into_iter().enumerate() {
        if report.replaced >= MAX_INLINE_SVGS {
            report.skipped += 1;
            if !limit_warned {
                report.warnings.push(format!(
                    "page has more than {MAX_INLINE_SVGS} inline <svg> elements; the remaining ones are left untouched (cap)"
                ));
                limit_warned = true;
            }
            continue;
        }
        if document.node(node_id).parent.is_none() {
            report.skipped += 1;
            continue;
        }
        if inline_svg_depth(document, node_id) > MAX_INLINE_SVG_DEPTH {
            report.skipped += 1;
            report.warnings.push(format!(
                "inline <svg> #{} left untouched: subtree depth exceeds {MAX_INLINE_SVG_DEPTH}",
                index + 1
            ));
            continue;
        }
        if inline_svg_node_count(document, node_id) > MAX_INLINE_SVG_NODES {
            report.skipped += 1;
            report.warnings.push(format!(
                "inline <svg> #{} left untouched: subtree exceeds {MAX_INLINE_SVG_NODES} nodes",
                index + 1
            ));
            continue;
        }
        let markup = svg_markup(document, node_id);
        if markup.len() > MAX_INLINE_SVG_BYTES {
            report.skipped += 1;
            report.warnings.push(format!(
                "inline <svg> #{} left untouched: serialized markup ({} bytes) exceeds {MAX_INLINE_SVG_BYTES} bytes",
                index + 1,
                markup.len()
            ));
            continue;
        }
        if total_bytes + markup.len() > MAX_TOTAL_INLINE_SVG_BYTES {
            report.skipped += candidate_count - index;
            report.warnings.push(format!(
                "inline <svg> #{} left untouched: total serialized SVG markup exceeds {MAX_TOTAL_INLINE_SVG_BYTES} bytes",
                index + 1
            ));
            break;
        }
        total_bytes += markup.len();
        let data_url = format!(
            "data:image/svg+xml;base64,{}",
            net::base64_encode(markup.as_bytes())
        );
        let attributes = synthetic_img_attributes(document, node_id, &data_url);
        let image_id = document.create_element(Namespace::Html, "img", attributes);
        if !document.insert_before(node_id, image_id) {
            document.remove_subtree(image_id);
            report.skipped += 1;
            continue;
        }
        document.remove_subtree(node_id);
        report.replaced += 1;
        report.serialized_bytes += markup.len();
    }
    report
}

/// Collects top-level inline SVGs (SVG namespace, not nested inside another `<svg>`).
fn collect_inline_svgs(document: &Document) -> Vec<NodeId> {
    let mut candidates = Vec::new();
    for node_id in document.descendants(document.root()) {
        let Some(element) = document.node(node_id).as_element() else {
            continue;
        };
        if element.namespace != Namespace::Svg || element.local_name != "svg" {
            continue;
        }
        let nested = document.ancestors(node_id).any(|ancestor| {
            document
                .node(ancestor)
                .as_element()
                .is_some_and(|el| el.namespace == Namespace::Svg)
        });
        if !nested {
            candidates.push(node_id);
        }
    }
    candidates
}

/// Iterative maximum nesting depth of the subtree rooted at `root_id`.

/// Number of DOM nodes in the subtree rooted at `root_id`, stopping one past the cap.
fn inline_svg_node_count(document: &Document, root_id: NodeId) -> usize {
    let mut count = 0usize;
    let mut stack = vec![root_id];
    while let Some(node_id) = stack.pop() {
        count += 1;
        if count > MAX_INLINE_SVG_NODES {
            break;
        }
        let mut child = document.try_node(node_id).and_then(|node| node.first_child);
        while let Some(child_id) = child {
            stack.push(child_id);
            child = document
                .try_node(child_id)
                .and_then(|node| node.next_sibling);
        }
    }
    count
}

fn inline_svg_depth(document: &Document, root_id: NodeId) -> usize {
    let mut maximum = 0usize;
    let mut stack = vec![(root_id, 0usize)];
    while let Some((node_id, depth)) = stack.pop() {
        maximum = maximum.max(depth);
        for child in document.children(node_id) {
            stack.push((child, depth + 1));
        }
    }
    maximum
}

/// Serializes the inline SVG to standalone SVG markup (repairing `xmlns` declarations).
fn svg_markup(document: &Document, node_id: NodeId) -> String {
    let serialized = html::serialize_outer_html(document, node_id);
    let element = document.node(node_id).as_element();
    let has_xmlns = element.is_some_and(|el| {
        el.attributes
            .iter()
            .any(|attribute| attribute.local_name == "xmlns")
    });
    let has_xlink = element.is_some_and(|el| {
        el.attributes
            .iter()
            .any(|attribute| attribute.local_name == "xlink")
    });
    if !serialized.starts_with("<svg") {
        return serialized;
    }
    let mut markup = String::with_capacity(serialized.len() + 96);
    markup.push_str("<svg");
    if !has_xmlns {
        markup.push_str(" xmlns=\"http://www.w3.org/2000/svg\"");
    }
    if !has_xlink {
        markup.push_str(" xmlns:xlink=\"http://www.w3.org/1999/xlink\"");
    }
    markup.push_str(&serialized[4..]);
    markup
}

/// Attributes of the synthetic `<img>`: `src` plus the relevant presentation/identity ones.
fn synthetic_img_attributes(document: &Document, svg_id: NodeId, data_url: &str) -> Vec<Attribute> {
    let mut attributes = Vec::with_capacity(COPIED_ATTRIBUTES.len() + 4);
    for &name in &COPIED_ATTRIBUTES {
        if let Some(value) = document
            .node(svg_id)
            .as_element()
            .and_then(|element| element.get_attribute(name))
        {
            attributes.push(Attribute::new(name.to_owned(), value.to_owned()));
        }
    }
    if let Some(element) = document.node(svg_id).as_element() {
        for attribute in &element.attributes {
            let name = attribute.local_name.as_str();
            if name.starts_with("aria-") || name.starts_with("data-") {
                attributes.push(Attribute::new(name.to_owned(), attribute.value.clone()));
            }
        }
    }
    attributes.push(Attribute::new("src", data_url.to_owned()));
    attributes
}
