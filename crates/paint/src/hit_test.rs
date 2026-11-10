//! # Hit-testing (Phase 7, WP-7.5)
//!
//! The Phase 4/5 display list is a *flat list of anonymous drawing commands*: it
//! is enough to rasterize pixels, but a browser window also needs to know which
//! DOM node is under the mouse (links, hover, focus, context menu). This module
//! adds the additive hit-testing layer:
//!
//! - [`HitRegion`] records, in painting order, the rectangle covered by each
//!   painted primitive together with the DOM node that generated it and a coarse
//!   [`HitKind`]. Regions are **clipped at emission time**: a region is stored
//!   already intersected with the active clip stack, so hit-tests never report
//!   content hidden by `overflow: hidden`.
//! - [`hit_test`] returns the node of the *last painted* region containing the
//!   point (topmost region under the cursor), and [`hit_test_all`] returns every
//!   containing region top-most first.
//! - [`find_link_at`] walks from the hit node up its DOM ancestors looking for
//!   the nearest HTML `<a href>` and returns the raw (unresolved) `href`.
//!
//! Regions are **optional**: the plain painting entry points
//! ([`crate::build_display_list`], [`crate::build_display_list_with_images`],
//! [`crate::build_display_list_with_images_and_backgrounds`]) do not pay for
//! them, so the Phase 4 golden image and the benchmark stay byte/timing
//! identical. Use [`crate::build_display_list_with_hit_regions`]
//! (or [`crate::build_display_list_and_hit_regions`]) to obtain both the
//! display list and the regions.
//!
//! ## Coordinate contract (with `render`/shell)
//!
//! Coordinates are **CSS px in the document viewport**, origin at the top-left
//! of the viewport, y-down. This engine has no scrolling yet, so the host (the
//! future shell) must add its scroll offset to the pointer position before
//! calling [`hit_test`]/[`find_link_at`], and subtract it when reporting back.
//! Rectangles are half-open on their right/bottom edges
//! (`x <= px < x + width`), so adjacent boxes never claim the same point twice.
//!
//! Regions mirror the painter's coarse primitives: element background/border
//! boxes, text runs, text decoration lines, replaced `<img>` boxes and list
//! markers. Hover-over-eventual-pseudo-element granularity, inline `span`
//! sub-rectangle precision, `pointer-events`, transforms and stacking-context
//! 3D effects are out of scope for Phase 7.

use dom::{Document, NodeId};
use layout::Rect;

/// Coarse kind of painted primitive that produced a [`HitRegion`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HitKind {
    /// Element background (color and/or `background-image`) / canvas.
    Background,
    /// Element border box.
    Border,
    /// A text run.
    Text,
    /// A text decoration line (underline/overline/line-through).
    Decoration,
    /// A decoded replaced element (`<img>`).
    Image,
    /// A CSS list marker (bullet, number or `list-style-image`).
    Marker,
}

/// One painted rectangle tagged with the DOM node that generated it.
///
/// `rect` is already clipped to the active clip stack at painting time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HitRegion {
    /// Painted area in viewport CSS px (already clipped).
    pub rect: Rect,
    /// DOM node of the element that painted the primitive, when known.
    ///
    /// Formatting artifacts (line boxes, anonymous boxes) have no node and are
    /// not recorded. Text runs inherit the nearest element ancestor's id.
    pub node_id: Option<NodeId>,
    /// Coarse primitive kind.
    pub kind: HitKind,
}

/// A `<a href>` found under (or above) the hit point.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HitLink {
    /// The `<a>` element node.
    pub node_id: NodeId,
    /// Raw `href` attribute value (unresolved; the host resolves it for
    /// navigation, exactly like the document loader does).
    pub href: String,
}

/// Collector used by the painter when hit regions are requested.
///
/// It mirrors the painter's clip stack and records clipped regions in painting
/// order.
#[derive(Default)]
pub(crate) struct RegionCollector {
    regions: Vec<HitRegion>,
    clip_stack: Vec<Rect>,
}

impl RegionCollector {
    pub(crate) fn push_clip(&mut self, rect: Rect) {
        self.clip_stack.push(rect);
    }

    pub(crate) fn pop_clip(&mut self) {
        self.clip_stack.pop();
    }

    /// Intersection of the whole clip stack (viewport bounds are enforced by the
    /// region rectangles themselves, which the painter already keeps in view).
    fn current_clip(&self) -> Option<Rect> {
        if self.clip_stack.is_empty() {
            return None;
        }
        let mut clip = self.clip_stack[0];
        for rect in &self.clip_stack[1..] {
            let x0 = clip.x.max(rect.x);
            let y0 = clip.y.max(rect.y);
            let x1 = (clip.x + clip.width).min(rect.x + rect.width);
            let y1 = (clip.y + clip.height).min(rect.y + rect.height);
            if x0 >= x1 || y0 >= y1 {
                return Some(Rect::new(x0, y0, 0.0, 0.0));
            }
            clip = Rect::new(x0, y0, x1 - x0, y1 - y0);
        }
        Some(clip)
    }

    /// Records `rect` clipped by the current clip stack.
    pub(crate) fn record(&mut self, rect: Rect, node_id: Option<NodeId>, kind: HitKind) {
        if !rect.x.is_finite()
            || !rect.y.is_finite()
            || !rect.width.is_finite()
            || !rect.height.is_finite()
            || rect.width <= 0.0
            || rect.height <= 0.0
        {
            return;
        }
        let rect = match self.current_clip() {
            None => rect,
            Some(clip) => intersect(rect, clip),
        };
        if rect.width > 0.0 && rect.height > 0.0 {
            self.regions.push(HitRegion {
                rect,
                node_id,
                kind,
            });
        }
    }

    /// Moves the collected regions out (used by the builder after painting).
    pub(crate) fn take_regions(&mut self) -> Vec<HitRegion> {
        std::mem::take(&mut self.regions)
    }
}

/// Intersects two rectangles; returns a zero-sized rect when disjoint.
fn intersect(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.width).min(b.x + b.width);
    let y1 = (a.y + a.height).min(b.y + b.height);
    if x0 >= x1 || y0 >= y1 {
        Rect::new(0.0, 0.0, 0.0, 0.0)
    } else {
        Rect::new(x0, y0, x1 - x0, y1 - y0)
    }
}

/// True when `(x, y)` is inside `rect` (half-open right/bottom edges).
#[inline]
fn contains(rect: Rect, x: f32, y: f32) -> bool {
    x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
}

/// Returns the **topmost** DOM node under `(x, y)`.
///
/// Regions are stored in painting order, so the last containing region wins
/// (CSS painting order: later paints are on top). Regions without a DOM node
/// (formatting artifacts) are skipped; clipped-away content is not present in
/// the region list at all.
pub fn hit_test(regions: &[HitRegion], x: f32, y: f32) -> Option<NodeId> {
    regions
        .iter()
        .rev()
        .find(|region| region.node_id.is_some() && contains(region.rect, x, y))
        .and_then(|region| region.node_id)
}

/// Returns every region containing `(x, y)`, **top-most first**.
pub fn hit_test_all(regions: &[HitRegion], x: f32, y: f32) -> Vec<&HitRegion> {
    regions
        .iter()
        .rev()
        .filter(|region| contains(region.rect, x, y))
        .collect()
}

/// Finds the nearest HTML `<a href>` under `(x, y)`.
///
/// Starts from the node returned by [`hit_test`] and walks up its DOM ancestors
/// (the element itself included). Returns the raw `href` attribute string; the
/// host is responsible for resolving it against the document base URL.
pub fn find_link_at(doc: &Document, regions: &[HitRegion], x: f32, y: f32) -> Option<HitLink> {
    let mut node = hit_test(regions, x, y)?;
    loop {
        if let Some(element) = doc.node(node).as_element()
            && element.local_name.eq_ignore_ascii_case("a")
            && element.namespace == dom::Namespace::Html
            && let Some(href) = element
                .get_attribute("href")
                .map(str::trim)
                .filter(|href| !href.is_empty())
        {
            return Some(HitLink {
                node_id: node,
                href: href.to_string(),
            });
        }
        match doc.node(node).parent {
            Some(parent) if doc.node(parent).as_element().is_some() => node = parent,
            _ => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(rect: Rect, node_id: NodeId, kind: HitKind) -> HitRegion {
        HitRegion {
            rect,
            node_id: Some(node_id),
            kind,
        }
    }

    fn node(id: u32) -> NodeId {
        NodeId::from_index(id as usize)
    }

    #[test]
    fn test_hit_test_last_painted_wins_and_outside_is_none() {
        let regions = vec![
            region(
                Rect::new(0.0, 0.0, 10.0, 10.0),
                node(1),
                HitKind::Background,
            ),
            region(Rect::new(5.0, 5.0, 10.0, 10.0), node(2), HitKind::Text),
        ];
        assert_eq!(hit_test(&regions, 7.0, 7.0), Some(node(2)));
        assert_eq!(hit_test(&regions, 2.0, 2.0), Some(node(1)));
        assert_eq!(hit_test(&regions, 50.0, 50.0), None);
        // Half-open right/bottom edges: (10, 10) is outside the first region but
        // still inside the second one (5..15).
        assert_eq!(hit_test(&regions, 10.0, 10.0), Some(node(2)));
        assert_eq!(hit_test(&regions, 20.0, 10.0), None);
    }

    #[test]
    fn test_hit_test_all_returns_topmost_first() {
        let regions = vec![
            region(
                Rect::new(0.0, 0.0, 10.0, 10.0),
                node(1),
                HitKind::Background,
            ),
            region(Rect::new(5.0, 5.0, 10.0, 10.0), node(2), HitKind::Text),
        ];
        let hits = hit_test_all(&regions, 7.0, 7.0);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].node_id, Some(node(2)));
        assert_eq!(hits[1].node_id, Some(node(1)));
    }

    #[test]
    fn test_region_collector_clips_regions_at_emission() {
        let mut collector = RegionCollector::default();
        collector.record(
            Rect::new(0.0, 0.0, 10.0, 10.0),
            Some(node(1)),
            HitKind::Background,
        );
        collector.push_clip(Rect::new(5.0, 0.0, 5.0, 10.0));
        collector.record(
            Rect::new(0.0, 0.0, 10.0, 10.0),
            Some(node(2)),
            HitKind::Border,
        );
        collector.pop_clip();
        let regions = collector.take_regions();
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[1].rect, Rect::new(5.0, 0.0, 5.0, 10.0));
        assert_eq!(hit_test(&regions, 2.0, 2.0), Some(node(1)));
        assert_eq!(hit_test(&regions, 7.0, 2.0), Some(node(2)));
    }
}
