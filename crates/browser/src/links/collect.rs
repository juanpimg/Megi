use super::*;

/// Converts a physical cursor position into page CSS coordinates.
///
/// `scale` is the *effective* CSS scale (`physical / CSS`, i.e. device scale × page zoom);
/// `scroll_offset` is the current vertical scroll offset in CSS px (added because layout
/// regions are page-space).
pub fn physical_to_page(
    physical_x: f64,
    physical_y: f64,
    scale: f64,
    scroll_offset: u32,
) -> (f32, f32) {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
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
    page_url: Option<&megi_webview::net::Url>,
    regions: &[LinkRegion],
    x: f32,
    y: f32,
) -> Result<Option<megi_webview::net::Url>, LinkError> {
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
/// ([`megi_webview::render::RenderOutput::hit_regions`]) and resolves the nearest HTML `<a href>`
/// ancestor with [`megi_webview::paint::find_link_at`] for each region. This reuses the exact painted
/// geometry of the frame (clipped regions, text runs inheriting the anchor's DOM node,
/// images and decorations inside links).
///
/// Fallback (documented): when the render output has no hit regions, the layout tree is
/// walked iteratively and element boxes are matched against the parsed DOM through
/// [`LayoutBox::node_id`]. Anchors with an empty/absent `href` or a zero-area border box do
/// not produce a region.
pub fn collect_link_regions(output: &megi_webview::render::RenderOutput) -> Vec<LinkRegion> {
    if output.hit_regions.is_empty() {
        return collect_link_regions_from_layout(output);
    }
    let mut regions: Vec<LinkRegion> = Vec::new();
    for hit_region in &output.hit_regions {
        let rect = hit_region.rect;
        let center_x = rect.x + rect.width / 2.0;
        let center_y = rect.y + rect.height / 2.0;
        if let Some(link) = megi_webview::paint::find_link_at(
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
                node: Some(link.node_id),
            };
            // Several painted primitives can cover the same anchor (e.g. a text run plus
            // its underline); keep one region per exact (node, rect) pair.
            if !regions
                .iter()
                .any(|existing| existing.node == region.node && existing.rect == region.rect)
            {
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
pub(crate) fn collect_link_regions_from_layout(
    output: &megi_webview::render::RenderOutput,
) -> Vec<LinkRegion> {
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
                    node: Some(node_id),
                });
            }
        }
        for child in layout_box.children.iter().rev() {
            stack.push(child);
        }
    }
    regions
}
