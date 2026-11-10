//! Window-title composition and `<title>` extraction (pure, display-free).
//!
//! The window title is `"<document title> — <short URL>"` (plus the scroll position and a
//! clipping marker), falling back to `"Megi — <short URL>"` when the document has no
//! `<title>`; render errors replace it with `"Megi — error: <message>"`. Everything here
//! is pure so the format can be unit-tested without a window.
//!
//! `document_title` walks the parsed DOM of the rendered document: the first HTML
//! `<title>` element's text content (descendant text nodes, whitespace collapsed) is the
//! page title, capped at [`MAX_TITLE_CHARS`]. `short_url` strips the fragment and caps the
//! URL at [`MAX_SHORT_URL_CHARS`] with an ellipsis; `data:` URLs are shortened to
//! `data:…` because they are usually huge inline documents.

use dom::{Document, Namespace, NodeId, NodeKind};

use net::Url;

/// Maximum number of characters kept from a document `<title>`.
pub const MAX_TITLE_CHARS: usize = 120;
/// Maximum number of characters kept from the URL in the window title.
pub const MAX_SHORT_URL_CHARS: usize = 80;
/// Title used when the document has no usable `<title>` (the engine name).
pub const FALLBACK_NAME: &str = "Megi";
/// Marker appended when the rendered page was clipped at the content-height cap.
pub const CAPPED_MARKER: &str = " [recortado]";

/// Collapses runs of ASCII whitespace into single spaces and trims the ends.
pub fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Truncates `text` to `max` characters, appending `…` when it was shortened.
fn truncate_chars(text: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let mut chars = text.char_indices();
    let mut cut = None;
    for (count, (index, _)) in chars.by_ref().enumerate() {
        if count == max {
            cut = Some(index);
            break;
        }
    }
    match cut {
        Some(index) => format!("{}…", &text[..index]),
        None => text.to_owned(),
    }
}

/// Concatenates the descendant text nodes of `node_id` (used for `<title>` content).
fn text_content(doc: &Document, node_id: NodeId) -> String {
    let mut out = String::new();
    let mut stack: Vec<NodeId> = Vec::new();
    if let Some(child) = doc.node(node_id).first_child {
        stack.push(child);
    }
    while let Some(id) = stack.pop() {
        let node = doc.node(id);
        if let NodeKind::Text(text) = &node.kind {
            out.push_str(text);
        }
        if let Some(sibling) = node.next_sibling {
            stack.push(sibling);
        }
        if let Some(child) = node.first_child {
            stack.push(child);
        }
    }
    out
}

/// Returns the document's `<title>` text (HTML namespace), collapsed and capped.
///
/// Returns `None` when there is no `<title>` element or its text is empty. The first
/// `<title>` in tree order wins (the HTML parser keeps at most one in `<head>`, but
/// foreign/serialized documents may contain more).
pub fn document_title(doc: &Document) -> Option<String> {
    let mut stack = vec![doc.root()];
    while let Some(id) = stack.pop() {
        let node = doc.node(id);
        if let Some(element) = node.as_element()
            && element.namespace == Namespace::Html
            && element.local_name == "title"
        {
            let text = collapse_whitespace(&text_content(doc, id));
            if text.is_empty() {
                return None;
            }
            return Some(truncate_chars(&text, MAX_TITLE_CHARS));
        }
        if let Some(sibling) = node.next_sibling {
            stack.push(sibling);
        }
        if let Some(child) = node.first_child {
            stack.push(child);
        }
    }
    None
}

/// Returns the URL as shown in the title: fragment stripped, `data:` shortened, capped.
pub fn short_url(url: &Url) -> String {
    let serialized = url.as_str();
    let without_fragment = serialized.split('#').next().unwrap_or(&serialized);
    if without_fragment.starts_with("data:") {
        return "data:…".to_owned();
    }
    truncate_chars(without_fragment, MAX_SHORT_URL_CHARS)
}

/// Builds the page part of the window title: `"<title> — <short url>"`.
pub fn page_title(document_title: Option<&str>, url: &Url) -> String {
    let short = short_url(url);
    match document_title.map(collapse_whitespace) {
        Some(title) if !title.is_empty() => {
            format!("{} — {short}", truncate_chars(&title, MAX_TITLE_CHARS))
        }
        _ => format!("{FALLBACK_NAME} — {short}"),
    }
}

/// Builds the complete window title from the page title and window state.
///
/// Errors take precedence (`"Megi — error: <message>"`), then the scroll percentage and
/// the clipping marker are appended to the page title.
pub fn window_title(page: &str, percent: u32, error: Option<&str>, capped: bool) -> String {
    if let Some(error) = error {
        let error = collapse_whitespace(error);
        return truncate_chars(&format!("{FALLBACK_NAME} — error: {error}"), 240);
    }
    let mut title = format!("{page} — {percent}%");
    if capped {
        title.push_str(CAPPED_MARKER);
    }
    title
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(raw: &str) -> Url {
        Url::parse(raw).expect("test URL parses")
    }

    fn doc_with_body(body: &str) -> Document {
        crate::headless::render_target(
            &format!("data:text/html,<html><body>{body}</body></html>"),
            200,
            100,
        )
        .expect("data URL renders")
        .document
    }

    #[test]
    fn collapse_whitespace_trims_and_squeezes() {
        assert_eq!(collapse_whitespace("  a \n\t b  "), "a b");
        assert_eq!(collapse_whitespace(""), "");
        assert_eq!(collapse_whitespace("solo"), "solo");
    }

    #[test]
    fn document_title_reads_head_title() {
        let output = crate::headless::render_target(
            "data:text/html,<html><head><title>  My  Page </title></head><body>x</body></html>",
            200,
            100,
        )
        .expect("render");
        assert_eq!(document_title(&output.document).as_deref(), Some("My Page"));
    }

    #[test]
    fn document_title_missing_or_empty_is_none() {
        assert!(document_title(&doc_with_body("<p>no title</p>")).is_none());
        let output = crate::headless::render_target(
            "data:text/html,<html><head><title>   </title></head><body>x</body></html>",
            200,
            100,
        )
        .expect("render");
        assert!(document_title(&output.document).is_none());
    }

    #[test]
    fn document_title_collapses_nested_text_and_caps_length() {
        let long = "x".repeat(MAX_TITLE_CHARS + 50);
        let output = crate::headless::render_target(
            &format!(
                "data:text/html,<html><head><title>a {long}</title></head><body>x</body></html>"
            ),
            200,
            100,
        )
        .expect("render");
        let title = document_title(&output.document).expect("title");
        assert!(title.ends_with('…'));
        assert!(title.chars().count() <= MAX_TITLE_CHARS + 1);
    }

    #[test]
    fn short_url_strips_fragments_and_shortens_data_urls() {
        assert_eq!(
            short_url(&url("https://example.com/a?b=1#frag")),
            "https://example.com/a?b=1"
        );
        assert_eq!(
            short_url(&url("file:///tmp/page.html")),
            "file:///tmp/page.html"
        );
        assert_eq!(short_url(&url("data:text/html,<p>hi</p>")), "data:…");
    }

    #[test]
    fn short_url_caps_long_urls() {
        let long = format!("https://example.com/{}", "a".repeat(200));
        let short = short_url(&url(&long));
        assert!(short.ends_with('…'));
        assert!(short.chars().count() <= MAX_SHORT_URL_CHARS + 1);
    }

    #[test]
    fn page_title_combines_title_and_url() {
        assert_eq!(
            page_title(
                Some("My Page"),
                &url("https://example.com/docs/page.html#x")
            ),
            "My Page — https://example.com/docs/page.html"
        );
        assert_eq!(
            page_title(None, &url("https://example.com/")),
            "Megi — https://example.com/"
        );
        assert_eq!(
            page_title(Some("   "), &url("https://example.com/")),
            "Megi — https://example.com/"
        );
    }

    #[test]
    fn window_title_appends_scroll_and_cap_marker() {
        assert_eq!(
            window_title("My Page — https://example.com/", 0, None, false),
            "My Page — https://example.com/ — 0%"
        );
        assert_eq!(
            window_title("My Page — https://example.com/", 42, None, true),
            "My Page — https://example.com/ — 42% [recortado]"
        );
    }

    #[test]
    fn window_title_prefers_errors() {
        assert_eq!(
            window_title(
                "My Page — https://example.com/",
                10,
                Some("connection refused"),
                false
            ),
            "Megi — error: connection refused"
        );
    }

    #[test]
    fn truncate_chars_respects_boundaries() {
        assert_eq!(truncate_chars("abc", 5), "abc");
        assert_eq!(truncate_chars("abcdef", 3), "abc…");
        assert_eq!(truncate_chars("añbó", 2), "añ…");
        assert_eq!(truncate_chars("abc", 0), "");
    }
}
