use super::*;

pub fn build_display_list(root: &LayoutBox, viewport: Rect) -> DisplayList {
    build_display_list_with_images(root, viewport, &ImageStore::new())
}
/// Builds a deterministic [`DisplayList`] from a laid-out [`LayoutBox`] tree,
/// painting decoded `<img>` replaced boxes from `images`.
///
/// A [`BoxType::Replaced`] box whose node is not present in the store is
/// painted as a gray placeholder box ([`IMAGE_PLACEHOLDER`]); image loading and
/// decoding are the caller's responsibility (see the `render` crate).
pub fn build_display_list_with_images(
    root: &LayoutBox,
    viewport: Rect,
    images: &ImageStore,
) -> DisplayList {
    build_display_list_with_images_and_backgrounds(root, viewport, images, &BackgroundImages::new())
}
/// Builds a deterministic [`DisplayList`] from a laid-out [`LayoutBox`] tree
/// with **replaced elements** (`<img>`, from `images`) and **CSS image
/// backgrounds / list markers** (`background-image` and `list-style-image`,
/// looked up by their raw URL string in `backgrounds`).
///
/// Painting rules (Phase 4/5 contract):
///
/// - `background-color` first, then `background-image` stretched over the
///   **padding box** (bilinear filtering, antialiased edges, clip stack).
///   `background-repeat`, `background-size`, `background-position` and
///   `background-clip` are **not** implemented; the image is always stretched
///   (documented limitation, WP-5.7 follow-up).
/// - If the URL of a `background-image` is absent from `backgrounds` nothing is
///   painted for it (no noisy placeholder; the color still paints). Canvas
///   propagation (`html`/`body` background) only propagates the *color*.
/// - A `list-item` with `list-style-image` present in `backgrounds` paints that
///   image (~1 em square, replacing `disc`/`decimal`); when the URL is missing
///   it falls back to the computed `list-style-type` marker.
///
/// `build_display_list_with_images` is equivalent with an empty background map.
pub fn build_display_list_with_images_and_backgrounds(
    root: &LayoutBox,
    viewport: Rect,
    images: &ImageStore,
    backgrounds: &BackgroundImages,
) -> DisplayList {
    build_display_list_inner(root, viewport, images, backgrounds, None).0
}
/// Variant of [`build_display_list_with_images_and_backgrounds`] that also
/// returns the [`HitRegion`] list used by the shell for clicks/hover.
///
/// The display list is byte-identical to the one produced without hit regions;
/// the regions are an additive side-channel in painting order.
pub fn build_display_list_with_hit_regions(
    root: &LayoutBox,
    viewport: Rect,
    images: &ImageStore,
    backgrounds: &BackgroundImages,
) -> (DisplayList, Vec<HitRegion>) {
    let collector = RefCell::new(RegionCollector::default());
    build_display_list_inner(root, viewport, images, backgrounds, Some(&collector))
}
/// Convenience hit-region variant without decoded images/backgrounds.
pub fn build_display_list_and_hit_regions(
    root: &LayoutBox,
    viewport: Rect,
) -> (DisplayList, Vec<HitRegion>) {
    let images = ImageStore::new();
    let backgrounds = BackgroundImages::new();
    let collector = RefCell::new(RegionCollector::default());
    build_display_list_inner(root, viewport, &images, &backgrounds, Some(&collector))
}
fn build_display_list_inner(
    root: &LayoutBox,
    viewport: Rect,
    images: &ImageStore,
    backgrounds: &BackgroundImages,
    regions: Option<&RefCell<RegionCollector>>,
) -> (DisplayList, Vec<HitRegion>) {
    let effects_present = tree_has_effects(root);
    let resources = PaintResources {
        images,
        backgrounds,
        regions,
        viewport,
        effects_present,
        // Fresh budget per pass: no global static to reset or share (#2296).
        text_mask_collectors: AtomicUsize::new(0),
    };
    let canvas = canvas_background(root, effects_present);
    // Reserve a modest starting capacity: the painter emits roughly one item per
    // text run plus one per painted box, and `DisplayItem` is a large enum, so a
    // few reallocations are expensive on text-heavy pages (R129).
    let mut list = DisplayList::with_capacity(20480);

    match canvas {
        CanvasBackground::None => {}
        CanvasBackground::Root(color) | CanvasBackground::Body { color, .. } => {
            list.push(DisplayItem::SolidColor {
                rect: viewport,
                color,
            });
        }
    }

    let skip = PaintSkip {
        root_background: matches!(canvas, CanvasBackground::Root(_)),
        body_node: match canvas {
            CanvasBackground::Body { node, .. } => Some(node),
            _ => None,
        },
    };
    if resources.regions.is_some() {
        // The canvas background is the bottom-most hit region: any click on the
        // page background reports the root element.
        resources.record(viewport, root.node_id(), HitKind::Background);
    }
    let (contexts, mut top_layer) = index_stacking_contexts(root, resources.effects_present);
    paint_stacking_context(root, &mut list, skip, &resources, false, true, 0, &contexts);
    top_layer.sort_by_key(|dialog_box| dialog_box.top_layer_order);
    for dialog_box in top_layer {
        if resources.regions.is_some() {
            // The ::backdrop intercepts pointer hit-testing over the entire viewport.
            resources.record(viewport, dialog_box.node_id(), HitKind::Background);
        }
        let backdrop_color = dialog_box
            .backdrop_color
            .unwrap_or_else(|| Color::new(0, 0, 0, 26));
        if backdrop_color.a > 0 {
            list.push(DisplayItem::SolidColor {
                rect: viewport,
                color: backdrop_color,
            });
        }
        paint_stacking_context(
            dialog_box, &mut list, skip, &resources, false, false, 0, &contexts,
        );
    }
    let regions = match regions {
        Some(collector) => collector.borrow_mut().take_regions(),
        None => Vec::new(),
    };
    (list, regions)
}
/// Returns the first block-level child of the root box, interpreted as `body`.
fn first_body_box(root: &LayoutBox, effects_present: bool) -> Option<&LayoutBox> {
    root.children.iter().find(|child| {
        !child.is_pseudo
            && matches!(child.box_type, BoxType::BlockNode(_))
            && phase(child, effects_present) == Phase::InFlow
    })
}
/// Computes the canvas background propagated from `html`/`body` (CSS 2.2 §14.2).
fn canvas_background(root: &LayoutBox, effects_present: bool) -> CanvasBackground {
    let root_background = root
        .style
        .as_ref()
        .map_or(Color::TRANSPARENT, |style| style.background_color);

    if root_background.a > 0 {
        return CanvasBackground::Root(root_background);
    }

    let Some(body) = first_body_box(root, effects_present) else {
        return CanvasBackground::None;
    };
    let body_background = body
        .style
        .as_ref()
        .map_or(Color::TRANSPARENT, |style| style.background_color);

    match body.node_id() {
        Some(node) if body_background.a > 0 => CanvasBackground::Body {
            color: body_background,
            node,
        },
        _ => CanvasBackground::None,
    }
}
