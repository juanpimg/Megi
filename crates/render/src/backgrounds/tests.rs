//! Unit tests for the background image loading stage.

use std::collections::HashMap;
use std::time::Duration;

use megi_core::{Document, NodeId};
use megi_css::ComputedStyle;
use megi_net::{FetchOptions, ResourceLoader, Url};

use super::*;

#[cfg(test)]
mod unit_tests {
    use super::*;

    fn options() -> FetchOptions {
        FetchOptions {
            connect_timeout: Duration::from_secs(2),
            read_timeout: Duration::from_secs(2),
            ..FetchOptions::default()
        }
    }

    fn styled_divs(
        backgrounds: &[&str],
        list_images: &[&str],
    ) -> (Document, HashMap<NodeId, ComputedStyle>) {
        let mut doc = Document::new();
        let body = doc.create_element(megi_core::Namespace::Html, "body", vec![]);
        doc.append_child(doc.root(), body);
        let mut styles = HashMap::new();
        for raw_url in backgrounds {
            let node = doc.create_element(megi_core::Namespace::Html, "div", vec![]);
            doc.append_child(body, node);
            let mut style = ComputedStyle::initial();
            style.background_image = Some((*raw_url).to_owned());
            styles.insert(node, style);
        }
        for raw_url in list_images {
            let node = doc.create_element(megi_core::Namespace::Html, "ul", vec![]);
            doc.append_child(body, node);
            let mut style = ComputedStyle::initial();
            style.list_style_image = Some((*raw_url).to_owned());
            styles.insert(node, style);
        }
        (doc, styles)
    }

    #[test]
    fn collects_references_in_document_order() {
        let (doc, styles) = styled_divs(&["a.png"], &["b.png"]);
        let references = collect_background_references(&doc, &styles);
        assert_eq!(references.len(), 2);
        assert_eq!(references[0].kind, BackgroundKind::Background);
        assert_eq!(references[0].raw_url, "a.png");
        assert_eq!(references[1].kind, BackgroundKind::ListStyleImage);
        assert_eq!(references[1].raw_url, "b.png");
        assert!(references[0].node.index() < references[1].node.index());
    }

    #[test]
    fn invalid_and_blocked_urls_are_non_fatal() {
        let (doc, styles) = styled_divs(
            &["http://exa mple.com/bg.png", "http://127.0.0.1:9/a.png"],
            &[],
        );
        let base = Url::parse("file:///tmp/index.html").expect("file URL");
        let mut loader = ResourceLoader::with_options(options());
        let mut warnings = Vec::new();
        let loaded = load_backgrounds(
            &doc,
            &styles,
            &base,
            &mut loader,
            true, // --no-network: the http URL is skipped without connecting
            BackgroundLimits::default(),
            &mut warnings,
        );
        assert_eq!(loaded.references, 2);
        assert_eq!(loaded.loaded, 0);
        assert_eq!(loaded.failed, 2);
        assert_eq!(loaded.images.len(), 0);
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("--no-network"))
        );
    }

    #[test]
    fn background_cap_counts_distinct_urls() {
        let (doc, styles) = styled_divs(&["a.png", "b.png", "c.png"], &[]);
        // Unique base directory so unrelated files in /tmp (from other tools or parallel
        // test runs) cannot make the "file not found" assertions flaky.
        let base = Url::parse(&format!(
            "file:///tmp/megi-render-backgrounds-cap-{}/index.html",
            std::process::id()
        ))
        .expect("file URL");
        let mut loader = ResourceLoader::with_options(options());
        let mut warnings = Vec::new();
        let limits = BackgroundLimits {
            max_backgrounds: 2,
            ..BackgroundLimits::default()
        };
        let loaded = load_backgrounds(
            &doc,
            &styles,
            &base,
            &mut loader,
            false,
            limits,
            &mut warnings,
        );
        // The third distinct URL is refused before any I/O; the first two fail with
        // file-not-found (still counted as failed attempts).
        assert_eq!(loaded.references, 3);
        assert_eq!(loaded.loaded, 0);
        assert_eq!(loaded.failed, 3);
        assert!(warnings.iter().any(|warning| warning.contains("(cap)")));
    }

    #[test]
    fn missing_files_are_warnings_not_errors() {
        let (doc, styles) = styled_divs(&["does-not-exist.png"], &[]);
        let base =
            Url::parse("file:///tmp/megi-render-backgrounds-test/index.html").expect("file URL");
        let mut loader = ResourceLoader::with_options(options());
        let mut warnings = Vec::new();
        let loaded = load_backgrounds(
            &doc,
            &styles,
            &base,
            &mut loader,
            false,
            BackgroundLimits::default(),
            &mut warnings,
        );
        assert_eq!(loaded.loaded, 0);
        assert_eq!(loaded.failed, 1);
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("skipping background image"))
        );
    }

    #[test]
    fn empty_styles_produce_empty_map() {
        let doc = Document::new();
        let styles = HashMap::new();
        let base = Url::parse("file:///tmp/index.html").expect("file URL");
        let mut loader = ResourceLoader::with_options(options());
        let mut warnings = Vec::new();
        let loaded = load_backgrounds(
            &doc,
            &styles,
            &base,
            &mut loader,
            false,
            BackgroundLimits::default(),
            &mut warnings,
        );
        assert!(loaded.is_empty());
        assert_eq!(loaded.references, 0);
        assert!(warnings.is_empty());
    }
}
