//! Link hit-testing and URL resolution (pure, display-free).
//!
//! The render worker extracts, from a finished [`render::RenderOutput`], one
//! [`LinkRegion`] per painted primitive that resolves to an `<a href>`: its page
//! rectangle in CSS pixels (origin = top-left of the full document, no scroll) and the raw
//! `href` attribute. The primary source is the additive hit-testing layer of `paint`
//! (`RenderOutput::hit_regions` produced by the same paint pass as the pixels): for every
//! [`paint::HitRegion`] the nearest HTML `<a href>` ancestor is resolved with
//! [`paint::find_link_at`]. When the render output carries no hit regions (e.g. a
//! hand-built `RenderOutput` in a test, or a page with no painted content) a documented
//! fallback walks the layout tree (`LayoutBox::node_id()` → DOM `<a href>`) instead.
//!
//! The window converts a physical cursor position into page CSS coordinates with
//! [`physical_to_page`] (divide by the HiDPI scale factor, add the scroll offset) and asks
//! [`link_url_at`] which URL a left click activates. Regions are stored next to the frame,
//! so after every re-render the hit map matches the pixels on screen.
//!
//! ## Behaviour and limits
//!
//! - Topmost wins: regions are searched in reverse paint order.
//! - Empty/`href`-less anchors are not regions; anchors with zero-area boxes are skipped.
//! - The raw `href` is resolved against the page URL with [`net::Url::join`]; `javascript:`
//!   and unknown schemes are not navigable (documented no-op), while `http(s)`, `file:`
//!   and `data:` are.
//! - Fragment-only links (`#section`) resolve to the same document URL with a fragment and
//!   therefore count as a new navigation in this round (no in-page smooth scrolling yet).
//! - No forms, no `<area>`, no `javascript:` execution, no downloads.

use dom::Namespace;
use layout::LayoutBox;

/// Rectangle in CSS page coordinates (CSS px, document origin, no scroll applied).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CssRect {
    /// Left edge in CSS px.
    pub x: f32,
    /// Top edge in CSS px.
    pub y: f32,
    /// Width in CSS px.
    pub width: f32,
    /// Height in CSS px.
    pub height: f32,
}

impl CssRect {
    /// Creates a rectangle.
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Returns `true` when `(x, y)` lies inside the half-open rectangle.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }

    /// Right edge (exclusive).
    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    /// Bottom edge (exclusive).
    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }
}

/// One clickable `<a href>` region in page coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct LinkRegion {
    /// Page-space rectangle covered by the anchor's border box.
    pub rect: CssRect,
    /// Raw `href` attribute value (not resolved).
    pub href: String,
}

/// Errors produced by [`resolve_href`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkError {
    /// The `href` was empty after trimming.
    Empty,
    /// The `href` exceeded [`MAX_HREF_LEN`] characters.
    TooLong,
    /// The URL resolver rejected the `href` (malformed or unsupported).
    Invalid(net::UrlError),
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("empty href"),
            Self::TooLong => write!(f, "href exceeds {MAX_HREF_LEN} characters"),
            Self::Invalid(error) => write!(f, "invalid href: {error}"),
        }
    }
}

impl std::error::Error for LinkError {}

/// Maximum accepted `href` length (defensive cap; real links are far shorter).
pub const MAX_HREF_LEN: usize = 4096;

/// Returns `true` for the schemes the shell is allowed to navigate to.
pub fn is_navigable(url: &net::Url) -> bool {
    matches!(
        url.scheme(),
        net::Scheme::Http | net::Scheme::Https | net::Scheme::File | net::Scheme::Data
    )
}

/// Resolves a raw `href` against the current page URL.
///
/// Whitespace around the attribute value is trimmed (HTML attribute semantics); an empty
/// `href` is rejected.
pub fn resolve_href(base: &net::Url, href: &str) -> Result<net::Url, LinkError> {
    let href = href.trim();
    if href.is_empty() {
        return Err(LinkError::Empty);
    }
    if href.chars().count() > MAX_HREF_LEN {
        return Err(LinkError::TooLong);
    }
    match base.join(href) {
        Ok(url) => Ok(url),
        // `Url::join` only resolves relative references against `http(s)`/`file` bases and
        // rejects absolute references with a non-special scheme; accept a standalone
        // absolute URL (`data:`, or a scheme the shell will treat as non-navigable).
        Err(join_error) => net::Url::parse(href).map_err(|parse_error| {
            let _ = join_error;
            LinkError::Invalid(parse_error)
        }),
    }
}

/// Returns the topmost region containing `(x, y)` (reverse paint order).
pub fn find_link_at(regions: &[LinkRegion], x: f32, y: f32) -> Option<&LinkRegion> {
    regions
        .iter()
        .rev()
        .find(|region| region.rect.contains(x, y))
}

/// Converts a physical cursor position into page CSS coordinates.
///
/// `scale_factor` is the window's HiDPI scale (`physical / CSS`); `scroll_offset` is the
/// current vertical scroll offset in CSS px (added because layout regions are page-space).
pub fn physical_to_page(
    physical_x: f64,
    physical_y: f64,
    scale_factor: f64,
    scroll_offset: u32,
) -> (f32, f32) {
    let scale = if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };
    (
        (physical_x / scale) as f32,
        (physical_y / scale) as f32 + scroll_offset as f32,
    )
}

/// Returns the URL a click at page position `(x, y)` would navigate to.
///
/// `Ok(None)` means "no action": no link under the cursor, an unresolvable `href`, or a
/// non-navigable scheme (`javascript:`, `mailto:`, ...).
pub fn link_url_at(
    page_url: Option<&net::Url>,
    regions: &[LinkRegion],
    x: f32,
    y: f32,
) -> Result<Option<net::Url>, LinkError> {
    let Some(region) = find_link_at(regions, x, y) else {
        return Ok(None);
    };
    let Some(base) = page_url else {
        return Ok(None);
    };
    let url = match resolve_href(base, &region.href) {
        Ok(url) => url,
        // Browsers ignore click handlers for unparsable URLs (`javascript:` with bad syntax,
        // malformed escapes, ...): the click is a documented no-op.
        Err(LinkError::Invalid(_)) => return Ok(None),
        Err(error) => return Err(error),
    };
    Ok(is_navigable(&url).then_some(url))
}

/// Extracts one [`LinkRegion`] per painted primitive that resolves to an `<a href>`.
///
/// Primary path: uses the additive `paint` hit regions from the render output
/// ([`render::RenderOutput::hit_regions`]) and resolves the nearest HTML `<a href>`
/// ancestor with [`paint::find_link_at`] for each region. This reuses the exact painted
/// geometry of the frame (clipped regions, text runs inheriting the anchor's DOM node,
/// images and decorations inside links).
///
/// Fallback (documented): when the render output has no hit regions, the layout tree is
/// walked iteratively and element boxes are matched against the parsed DOM through
/// [`LayoutBox::node_id`]. Anchors with an empty/absent `href` or a zero-area border box do
/// not produce a region.
pub fn collect_link_regions(output: &render::RenderOutput) -> Vec<LinkRegion> {
    if output.hit_regions.is_empty() {
        return collect_link_regions_from_layout(output);
    }
    let mut regions = Vec::new();
    for hit_region in &output.hit_regions {
        let rect = hit_region.rect;
        let center_x = rect.x + rect.width / 2.0;
        let center_y = rect.y + rect.height / 2.0;
        if let Some(link) = paint::find_link_at(
            &output.document,
            std::slice::from_ref(hit_region),
            center_x,
            center_y,
        ) && rect.width > 0.0
            && rect.height > 0.0
        {
            let region = LinkRegion {
                rect: CssRect::new(rect.x, rect.y, rect.width, rect.height),
                href: link.href,
            };
            // Several painted primitives can cover the same anchor rectangle (e.g. a text
            // run plus its underline); keep one region per exact (href, rect) pair.
            if !regions.contains(&region) {
                regions.push(region);
            }
        }
    }
    if regions.is_empty() {
        // No painted primitive resolved to an anchor: fall back to the layout tree rather
        // than silently reporting an unclickable page.
        return collect_link_regions_from_layout(output);
    }
    regions
}

/// Documented fallback for [`collect_link_regions`]: walks the layout tree and matches
/// element boxes to DOM `<a href>` nodes (`LayoutBox::node_id`).
fn collect_link_regions_from_layout(output: &render::RenderOutput) -> Vec<LinkRegion> {
    let mut regions = Vec::new();
    let mut stack: Vec<&LayoutBox> = vec![&output.layout];
    while let Some(layout_box) = stack.pop() {
        if let Some(node_id) = layout_box.node_id()
            && let Some(element) = output.document.node(node_id).as_element()
            && element.namespace == Namespace::Html
            && element.local_name == "a"
            && let Some(href) = element.get_attribute("href")
            && !href.trim().is_empty()
        {
            let rect = layout_box.dimensions.border_box();
            if rect.width > 0.0 && rect.height > 0.0 {
                regions.push(LinkRegion {
                    rect: CssRect::new(rect.x, rect.y, rect.width, rect.height),
                    href: href.to_owned(),
                });
            }
        }
        for child in layout_box.children.iter().rev() {
            stack.push(child);
        }
    }
    regions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(raw: &str) -> net::Url {
        net::Url::parse(raw).expect("test URL parses")
    }

    fn region(x: f32, y: f32, width: f32, height: f32, href: &str) -> LinkRegion {
        LinkRegion {
            rect: CssRect::new(x, y, width, height),
            href: href.to_owned(),
        }
    }

    #[test]
    fn rect_contains_uses_half_open_bounds() {
        let rect = CssRect::new(10.0, 20.0, 100.0, 30.0);
        assert!(rect.contains(10.0, 20.0));
        assert!(rect.contains(109.9, 49.9));
        assert!(
            !rect.contains(110.0, 30.0),
            "right/bottom edges are exclusive"
        );
        assert!(!rect.contains(9.9, 20.0));
        assert_eq!((rect.right(), rect.bottom()), (110.0, 50.0));
    }

    #[test]
    fn find_link_at_returns_topmost_region() {
        let regions = vec![
            region(0.0, 0.0, 100.0, 100.0, "https://example.com/background"),
            region(10.0, 10.0, 20.0, 20.0, "https://example.com/foreground"),
        ];
        assert_eq!(
            find_link_at(&regions, 20.0, 20.0).map(|link| link.href.as_str()),
            Some("https://example.com/foreground")
        );
        assert_eq!(
            find_link_at(&regions, 80.0, 80.0).map(|link| link.href.as_str()),
            Some("https://example.com/background")
        );
        assert!(find_link_at(&regions, 500.0, 500.0).is_none());
    }

    #[test]
    fn resolve_relative_absolute_and_fragment_hrefs() {
        let base = url("https://example.com/docs/page.html");
        assert_eq!(
            resolve_href(&base, "other.html").unwrap().as_str(),
            "https://example.com/docs/other.html"
        );
        assert_eq!(
            resolve_href(&base, "/root.html").unwrap().as_str(),
            "https://example.com/root.html"
        );
        assert_eq!(
            resolve_href(&base, "  https://other.example/x  ")
                .unwrap()
                .as_str(),
            "https://other.example/x"
        );
        assert_eq!(
            resolve_href(&base, "#section").unwrap().as_str(),
            "https://example.com/docs/page.html#section"
        );
        assert_eq!(
            resolve_href(&base, "../up.html").unwrap().as_str(),
            "https://example.com/up.html"
        );
    }

    #[test]
    fn resolve_href_rejects_empty_and_oversized_values() {
        let base = url("https://example.com/");
        assert_eq!(resolve_href(&base, ""), Err(LinkError::Empty));
        assert_eq!(resolve_href(&base, "   "), Err(LinkError::Empty));
        assert_eq!(
            resolve_href(&base, &"a".repeat(MAX_HREF_LEN + 1)),
            Err(LinkError::TooLong)
        );
    }

    #[test]
    fn resolve_href_supports_file_and_data_urls() {
        let base = url("file:///tmp/page.html");
        assert_eq!(
            resolve_href(&base, "other.html").unwrap().as_str(),
            "file:///tmp/other.html"
        );
        let data = resolve_href(&url("https://example.com/"), "data:text/html,x").unwrap();
        assert!(matches!(data.scheme(), net::Scheme::Data));
    }

    #[test]
    fn navigable_schemes_are_limited_to_network_file_and_data() {
        assert!(is_navigable(&url("http://example.com/")));
        assert!(is_navigable(&url("https://example.com/")));
        assert!(is_navigable(&url("file:///tmp/x.html")));
        assert!(is_navigable(&url("data:text/html,x")));
        // Non-special schemes are rejected by the URL resolver (`UnsupportedScheme`), so
        // they can never reach `is_navigable`; the resolver error is documented.
        let base = url("https://example.com/");
        assert!(matches!(
            resolve_href(&base, "mailto:a@example.com"),
            Err(LinkError::Invalid(net::UrlError::UnsupportedScheme(_)))
        ));
        assert!(matches!(
            resolve_href(&base, "javascript:alert(1)"),
            Err(LinkError::Invalid(net::UrlError::UnsupportedScheme(_)))
        ));
    }

    #[test]
    fn link_url_at_combines_hit_test_and_resolution() {
        let page = url("https://example.com/docs/page.html");
        let regions = vec![
            region(0.0, 0.0, 50.0, 20.0, "next.html"),
            region(0.0, 40.0, 50.0, 20.0, "mailto:a@example.com"),
        ];
        assert_eq!(
            link_url_at(Some(&page), &regions, 10.0, 10.0)
                .unwrap()
                .map(|url| url.as_str()),
            Some("https://example.com/docs/next.html".to_owned())
        );
        assert_eq!(
            link_url_at(Some(&page), &regions, 10.0, 50.0).unwrap(),
            None,
            "non-navigable schemes are a no-op"
        );
        assert_eq!(
            link_url_at(Some(&page), &regions, 500.0, 500.0).unwrap(),
            None
        );
        assert_eq!(
            link_url_at(None, &regions, 10.0, 10.0).unwrap(),
            None,
            "clicking before the first page URL is a no-op"
        );
    }

    #[test]
    fn link_url_at_ignores_unparsable_hrefs() {
        let page = url("https://example.com/");
        let regions = vec![region(0.0, 0.0, 10.0, 10.0, "http://[bad")];
        assert_eq!(
            link_url_at(Some(&page), &regions, 5.0, 5.0).unwrap(),
            None,
            "browser-style no-op for unparsable hrefs"
        );
    }

    #[test]
    fn zoomed_metrics_drive_page_coordinates() {
        // Page hit-testing must divide by the effective scale (device scale * page zoom),
        // the same factor `present_region` uses to map CSS pixels to the window.
        let metrics = crate::scale::window_metrics_zoomed(800, 600, 1.0, 2.0);
        assert_eq!((metrics.css_width, metrics.css_height), (400, 300));
        assert_eq!(
            physical_to_page(100.0, 100.0, metrics.scale, 0),
            (50.0, 50.0)
        );
    }

    #[test]
    fn physical_to_page_applies_scale_and_scroll() {
        assert_eq!(physical_to_page(100.0, 200.0, 2.0, 50), (50.0, 150.0));
        assert_eq!(physical_to_page(10.0, 10.0, 1.0, 0), (10.0, 10.0));
        // Degenerate scale factors fall back to 1.0 instead of dividing by zero.
        assert_eq!(physical_to_page(10.0, 10.0, 0.0, 5), (10.0, 15.0));
        assert_eq!(physical_to_page(10.0, 10.0, f64::NAN, 0), (10.0, 10.0));
    }

    #[test]
    fn collect_link_regions_finds_anchors_in_a_rendered_page() {
        let output = crate::headless::render_target(
            "data:text/html,<body><a href=\"next.html\">next page</a><p>text</p></body>",
            320,
            120,
        )
        .expect("data URL renders");
        let regions = collect_link_regions(&output);
        assert_eq!(regions.len(), 1, "regions: {regions:?}");
        assert_eq!(regions[0].href, "next.html");
        assert!(regions[0].rect.width > 0.0);
        assert!(regions[0].rect.height > 0.0);
        let center_x = regions[0].rect.x + regions[0].rect.width / 2.0;
        let center_y = regions[0].rect.y + regions[0].rect.height / 2.0;
        let page = net::Url::parse("https://example.com/docs/page.html").expect("page URL");
        let resolved = link_url_at(Some(&page), &regions, center_x, center_y)
            .expect("resolution")
            .expect("navigable");
        assert_eq!(resolved.as_str(), "https://example.com/docs/next.html");
    }

    #[test]
    fn collect_link_regions_skips_anchors_without_href() {
        let output = crate::headless::render_target(
            "data:text/html,<body><a name=\"x\">no href</a><a href=\"%23frag\">frag</a></body>",
            320,
            120,
        )
        .expect("data URL renders");
        let regions = collect_link_regions(&output);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].href, "#frag");
    }
}
