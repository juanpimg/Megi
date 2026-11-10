//! HTML DOM traversal helpers for the render pipeline: `<base href>` and stylesheet
//! collection (`<style>` elements and `<link rel="stylesheet">`).
//!
//! ## Scope
//!
//! - Only the **first** `<base href>` in document order is honored (WHATWG HTML
//!   §4.2.3), resolved against the document URL.
//! - `<style>` text content is concatenated in document order.
//! - `<link>` is collected only when `rel` contains the `stylesheet` keyword (alternate
//!   style sheets are ignored) and `media` is absent or a screen/all medium; the `media`
//!   attribute value is not evaluated as a full media query (documented).
//! - `@import` inside author CSS is **not** followed (CSS parser drops at-rules whose
//!   contents are not style rules); this is documented as out of scope.
//! - `style=""` attributes are handled later by the CSS cascade, not here.

use dom::{Document, Namespace, NodeId, NodeKind};
use net::Url;

use crate::error::RenderError;

/// One author stylesheet source in document order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CssSource {
    /// Inline `<style>` element contents.
    Inline(String),
    /// External `<link rel="stylesheet">` URL (already resolved against `<base>`).
    External(Url),
}

/// Stylesheets collected from a document, plus non-fatal warnings.
#[derive(Clone, Debug, Default)]
pub struct CollectedStyles {
    /// Sources in document order (cascade order).
    pub sources: Vec<CssSource>,
    /// Non-fatal problems (invalid `<base>`/`href`, skipped media, ...).
    pub warnings: Vec<String>,
}

impl CollectedStyles {
    /// Number of external `<link rel="stylesheet">` sources.
    pub fn external_count(&self) -> usize {
        self.sources
            .iter()
            .filter(|source| matches!(source, CssSource::External(_)))
            .count()
    }
}

/// Returns the element payload for `node_id` when it is an HTML element.
fn html_element(doc: &Document, node_id: NodeId) -> Option<&dom::ElementData> {
    doc.node(node_id)
        .as_element()
        .filter(|element| element.namespace == Namespace::Html)
}

/// Case-insensitive attribute lookup by local name in the null namespace.
fn attr<'a>(element: &'a dom::ElementData, name: &str) -> Option<&'a str> {
    element
        .attributes
        .iter()
        .find(|attribute| {
            attribute.namespace == Namespace::None
                && attribute.local_name.eq_ignore_ascii_case(name)
        })
        .map(|attribute| attribute.value.as_str())
}

/// Returns every node in document order (iterative pre-order traversal).
fn nodes_in_document_order(doc: &Document) -> Vec<NodeId> {
    let mut order = Vec::new();
    let mut stack: Vec<NodeId> = Vec::new();
    let mut child = doc.node(doc.root()).first_child;
    let mut root_children = Vec::new();
    while let Some(node_id) = child {
        root_children.push(node_id);
        child = doc.node(node_id).next_sibling;
    }
    for node_id in root_children.into_iter().rev() {
        stack.push(node_id);
    }
    while let Some(node_id) = stack.pop() {
        order.push(node_id);
        let mut children = Vec::new();
        let mut child = doc.node(node_id).first_child;
        while let Some(child_id) = child {
            children.push(child_id);
            child = doc.node(child_id).next_sibling;
        }
        for child_id in children.into_iter().rev() {
            stack.push(child_id);
        }
    }
    order
}

/// Concatenated text descendants of `node_id` (used for `<style>` elements).
fn text_content(doc: &Document, node_id: NodeId) -> String {
    let mut out = String::new();
    let mut stack: Vec<NodeId> = vec![node_id];
    while let Some(current) = stack.pop() {
        let node = doc.node(current);
        if let NodeKind::Text(text) = &node.kind {
            out.push_str(text);
        }
        let mut children = Vec::new();
        let mut child = node.first_child;
        while let Some(child_id) = child {
            children.push(child_id);
            child = doc.node(child_id).next_sibling;
        }
        for child_id in children.into_iter().rev() {
            stack.push(child_id);
        }
    }
    out
}

/// Resolves the document base URL: the URL used for every relative resource.
///
/// The first `<base href>` in document order wins and is resolved against
/// `document_url`; an unparsable/empty `href` is reported as a warning and ignored.
pub fn find_base_url(doc: &Document, document_url: &Url) -> (Url, Vec<String>) {
    let mut warnings = Vec::new();
    for node_id in nodes_in_document_order(doc) {
        let Some(element) = html_element(doc, node_id) else {
            continue;
        };
        if !element.local_name.eq_ignore_ascii_case("base") {
            continue;
        }
        let Some(href) = attr(element, "href") else {
            continue;
        };
        if href.trim().is_empty() {
            break;
        }
        return match document_url.join(href.trim()) {
            Ok(base) => (base, warnings),
            Err(error) => {
                warnings.push(format!("ignoring invalid <base href=\"{href}\">: {error}"));
                (document_url.clone(), warnings)
            }
        };
    }
    (document_url.clone(), warnings)
}

/// Collects inline and external stylesheets in document order.
pub fn collect_stylesheets(doc: &Document, base_url: &Url) -> CollectedStyles {
    let mut collected = CollectedStyles::default();
    let mut seen_external = std::collections::HashSet::new();

    for node_id in nodes_in_document_order(doc) {
        let Some(element) = html_element(doc, node_id) else {
            continue;
        };
        let name = element.local_name.to_ascii_lowercase();
        if name == "style" {
            collected
                .sources
                .push(CssSource::Inline(text_content(doc, node_id)));
        } else if name == "link" {
            let rel = attr(element, "rel").unwrap_or("").to_ascii_lowercase();
            let relations: Vec<&str> = rel.split_ascii_whitespace().collect();
            if !relations.contains(&"stylesheet") || relations.contains(&"alternate") {
                continue;
            }
            if !media_matches_screen(attr(element, "media")) {
                continue;
            }
            let Some(href) = attr(element, "href") else {
                continue;
            };
            if href.trim().is_empty() {
                continue;
            }
            match base_url.join(href.trim()) {
                Ok(url) => {
                    if seen_external.insert(url.cache_key()) {
                        collected.sources.push(CssSource::External(url));
                    }
                }
                Err(error) => collected.warnings.push(format!(
                    "ignoring invalid stylesheet href \"{href}\": {error}"
                )),
            }
        }
    }
    collected
}

/// `true` when a `media` attribute selects the screen medium (or is absent/empty).
fn media_matches_screen(media: Option<&str>) -> bool {
    let Some(media) = media else {
        return true;
    };
    if media.trim().is_empty() {
        return true;
    }
    media.split(',').any(|query| {
        let query = query.trim().to_ascii_lowercase();
        query == "all"
            || query.starts_with("all ")
            || query == "screen"
            || query.starts_with("screen ")
    })
}

/// Validates that a URL scheme is fetchable by the render pipeline.
pub fn ensure_supported_scheme(url: &Url) -> Result<(), RenderError> {
    match url.scheme() {
        net::Scheme::Http | net::Scheme::Https | net::Scheme::File | net::Scheme::Data => Ok(()),
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn media_screen_filter() {
        assert!(media_matches_screen(None));
        assert!(media_matches_screen(Some("")));
        assert!(media_matches_screen(Some("screen")));
        assert!(media_matches_screen(Some("all")));
        assert!(media_matches_screen(Some("screen and (min-width: 600px)")));
        assert!(!media_matches_screen(Some("print")));
        assert!(!media_matches_screen(Some("print, speech")));
    }

    #[test]
    fn collects_base_and_stylesheets_in_order() {
        let mut doc = Document::new();
        let html = doc.create_element(Namespace::Html, "html", vec![]);
        let head = doc.create_element(Namespace::Html, "head", vec![]);
        let base = doc.create_element(
            Namespace::Html,
            "base",
            vec![dom::Attribute::new("href", "/assets/")],
        );
        let style = doc.create_element(Namespace::Html, "style", vec![]);
        let link = doc.create_element(
            Namespace::Html,
            "link",
            vec![
                dom::Attribute::new("rel", "stylesheet"),
                dom::Attribute::new("href", "theme.css"),
            ],
        );
        doc.append_child(doc.root(), html);
        doc.append_child(html, head);
        doc.append_child(head, base);
        doc.append_child(head, style);
        doc.append_text(style, "p { color: red }");
        doc.append_child(head, link);

        let document_url = Url::parse("http://example.test/page.html").expect("url");
        let (base_url, warnings) = find_base_url(&doc, &document_url);
        assert!(warnings.is_empty());
        assert_eq!(base_url.as_str(), "http://example.test/assets/");

        let collected = collect_stylesheets(&doc, &base_url);
        assert_eq!(collected.sources.len(), 2);
        assert!(matches!(&collected.sources[0], CssSource::Inline(text) if text.contains("red")));
        match &collected.sources[1] {
            CssSource::External(url) => {
                assert_eq!(url.as_str(), "http://example.test/assets/theme.css")
            }
            other => panic!("expected external stylesheet, got {other:?}"),
        }
    }
}
