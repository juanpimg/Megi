//! Deterministic reader-mode extraction (`about:reader` / `megi://reader`).
//!
//! The extractor is intentionally simple and deterministic: it skips navigation/aside/
//! footer/script/style/header/form subtrees, picks the candidate container (`article`,
//! `main`, `section`, `div`, `body`) with the **most visible text**, and turns its
//! heading/paragraph/list/quote blocks into a clean article. There is no scoring model,
//! no timers and no scripting; the same DOM always yields the same HTML.

use dom::{Document, NodeId};

/// Tags whose text is never part of the article.
const SKIPPED: &[&str] = &[
    "nav", "aside", "footer", "header", "script", "style", "noscript", "form", "svg", "iframe",
    "template",
];

/// Tags that can contain the article.
const CONTAINERS: &[&str] = &["article", "main", "section", "div", "body"];

/// Tags extracted as article blocks.
const BLOCKS: &[&str] = &["h1", "h2", "h3", "h4", "p", "li", "blockquote", "pre", "td"];

/// Extracted reader article.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Article {
    /// Article heading (first `<h1>` or document title fallback).
    pub title: String,
    /// Clean paragraph/heading blocks in document order.
    pub paragraphs: Vec<String>,
}

/// Whether `url` is the generated reader page.
pub fn is_reader_url(url: &str) -> bool {
    matches!(
        url.trim().to_ascii_lowercase().as_str(),
        "about:reader" | "megi://reader" | "megi://reader/"
    )
}

fn is_skipped(name: &str) -> bool {
    SKIPPED.contains(&name)
}

const MAX_READER_NODES: usize = 100_000;

fn collect_text(document: &Document, node: NodeId, out: &mut String) {
    // Iterative traversal with a visit budget to avoid stack overflow on deeply
    // nested documents.
    let mut stack = vec![node];
    let mut node_count = 0usize;
    while let Some(current) = stack.pop() {
        if node_count >= MAX_READER_NODES {
            break;
        }
        node_count += 1;
        let Some(data) = document.try_node(current) else {
            continue;
        };
        if let Some(element) = data.as_element()
            && is_skipped(&element.local_name)
        {
            continue;
        }
        if let Some(text) = data.as_text() {
            out.push_str(text);
            out.push(' ');
            continue;
        }
        for child in document.children(current) {
            stack.push(child);
        }
    }
}

fn visible_text(document: &Document, node: NodeId) -> String {
    let mut text = String::new();
    collect_text(document, node, &mut text);
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Extracts the main article from `document` (returns `None` for empty/too-short pages).
pub fn extract(document: &Document) -> Option<Article> {
    let body = document.body()?;
    let mut best: Option<(NodeId, usize)> = None;
    // Memoize visible text lengths to avoid O(N^2) re-walks of nested containers.
    let mut cache: std::collections::HashMap<NodeId, usize> = std::collections::HashMap::new();
    for (node_count, node) in document.descendants(body).enumerate() {
        if node_count >= MAX_READER_NODES {
            break;
        }
        let Some(element) = document.try_node(node).and_then(|node| node.as_element()) else {
            continue;
        };
        if is_skipped(&element.local_name) || !CONTAINERS.contains(&element.local_name.as_str()) {
            continue;
        }
        let length = *cache
            .entry(node)
            .or_insert_with(|| visible_text(document, node).chars().count());
        if length > best.map_or(0, |(_, best)| best) {
            best = Some((node, length));
        }
    }
    let (container, length) = best?;
    if length < 80 {
        return None;
    }
    let mut title = String::new();
    let mut paragraphs = Vec::new();
    for node in document.descendants(container) {
        let Some(element) = document.try_node(node).and_then(|node| node.as_element()) else {
            continue;
        };
        let name = element.local_name.as_str();
        if is_skipped(name) || !BLOCKS.contains(&name) {
            continue;
        }
        let text = visible_text(document, node);
        if text.is_empty() {
            continue;
        }
        if name == "h1" && title.is_empty() {
            title = text.clone();
        }
        if text.chars().count() < 20 && name != "h1" && name != "h2" && name != "h3" {
            continue;
        }
        if paragraphs.last().is_some_and(|last: &String| last == &text) {
            continue;
        }
        paragraphs.push(text);
    }
    if paragraphs.is_empty() {
        return None;
    }
    if title.is_empty() {
        title = paragraphs[0].chars().take(80).collect();
    }
    Some(Article { title, paragraphs })
}

/// Builds the reader page HTML (clean, centered at 700 CSS px, no scripting).
pub fn reader_page(article: &Article, source_url: &str) -> String {
    crate::pages::reader_page(&article.title, &article.paragraphs, source_url)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(html: &str) -> Document {
        html::parse_document(html, true)
    }

    #[test]
    fn extractor_skips_navigation_and_boilerplate() {
        let html = "<!doctype html><html><body>\
            <nav>Home | About | Contact</nav>\
            <aside>Related links</aside>\
            <main><article><h1>My Article</h1>\
            <p>This is the first paragraph of the main article body with enough text to count.</p>\
            <p>This is the second paragraph with even more meaningful article content here.</p>\
            </article></main>\
            <footer>Copyright footer</footer>\
            <script>document.body.textContent='x';</script>\
            </body></html>";
        let article = extract(&doc(html)).expect("article");
        assert_eq!(article.title, "My Article");
        assert!(
            article
                .paragraphs
                .iter()
                .any(|p| p.contains("first paragraph"))
        );
        assert!(
            article
                .paragraphs
                .iter()
                .any(|p| p.contains("second paragraph"))
        );
        let joined = article.paragraphs.join(" ");
        assert!(!joined.contains("Home | About"));
        assert!(!joined.contains("Related links"));
        assert!(!joined.contains("Copyright"));
        assert!(!joined.contains("document.body"));
    }

    #[test]
    fn extractor_picks_the_highest_text_density_container() {
        let html = "<!doctype html><html><body>\
            <div><p>Short card text that is not the article, just a teaser.</p></div>\
            <div><article><h1>Long Read</h1>\
            <p>This long article paragraph carries the majority of the visible text in the page.</p>\
            <p>Another long paragraph continues the article with additional meaningful text.</p>\
            <p>A third paragraph ensures the article container has the highest text density.</p>\
            </article></div></body></html>";
        let article = extract(&doc(html)).expect("article");
        assert_eq!(article.title, "Long Read");
        assert_eq!(article.paragraphs.len(), 4);
        assert!(article.paragraphs.join(" ").contains("majority"));
        assert!(!article.paragraphs.join(" ").contains("teaser"));
    }

    #[test]
    fn empty_or_textless_pages_have_no_article() {
        assert!(extract(&doc("<!doctype html><p>tiny</p>")).is_none());
        assert!(extract(&doc("<!doctype html><main></main>")).is_none());
    }

    #[test]
    fn reader_page_is_clean_centered_and_escaped() {
        let article = Article {
            title: "T <script> & \"quote\"".to_string(),
            paragraphs: vec!["Body <b>bold</b> & more".to_string()],
        };
        let page = reader_page(&article, "https://example.com/a?x=1&y=2");
        assert!(page.contains("max-width:700px"), "{page}");
        assert!(page.contains("serif"), "{page}");
        assert!(!page.contains("<script>"));
        assert!(page.contains("&lt;script&gt;"));
        assert!(page.contains("Body &lt;b&gt;bold&lt;/b&gt; &amp; more"));
        assert!(page.contains("https://example.com/a?x=1&amp;y=2"));
        assert!(!page.to_lowercase().contains("javascript:"));
    }

    #[test]
    fn reader_url_detection() {
        assert!(is_reader_url("about:reader"));
        assert!(is_reader_url("MEGI://reader/"));
        assert!(!is_reader_url("about:history"));
        assert!(!is_reader_url("https://example.com/reader"));
    }
}
