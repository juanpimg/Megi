//! Image hit regions for shell interactions (context menu "Save image").
//!
//! Extracted from the layout tree like [`crate::links`]: every rendered `<img>` element with
//! a `src` attribute becomes an [`ImageRegion`] in page CSS coordinates (the same frame as
//! the pixels, so hit-testing matches what the user sees). Only `<img>` elements are
//! tracked; CSS background images are a documented gap (they have no URL in the layout).
//!
//! Limits: no lazily loaded/`srcset` selection beyond the parsed attribute, no image bytes
//! cached here (the shell re-fetches the resource on demand when saving; see
//! [`crate::download::save_image`]).

use megi_webview::core::{Document, NodeId};
use megi_webview::layout::LayoutBox;

use crate::links::CssRect;

/// One rendered image.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageRegion {
    /// DOM node id of the `<img>` element.
    pub node_id: NodeId,
    /// Border box in page CSS pixels.
    pub rect: CssRect,
    /// Resolved absolute image URL (empty when `src` is missing/invalid).
    pub url: String,
}

impl ImageRegion {
    /// Whether the URL is usable for saving.
    pub fn is_usable(&self) -> bool {
        !self.url.is_empty()
    }
}

/// Collects the rendered `<img>` regions in painting/document order.
pub fn collect_image_regions(
    document: &Document,
    layout: &LayoutBox,
    base_url: Option<&megi_webview::net::Url>,
) -> Vec<ImageRegion> {
    let mut images = Vec::new();
    let mut stack: Vec<&LayoutBox> = vec![layout];
    while let Some(layout_box) = stack.pop() {
        if let Some(node_id) = layout_box.node_id()
            && let Some(element) = document.node(node_id).as_element()
            && element.local_name == "img"
            && let Some(src) = megi_webview::render::images::resolve_img_source(
                document,
                node_id,
                &megi_webview::css::MediaContext::default(),
                1.0,
            )
        {
            let rect = layout_box.dimensions.border_box();
            if rect.width > 0.0 && rect.height > 0.0 {
                let url = match base_url {
                    Some(base) => base
                        .join(&src)
                        .map(|resolved| resolved.as_str())
                        .unwrap_or_default(),
                    None => megi_webview::net::Url::parse(&src)
                        .map(|resolved| resolved.as_str())
                        .unwrap_or_default(),
                };
                images.push(ImageRegion {
                    node_id,
                    rect: CssRect::new(rect.x, rect.y, rect.width, rect.height),
                    url,
                });
            }
        }
        for child in layout_box.children.iter().rev() {
            stack.push(child);
        }
    }
    images
}

/// Topmost image containing page point `(x, y)`.
pub fn image_at(images: &[ImageRegion], x: f32, y: f32) -> Option<&ImageRegion> {
    images.iter().rev().find(|image| image.rect.contains(x, y))
}

/// Builds a resource loader for an on-demand save using the window's session state
/// (#1829). A bare `ResourceLoader::new()` drops the cookie jar and CORS `Origin`, so
/// authenticated images are re-fetched anonymously and the server returns a login page.
/// Mirrors the subresource fetch setup in `megi_render`'s final stage.
pub fn loader_for_session_save(
    options: &megi_webview::render::RenderOptions,
    document_url: Option<&megi_webview::net::Url>,
) -> megi_webview::net::ResourceLoader {
    use megi_webview::net::Scheme;

    let mut fetch_options = options.fetch_options.clone();
    fetch_options.cookie_jar = options.cookie_jar.clone();
    if fetch_options.cookie_jar.is_none() && options.cookies {
        fetch_options.cookie_jar = Some(std::sync::Arc::new(std::sync::Mutex::new(
            megi_webview::net::CookieJar::new(),
        )));
    }
    let origin = options.origin.clone().or_else(|| {
        document_url.and_then(|url| match url.scheme() {
            Scheme::Http | Scheme::Https => {
                megi_webview::net::Origin::from_url(url).map(|origin| origin.as_str())
            }
            Scheme::File | Scheme::Data => None,
        })
    });
    if let Some(origin) = origin {
        fetch_options.origin = Some(origin);
    }
    let mut loader = megi_webview::net::ResourceLoader::with_options(fetch_options);
    if let Some(url) = document_url {
        loader.set_mixed_content_context(Some(url.scheme()));
    }
    loader
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_save_loader_inherits_cookie_jar_and_origin() {
        // #1829: re-fetching an image for "Save image" must reuse the session's cookies
        // and Origin, otherwise authenticated resources return the login page.
        let jar = std::sync::Arc::new(std::sync::Mutex::new(megi_webview::net::CookieJar::new()));
        let options = megi_webview::render::RenderOptions {
            cookie_jar: Some(std::sync::Arc::clone(&jar)),
            ..Default::default()
        };
        let url = megi_webview::net::Url::parse("https://app.test/page").expect("url");

        let loader = loader_for_session_save(&options, Some(&url));
        let fetch = loader.fetch_options();
        assert!(
            fetch.cookie_jar.is_some(),
            "the session cookie jar must be reused"
        );
        assert_eq!(fetch.origin.as_deref(), Some("https://app.test"));
    }

    #[test]
    fn session_save_loader_without_document_has_no_origin() {
        let options = megi_webview::render::RenderOptions::default();
        let loader = loader_for_session_save(&options, None);
        assert!(loader.fetch_options().cookie_jar.is_none());
        assert!(loader.fetch_options().origin.is_none());
    }

    #[test]
    fn image_regions_are_extracted_from_a_rendered_page() {
        // The helper above only returns metadata; use the render output directly.
        let output = crate::headless::render_target(
            "data:text/html,<img src='file:///tmp/a.png' width=40 height=20>",
            200,
            120,
        )
        .expect("render");
        let images =
            collect_image_regions(&output.document, &output.layout, Some(&output.document_url));
        assert_eq!(images.len(), 1, "one <img>: {images:?}");
        assert!(images[0].is_usable());
        assert!(images[0].url.contains("a.png"));
        assert!(images[0].rect.width > 0.0);
        let hit = image_at(&images, images[0].rect.x + 2.0, images[0].rect.y + 2.0);
        assert_eq!(hit.map(|image| image.node_id), Some(images[0].node_id));
        assert!(image_at(&images, 1.0, 1.0).is_none() || images[0].rect.x <= 1.0);
    }

    #[test]
    fn missing_src_or_non_image_layouts_are_ignored() {
        let output = crate::headless::render_target(
            "data:text/html,<div style='width:20px;height:10px'></div><img alt=no-src>",
            200,
            120,
        )
        .expect("render");
        let images = collect_image_regions(&output.document, &output.layout, None);
        assert!(images.is_empty());
    }
}
