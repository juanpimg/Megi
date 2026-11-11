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

use dom::{Document, NodeId};
use layout::LayoutBox;

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
    base_url: Option<&net::Url>,
) -> Vec<ImageRegion> {
    let mut images = Vec::new();
    let mut stack: Vec<&LayoutBox> = vec![layout];
    while let Some(layout_box) = stack.pop() {
        if let Some(node_id) = layout_box.node_id()
            && let Some(element) = document.node(node_id).as_element()
            && element.local_name == "img"
            && let Some(src) = element.get_attribute("src")
        {
            let rect = layout_box.dimensions.border_box();
            if rect.width > 0.0 && rect.height > 0.0 {
                let url = match base_url {
                    Some(base) => base
                        .join(src)
                        .map(|resolved| resolved.as_str())
                        .unwrap_or_default(),
                    None => net::Url::parse(src)
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

#[cfg(test)]
mod tests {
    use super::*;

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
