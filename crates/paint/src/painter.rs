//! # Display List Painter
//!
//! Traverses the CSS [`LayoutBox`] tree in CSS 2.2 Appendix E painting order
//! to produce a decoupled, flat [`DisplayList`].
//!
//! ## Painting order (WP-4.6)
//!
//! [`build_display_list`] resolves the complete Appendix E order into the flat
//! list (no group items are needed for ordering; the rasterizer consumes the
//! list sequentially). For every stacking context, in order:
//!
//! 1. canvas background (root context only, CSS 2.2 §14.2);
//! 2. background and borders of the element that establishes the context;
//! 3. descendants that establish a stacking context with negative `z-index`,
//!    sorted by `z-index` (stable, so ties keep tree order);
//! 4. background/borders of in-flow, non-positioned block-level descendants,
//!    in tree order;
//! 5. non-positioned floats (CSS 2.2 §9.5), in tree order;
//! 6. in-flow inline content: inline backgrounds/borders, text runs, text
//!    decorations and list markers, in tree order;
//! 7. positioned descendants with `z-index: auto/0` and stacking contexts with
//!    `z-index: 0` (including `opacity < 1` contexts), in tree order;
//! 8. stacking contexts with positive `z-index`, sorted by `z-index`.
//!
//! This is the Phase 4 in-flow case of Appendix E: line boxes are folded into
//! step 6 and floats are painted as a single block after all in-flow block
//! backgrounds (the classic approximation already documented by the audit;
//! step 5 ordering inside the float subtree follows the same phases).
//!
//! ## Opacity groups
//!
//! A box with `opacity < 1` creates a stacking context (css-color-3 §3.2). Its
//! painted items are wrapped into [`DisplayItem::OpacityGroup`] so the
//! rasterizer composites the subtree offscreen as one layer; compositing each
//! child individually would make overlaps darker than the specified opacity.
//!
//! ## Clipping (`overflow`)
//!
//! Layout propagates the absolute clip rectangle (`LayoutBox::clip`) to every
//! box that is affected by an ancestor clip. The painter emits
//! `PushClip`/`PopClip` around each clipped subtree (a clipping box's own
//! background/border are not clipped, per CSS 2.2 §11.1.1).
//!
//! When a box combines `overflow: hidden`/`clip` with `border-radius`, the
//! painter wraps its descendant content (block backgrounds, inline content and
//! floats painted from this context, plus nested stacking contexts) in a
//! [`DisplayItem::PushRoundedGroup`]/[`DisplayItem::PopRoundedGroup`] pair. The
//! clipping edge is the padding box and the corner radii are shrunk by the
//! border widths (CSS Overflow 3 §3), so text, images and children are clipped
//! by the inner rounded curve with antialiasing. The box's own background and
//! border were already rounded when painted and are not re-clipped. Floating
//! boxes whose clipping ancestor lies outside the current context are a
//! documented approximation (normal-flow content is clipped).
//!
//! ## `background-clip: text`
//!
//! When a background layer uses `background-clip: text` (or the
//! `-webkit-background-clip` alias), the painter collects the glyph runs of the
//! box's text descendants and wraps the background color/image items in
//! [`DisplayItem::PushTextMaskGroup`]/[`DisplayItem::PopTextMaskGroup`]. The
//! rasterizer renders that background offscreen and composites it through the
//! shaped glyph coverage, producing the usual "gradient text" effect when the
//! text color is transparent. At most [`crate::MAX_TEXT_MASK_RUNS`] runs are
//! collected and the offscreen area is capped (documented limits: no SVG text,
//! no `-webkit-text-stroke`).
//!
//! ## Text decorations and list markers (WP-4.9)
//!
//! `text-decoration` lines are painted from the computed style using the font
//! size (no glyph metrics are available in the painter): underline at
//! baseline + 0.12 em, overline at baseline - 0.8 em and line-through at
//! baseline - 0.3 em, with a `max(1px, font-size / 14)` thickness. `list-item`
//! boxes get a marker in front of the content box (fixed offset documented in
//! `paint_marker`) with `disc`/`circle`/`square`/`decimal` styles.
//!
//! ## Canvas and root/`body` background propagation
//!
//! CSS 2.2 §14.2 and CSS Backgrounds 3 §2.11.2: the canvas background comes
//! from the root element when it is not transparent, otherwise from the first
//! `body` child. The propagated color is emitted as the first display item over
//! the whole viewport and the originating element does not paint it again.

use css::{BorderStyle, Color, Display, ListStyleType, Position, TextDecoration, Visibility};
use dom::NodeId;
use layout::{BoxType, LayoutBox, Rect};

use std::cell::RefCell;

use crate::display_list::{
    BackgroundImages, BorderSide, DisplayItem, DisplayList, ImageStore, MAX_SHADOWS,
    MAX_TEXT_MASK_RUNS, MarkerKind, TextMaskRun,
};
use crate::hit_test::{HitKind, HitRegion, RegionCollector};

/// Background color propagated from the root element or `body` to the canvas.
#[derive(Clone, Copy, Debug, PartialEq)]
enum CanvasBackground {
    /// Nothing to propagate; the renderer default canvas color applies.
    None,
    /// `html` paints the canvas; the root box must skip its own background.
    Root(Color),
    /// `html` is transparent and `body` paints the canvas; `body` must skip its
    /// own background.
    Body { color: Color, node: NodeId },
}

/// Whether the root/`body` background painting should be skipped for an
/// already-propagated background.
#[derive(Clone, Copy, Debug, PartialEq)]
struct PaintSkip {
    root_background: bool,
    body_node: Option<NodeId>,
}

/// Decoded image resources available to one paint pass.
///
/// Bundled so the internal painting helpers thread a single reference: replaced
/// elements are keyed by [`dom::NodeId`] ([`ImageStore`]) while CSS background
/// and list-marker images are keyed by their raw CSS URL string
/// ([`BackgroundImages`]).
struct PaintResources<'a> {
    images: &'a ImageStore,
    backgrounds: &'a BackgroundImages,
    /// Optional hit-testing sink; `None` for the classic painting entry points,
    /// which must stay allocation- and output-identical.
    regions: Option<&'a RefCell<RegionCollector>>,
    /// Render viewport; used by `background-attachment: fixed` layers.
    viewport: Rect,
}

impl PaintResources<'_> {
    /// Records a painted primitive for hit-testing (no-op when regions are off).
    fn record(&self, rect: Rect, node_id: Option<dom::NodeId>, kind: HitKind) {
        if let Some(collector) = self.regions {
            collector.borrow_mut().record(rect, node_id, kind);
        }
    }

    fn push_clip(&self, rect: Rect) {
        if let Some(collector) = self.regions {
            collector.borrow_mut().push_clip(rect);
        }
    }

    fn pop_clip(&self) {
        if let Some(collector) = self.regions {
            collector.borrow_mut().pop_clip();
        }
    }
}

/// Emits a `PushClip` display item and mirrors it into the hit-region clip stack.
fn push_clip_item(list: &mut DisplayList, resources: &PaintResources<'_>, rect: Rect) {
    list.push(DisplayItem::PushClip { rect });
    resources.push_clip(rect);
}

/// Emits a `PopClip` display item and mirrors it into the hit-region clip stack.
fn pop_clip_item(list: &mut DisplayList, resources: &PaintResources<'_>) {
    list.push(DisplayItem::PopClip);
    resources.pop_clip();
}

/// Appendix E phase a box belongs to inside its nearest stacking context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// In-flow, non-positioned: steps 4 (blocks) and 6 (inline).
    InFlow,
    /// Non-positioned float: step 5.
    Float,
    /// Stacking context with a negative `z-index`: step 3.
    Negative,
    /// Positioned `z-index: auto/0` or `z-index: 0` context: step 7.
    Zero,
    /// Stacking context with a positive `z-index`: step 8.
    Positive,
}

/// Maximum nesting of stacking contexts the painter recurses into.
///
/// Deeper contexts are painted as flat in-flow content (their own visual box is
/// still painted) instead of recursing further. This bounds stack usage for
/// hostile documents. 256 levels is far beyond any real page (browsers cap
/// `z-index`/nesting behaviour long before that).
pub const MAX_STACKING_CONTEXT_DEPTH: usize = 256;

/// Maximum nesting of floats the painter follows recursively (see
/// [`MAX_STACKING_CONTEXT_DEPTH`]; floats beyond this depth are painted as
/// in-flow content).
pub const MAX_FLOAT_DEPTH: usize = 64;

/// Fill color used for a replaced element (`<img>`) whose image was not
/// decoded (missing file, unsupported format, network disabled/capped).
pub const IMAGE_PLACEHOLDER: Color = Color::new(0xD9, 0xD9, 0xD9, 255);

/// Builds a deterministic [`DisplayList`] from a laid-out [`LayoutBox`] tree,
/// without decoding replaced elements (they become gray placeholders).
///
/// This keeps the Phase 4 fixture pipeline unchanged; use
/// [`build_display_list_with_images`] to paint real images.
///
/// `viewport` is the canvas rectangle (normally `Rect::new(0.0, 0.0, w, h)`);
/// it is used for the propagated root/`body` canvas background.
///
/// # Robustness
///
/// All tree traversals use explicit stacks (iterative pre-order walks) instead
/// of native recursion, so deeply nested trees (tested with 20 000 levels) do
/// not overflow the process stack. Stacking-context nesting and float nesting
/// are additionally capped by [`MAX_STACKING_CONTEXT_DEPTH`] and
/// [`MAX_FLOAT_DEPTH`]; beyond the caps the subtree is painted as flat in-flow
/// content. The resulting list is bounded by
/// [`MAX_DISPLAY_LIST_ITEMS`](crate::MAX_DISPLAY_LIST_ITEMS).
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
    let resources = PaintResources {
        images,
        backgrounds,
        regions,
        viewport,
    };
    let canvas = canvas_background(root);
    let mut list = DisplayList::new();

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
    paint_stacking_context(root, &mut list, skip, &resources, false, true, 0);
    let regions = match regions {
        Some(collector) => collector.borrow_mut().take_regions(),
        None => Vec::new(),
    };
    (list, regions)
}

/// Returns the first block-level child of the root box, interpreted as `body`.
fn first_body_box(root: &LayoutBox) -> Option<&LayoutBox> {
    root.children.iter().find(|child| {
        matches!(child.box_type, BoxType::BlockNode(_)) && phase(child) == Phase::InFlow
    })
}

/// Computes the canvas background propagated from `html`/`body` (CSS 2.2 §14.2).
fn canvas_background(root: &LayoutBox) -> CanvasBackground {
    let root_background = root
        .style
        .as_ref()
        .map_or(Color::TRANSPARENT, |style| style.background_color);

    if root_background.a > 0 {
        return CanvasBackground::Root(root_background);
    }

    let Some(body) = first_body_box(root) else {
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

/// Builds the four computed border sides of a box (CSS order: top, right,
/// bottom, left), zeroing the width of `none`/`hidden` sides (CSS 2.2 §8.5.3).
fn border_sides(style: &css::ComputedStyle, border: layout::EdgeSizes) -> [BorderSide; 4] {
    let widths = [border.top, border.right, border.bottom, border.left];
    let styles = [
        style.border_style.top,
        style.border_style.right,
        style.border_style.bottom,
        style.border_style.left,
    ];
    let colors = [
        style.border_color.top,
        style.border_color.right,
        style.border_color.bottom,
        style.border_color.left,
    ];

    let mut sides = [BorderSide::NONE; 4];
    for (index, side) in sides.iter_mut().enumerate() {
        let width = if matches!(styles[index], BorderStyle::None | BorderStyle::Hidden) {
            0.0
        } else if widths[index].is_finite() {
            widths[index].max(0.0)
        } else {
            0.0
        };
        *side = BorderSide::new(width, styles[index], colors[index]);
    }
    sides
}

/// Appendix E phase of `layout_box` inside its nearest stacking context.
fn phase(layout_box: &LayoutBox) -> Phase {
    if layout_box.establishes_stacking_context {
        if layout_box.z_index < 0 {
            Phase::Negative
        } else if layout_box.z_index > 0 {
            Phase::Positive
        } else {
            Phase::Zero
        }
    } else if layout_box.position != Position::Static {
        // Positioned with `z-index: auto`: painted in step 8 as if it created a
        // stacking context for ordering purposes (CSS 2.2 Appendix E step 8).
        Phase::Zero
    } else if layout_box.is_float {
        Phase::Float
    } else {
        Phase::InFlow
    }
}

/// Applies `alpha_scale` to a color's alpha (flat-opacity fallback).
fn scaled(color: Color, alpha_scale: f32) -> Color {
    if alpha_scale >= 1.0 {
        return color;
    }
    let alpha = (f32::from(color.a) * alpha_scale.max(0.0)).round() as u8;
    Color::new(color.r, color.g, color.b, alpha)
}

/// True when the box's own style is hidden (CSS 2.2 §11.2 `visibility`).
fn is_hidden(layout_box: &LayoutBox) -> bool {
    layout_box
        .style
        .as_ref()
        .is_some_and(|style| matches!(style.visibility, Visibility::Hidden | Visibility::Collapse))
}

/// Rounded clip applied to a box's descendants when `overflow: hidden`/`clip`
/// and `border-radius` are authored.
///
/// The clipping edge is the padding box and the corner radii are shrunk by the
/// border widths (CSS Overflow 3 §3 / CSS Backgrounds 3 §5.5), so content is
/// clipped by the inner curve, not by the outer border box.
fn overflow_rounded_clip(layout_box: &LayoutBox) -> Option<(Rect, css::BorderRadii)> {
    let style = layout_box.style.as_ref()?;
    if !matches!(style.overflow, css::Overflow::Hidden | css::Overflow::Clip)
        || style.border_radius.is_zero()
    {
        return None;
    }
    let padding_box = layout_box.dimensions.padding_box();
    if padding_box.width <= 0.0 || padding_box.height <= 0.0 {
        return None;
    }
    let border = layout_box.dimensions.border;
    let shrink = |length: css::Length, width: f32| -> css::Length {
        match length {
            css::Length::Px(px) => css::Length::Px((px - width).max(0.0)),
            other => other,
        }
    };
    let radii = css::BorderRadii {
        top_left: css::Radius {
            x: shrink(style.border_radius.top_left.x, border.left),
            y: shrink(style.border_radius.top_left.y, border.top),
        },
        top_right: css::Radius {
            x: shrink(style.border_radius.top_right.x, border.right),
            y: shrink(style.border_radius.top_right.y, border.top),
        },
        bottom_right: css::Radius {
            x: shrink(style.border_radius.bottom_right.x, border.right),
            y: shrink(style.border_radius.bottom_right.y, border.bottom),
        },
        bottom_left: css::Radius {
            x: shrink(style.border_radius.bottom_left.x, border.left),
            y: shrink(style.border_radius.bottom_left.y, border.bottom),
        },
    };
    Some((padding_box, radii))
}

/// True when a background layer asks to be clipped to the element's glyphs.
fn background_clips_to_text(style: &css::ComputedStyle) -> bool {
    style
        .background_layers
        .iter()
        .any(|layer| layer.clip == css::BackgroundClip::Text)
}

/// Collects the glyph runs of a box's subtree for a `background-clip: text` mask.
///
/// Visibility-hidden boxes and empty text nodes are skipped, matching the text
/// items actually painted. Runs are capped at [`MAX_TEXT_MASK_RUNS`]; beyond the
/// cap the remaining text simply does not contribute to the mask (documented).
fn collect_text_mask_runs(layout_box: &LayoutBox) -> Vec<TextMaskRun> {
    let mut runs = Vec::new();
    let mut stack: Vec<(&LayoutBox, bool)> = Vec::new();
    for child in layout_box.children.iter().rev() {
        stack.push((child, is_hidden(child)));
    }
    while let Some((child, hidden)) = stack.pop() {
        if hidden {
            continue;
        }
        if let BoxType::TextNode(text) = &child.box_type
            && !text.trim().is_empty()
        {
            let font_size = child.style.as_ref().map_or(16.0, |style| style.font_size);
            let char_count = text.chars().count();
            let advance = if char_count > 0 && child.dimensions.content.width > 0.0 {
                child.dimensions.content.width / char_count as f32
            } else {
                0.6 * font_size
            };
            runs.push(TextMaskRun {
                text: text.clone(),
                rect: child.dimensions.content,
                font_size,
                font_family: child.style.as_ref().map_or_else(
                    || "sans-serif".to_string(),
                    |style| style.font_family.clone(),
                ),
                font_weight: child.style.as_ref().map_or(400, |style| style.font_weight),
                font_italic: child.style.as_ref().is_some_and(|style| {
                    matches!(
                        style.font_style,
                        css::FontStyle::Italic | css::FontStyle::Oblique
                    )
                }),
                advance,
                baseline: child.baseline,
            });
            if runs.len() >= MAX_TEXT_MASK_RUNS {
                return runs;
            }
        }
        for grandchild in child.children.iter().rev() {
            stack.push((grandchild, hidden || is_hidden(grandchild)));
        }
    }
    runs
}

/// Paints the appendix E phases of one stacking context.
fn paint_stacking_context(
    layout_box: &LayoutBox,
    list: &mut DisplayList,
    skip: PaintSkip,
    resources: &PaintResources<'_>,
    inherited_hidden: bool,
    is_root: bool,
    depth: usize,
) {
    let start = list.len();
    let hidden = inherited_hidden || is_hidden(layout_box);
    let opacity = layout_box.opacity.clamp(0.0, 1.0);

    // Step 2: background and borders of the element establishing the context.
    if !hidden {
        paint_background_border(layout_box, list, skip, resources, is_root, 1.0);
    }

    // `overflow: hidden`/`clip` + radius clips the descendant subtree of this
    // stacking context (all phases, including nested contexts and z-index).
    let overflow_clip = if hidden {
        None
    } else {
        overflow_rounded_clip(layout_box)
    };
    if let Some((rect, radii)) = overflow_clip {
        list.push(DisplayItem::PushRoundedGroup { rect, radii });
    }

    let clip = layout_box.clip;
    if let Some(rect) = clip {
        push_clip_item(list, resources, rect);
    }

    if depth >= MAX_STACKING_CONTEXT_DEPTH {
        // Depth guard: paint the remaining subtree as flat in-flow content
        // instead of recursing into further stacking contexts.
        paint_block_backgrounds(layout_box, list, skip, resources, hidden, 1.0);
        paint_floats(layout_box, list, skip, resources, hidden, 1.0, depth);
        paint_inline_content(
            layout_box,
            list,
            skip,
            resources,
            hidden,
            TextDecoration::NONE,
            1.0,
        );
        if clip.is_some() {
            pop_clip_item(list, resources);
        }
        if overflow_clip.is_some() {
            list.push(DisplayItem::PopRoundedGroup);
        }
        return;
    }

    // Split the participating descendant contexts into phase buckets.
    let mut negative: Vec<&LayoutBox> = Vec::new();
    let mut zero: Vec<&LayoutBox> = Vec::new();
    let mut positive: Vec<&LayoutBox> = Vec::new();
    collect_context_boxes(layout_box, &mut negative, &mut zero, &mut positive);
    negative.sort_by_key(|context| context.z_index);
    positive.sort_by_key(|context| context.z_index);

    // Step 3: negative z-index contexts.
    for context in negative {
        paint_stacking_context(context, list, skip, resources, hidden, false, depth + 1);
    }

    // Steps 4-6: in-flow block backgrounds, floats, inline content.
    paint_block_backgrounds(layout_box, list, skip, resources, hidden, 1.0);
    paint_floats(layout_box, list, skip, resources, hidden, 1.0, depth);
    paint_inline_content(
        layout_box,
        list,
        skip,
        resources,
        hidden,
        TextDecoration::NONE,
        1.0,
    );

    // Step 7: positioned z-index auto/0 and z-index 0 contexts.
    for context in zero {
        paint_stacking_context(context, list, skip, resources, hidden, false, depth + 1);
    }

    // Step 8: positive z-index contexts.
    for context in positive {
        paint_stacking_context(context, list, skip, resources, hidden, false, depth + 1);
    }

    if clip.is_some() {
        pop_clip_item(list, resources);
    }
    if overflow_clip.is_some() {
        list.push(DisplayItem::PopRoundedGroup);
    }

    // Opacity group: the whole subtree is composited once with `opacity`.
    if !is_root && opacity < 1.0 && list.len() > start {
        let items: Vec<DisplayItem> = list.items.drain(start..).collect();
        let rect = items_bounds(&items).unwrap_or_else(|| layout_box.dimensions.border_box());
        list.push(DisplayItem::OpacityGroup {
            rect,
            opacity,
            items,
        });
    }
}

/// Collects the stacking contexts that participate in the context rooted at
/// `layout_box` (nearest-context descendants, in tree order).
///
/// Iterative by design: a 20 000-level layout tree must not exhaust the stack.
fn collect_context_boxes<'a>(
    layout_box: &'a LayoutBox,
    negative: &mut Vec<&'a LayoutBox>,
    zero: &mut Vec<&'a LayoutBox>,
    positive: &mut Vec<&'a LayoutBox>,
) {
    let mut stack: Vec<&'a LayoutBox> = Vec::new();
    for child in layout_box.children.iter().rev() {
        stack.push(child);
    }
    while let Some(child) = stack.pop() {
        match phase(child) {
            Phase::Negative => negative.push(child),
            Phase::Zero => zero.push(child),
            Phase::Positive => positive.push(child),
            Phase::Float | Phase::InFlow => {
                for grandchild in child.children.iter().rev() {
                    stack.push(grandchild);
                }
            }
        }
    }
}

/// Explicit-stack task for the iterative block-background traversal.
enum BlockTask<'a> {
    Visit {
        layout_box: &'a LayoutBox,
        inherited_hidden: bool,
        alpha_scale: f32,
    },
    PopClip,
    PopRoundedGroup,
}

/// Paints background and borders of in-flow block-level descendants (step 4).
///
/// Iterative: uses an explicit stack instead of recursion so deeply nested
/// documents cannot overflow the native stack.
fn paint_block_backgrounds(
    layout_box: &LayoutBox,
    list: &mut DisplayList,
    skip: PaintSkip,
    resources: &PaintResources<'_>,
    inherited_hidden: bool,
    alpha_scale: f32,
) {
    let mut stack: Vec<BlockTask<'_>> = Vec::new();
    push_block_children(layout_box, inherited_hidden, alpha_scale, &mut stack);
    while let Some(task) = stack.pop() {
        match task {
            BlockTask::PopClip => pop_clip_item(list, resources),
            BlockTask::PopRoundedGroup => list.push(DisplayItem::PopRoundedGroup),
            BlockTask::Visit {
                layout_box: child,
                inherited_hidden,
                alpha_scale,
            } => {
                let hidden = inherited_hidden || is_hidden(child);
                let alpha = alpha_scale * child.opacity.clamp(0.0, 1.0);
                if let Some(rect) = child.clip {
                    push_clip_item(list, resources, rect);
                    stack.push(BlockTask::PopClip);
                }
                if child.is_block() && !hidden {
                    paint_background_border(child, list, skip, resources, false, alpha);
                }
                // `overflow: hidden` + radius clips the descendant subtree.
                if !hidden && let Some((rect, radii)) = overflow_rounded_clip(child) {
                    list.push(DisplayItem::PushRoundedGroup { rect, radii });
                    stack.push(BlockTask::PopRoundedGroup);
                }
                push_block_children(child, hidden, alpha, &mut stack);
            }
        }
    }
}

fn push_block_children<'a>(
    parent: &'a LayoutBox,
    inherited_hidden: bool,
    alpha_scale: f32,
    stack: &mut Vec<BlockTask<'a>>,
) {
    for child in parent.children.iter().rev() {
        if phase(child) != Phase::InFlow {
            continue;
        }
        stack.push(BlockTask::Visit {
            layout_box: child,
            inherited_hidden,
            alpha_scale,
        });
    }
}

/// Paints non-positioned floats (step 5), in tree order.
///
/// Iterative; nested floats recurse only up to [`MAX_FLOAT_DEPTH`].
fn paint_floats(
    layout_box: &LayoutBox,
    list: &mut DisplayList,
    skip: PaintSkip,
    resources: &PaintResources<'_>,
    inherited_hidden: bool,
    alpha_scale: f32,
    depth: usize,
) {
    let mut stack: Vec<&LayoutBox> = Vec::new();
    for child in layout_box.children.iter().rev() {
        stack.push(child);
    }
    while let Some(child) = stack.pop() {
        match phase(child) {
            Phase::Float => paint_float(
                child,
                list,
                skip,
                resources,
                inherited_hidden,
                alpha_scale,
                depth,
            ),
            Phase::InFlow => {
                for grandchild in child.children.iter().rev() {
                    stack.push(grandchild);
                }
            }
            Phase::Negative | Phase::Zero | Phase::Positive => {}
        }
    }
}

/// Paints one float: its background/border and its in-flow content (a float
/// behaves like a stacking context for painting purposes).
fn paint_float(
    layout_box: &LayoutBox,
    list: &mut DisplayList,
    skip: PaintSkip,
    resources: &PaintResources<'_>,
    inherited_hidden: bool,
    alpha_scale: f32,
    depth: usize,
) {
    let hidden = inherited_hidden || is_hidden(layout_box);
    let alpha = alpha_scale * layout_box.opacity.clamp(0.0, 1.0);
    let clipped = layout_box.clip;
    if let Some(rect) = clipped {
        push_clip_item(list, resources, rect);
    }
    if !hidden {
        paint_background_border(layout_box, list, skip, resources, false, alpha);
    }
    let overflow_clip = if hidden {
        None
    } else {
        overflow_rounded_clip(layout_box)
    };
    if let Some((rect, radii)) = overflow_clip {
        list.push(DisplayItem::PushRoundedGroup { rect, radii });
    }
    paint_block_backgrounds(layout_box, list, skip, resources, hidden, alpha);
    if depth < MAX_FLOAT_DEPTH {
        paint_floats(layout_box, list, skip, resources, hidden, alpha, depth + 1);
    }
    paint_inline_content(
        layout_box,
        list,
        skip,
        resources,
        hidden,
        TextDecoration::NONE,
        alpha,
    );
    if overflow_clip.is_some() {
        list.push(DisplayItem::PopRoundedGroup);
    }
    if clipped.is_some() {
        pop_clip_item(list, resources);
    }
}

/// Explicit-stack task for the iterative inline-content traversal.
enum InlineTask<'a> {
    Visit {
        layout_box: &'a LayoutBox,
        inherited_hidden: bool,
        inherited_decoration: TextDecoration,
        alpha: f32,
        ordinal: u32,
        /// Nearest element `NodeId` (for text/decoration hit regions).
        owner: Option<dom::NodeId>,
    },
    PopClip,
    PopRoundedGroup,
}

/// Paints in-flow inline content (step 6): inline boxes, text runs with their
/// decorations and list markers, in tree order.
///
/// Iterative: explicit stack, so deep inline nesting cannot overflow the stack.
fn paint_inline_content(
    layout_box: &LayoutBox,
    list: &mut DisplayList,
    skip: PaintSkip,
    resources: &PaintResources<'_>,
    inherited_hidden: bool,
    inherited_decoration: TextDecoration,
    alpha_scale: f32,
) {
    let owner = layout_box.node_id();
    let mut stack: Vec<InlineTask<'_>> = Vec::new();
    push_inline_children(
        layout_box,
        inherited_hidden,
        inherited_decoration,
        alpha_scale,
        owner,
        &mut stack,
    );
    while let Some(task) = stack.pop() {
        match task {
            InlineTask::PopClip => pop_clip_item(list, resources),
            InlineTask::PopRoundedGroup => list.push(DisplayItem::PopRoundedGroup),
            InlineTask::Visit {
                layout_box: child,
                inherited_hidden,
                inherited_decoration,
                alpha,
                ordinal,
                owner,
            } => {
                let own_owner = child.node_id().or(owner);
                let parent_decoration = inherited_decoration
                    | child
                        .style
                        .as_ref()
                        .map_or(TextDecoration::NONE, |style| style.text_decoration);
                let hidden = inherited_hidden || is_hidden(child);
                let clipped = child.clip;
                if let Some(rect) = clipped {
                    push_clip_item(list, resources, rect);
                    stack.push(InlineTask::PopClip);
                }

                match &child.box_type {
                    BoxType::TextNode(text) if !text.trim().is_empty() => {
                        if !hidden {
                            paint_text_run(child, text, list, resources, own_owner, alpha);
                            paint_text_decorations(
                                child,
                                parent_decoration
                                    | child.style.as_ref().map_or(TextDecoration::NONE, |style| {
                                        style.text_decoration
                                    }),
                                list,
                                resources,
                                own_owner,
                                alpha,
                            );
                        }
                    }
                    BoxType::TextNode(_) | BoxType::LineBreak => {}
                    BoxType::Replaced(node_id) => {
                        if !hidden {
                            paint_background_border(child, list, skip, resources, false, alpha);
                            let replaced_radius = child
                                .style
                                .as_ref()
                                .map(|style| style.border_radius)
                                .filter(|radii| !radii.is_zero());
                            if let Some(radii) = replaced_radius {
                                // Replaced content is clipped by `border-radius`
                                // (CSS Backgrounds 3 §5.5).
                                list.push(DisplayItem::PushRoundedGroup {
                                    rect: child.dimensions.content,
                                    radii,
                                });
                                paint_replaced_element(child, *node_id, list, resources, alpha);
                                list.push(DisplayItem::PopRoundedGroup);
                            } else {
                                paint_replaced_element(child, *node_id, list, resources, alpha);
                            }
                        }
                    }
                    BoxType::InlineNode(_) => {
                        if !hidden {
                            paint_background_border(child, list, skip, resources, false, alpha);
                        }
                        if !hidden && let Some((rect, radii)) = overflow_rounded_clip(child) {
                            list.push(DisplayItem::PushRoundedGroup { rect, radii });
                            stack.push(InlineTask::PopRoundedGroup);
                        }
                        push_inline_children(
                            child,
                            hidden,
                            parent_decoration,
                            alpha,
                            own_owner,
                            &mut stack,
                        );
                    }
                    BoxType::BlockNode(_) | BoxType::AnonymousBlock | BoxType::LineBox => {
                        if is_list_item(child) && !hidden {
                            paint_marker(child, ordinal, list, resources, alpha);
                        }
                        if !hidden && let Some((rect, radii)) = overflow_rounded_clip(child) {
                            list.push(DisplayItem::PushRoundedGroup { rect, radii });
                            stack.push(InlineTask::PopRoundedGroup);
                        }
                        push_inline_children(
                            child,
                            hidden,
                            parent_decoration,
                            alpha,
                            own_owner,
                            &mut stack,
                        );
                    }
                    // Table internals and inline-block wrappers: backgrounds and
                    // borders are emitted by the block-background pass, but their
                    // inline content (line boxes, text runs, decorations) must be
                    // visited here as well or table cells' text never paints
                    // (reported by the layout lane, task-53).
                    BoxType::InlineBlock(_)
                    | BoxType::Table(_)
                    | BoxType::TableRowGroup(_)
                    | BoxType::TableRow(_)
                    | BoxType::TableCell(_)
                    | BoxType::TableCaption(_)
                    | BoxType::TableColumn(_)
                    | BoxType::AnonymousTableRow
                    | BoxType::AnonymousTableCell => {
                        if !hidden && let Some((rect, radii)) = overflow_rounded_clip(child) {
                            list.push(DisplayItem::PushRoundedGroup { rect, radii });
                            stack.push(InlineTask::PopRoundedGroup);
                        }
                        push_inline_children(
                            child,
                            hidden,
                            parent_decoration,
                            alpha,
                            own_owner,
                            &mut stack,
                        );
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Pushes the inline tasks of `parent`'s children (reversed, so the stack pops
/// them in tree order), computing per-box hidden/decoration/alpha state and
/// `list-item` ordinals.
///
/// Allocation-free: a single backwards pass derives the ordinals directly
/// instead of collecting intermediate task vectors (this hot path runs for
/// every box of the layout tree).
fn push_inline_children<'a>(
    parent: &'a LayoutBox,
    inherited_hidden: bool,
    inherited_decoration: TextDecoration,
    alpha_scale: f32,
    owner: Option<dom::NodeId>,
    stack: &mut Vec<InlineTask<'a>>,
) {
    let parent_decoration = inherited_decoration
        | parent
            .style
            .as_ref()
            .map_or(TextDecoration::NONE, |style| style.text_decoration);
    // 1-based ordinal of the last list item in this parent, decremented while
    // walking the children backwards.
    let mut next_ordinal = parent
        .children
        .iter()
        .filter(|child| {
            !matches!(
                phase(child),
                Phase::Float | Phase::Negative | Phase::Zero | Phase::Positive
            ) && is_list_item(child)
        })
        .count() as u32;

    for child in parent.children.iter().rev() {
        if matches!(
            phase(child),
            Phase::Float | Phase::Negative | Phase::Zero | Phase::Positive
        ) {
            continue;
        }
        let ordinal = if is_list_item(child) {
            let ordinal = next_ordinal.max(1);
            next_ordinal = next_ordinal.saturating_sub(1);
            ordinal
        } else {
            0
        };
        let hidden = inherited_hidden || is_hidden(child);
        let alpha = alpha_scale * child.opacity.clamp(0.0, 1.0);
        stack.push(InlineTask::Visit {
            layout_box: child,
            inherited_hidden: hidden,
            inherited_decoration: parent_decoration,
            alpha,
            ordinal,
            owner: child.node_id().or(owner),
        });
    }
}

/// Emits the paint of a replaced element: its decoded image if available,
/// otherwise a documented gray placeholder box (CSS 2.2 §10.6.2 intrinsic size
/// is handled by layout; paint only stretches the bitmap to `content`).
fn paint_replaced_element(
    layout_box: &LayoutBox,
    node_id: dom::NodeId,
    list: &mut DisplayList,
    resources: &PaintResources<'_>,
    alpha_scale: f32,
) {
    let rect = layout_box.dimensions.content;
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    resources.record(rect, Some(node_id), HitKind::Image);
    match resources.images.get(&node_id) {
        Some(image) if !image.is_empty() => {
            list.push(DisplayItem::Image {
                rect,
                image: std::sync::Arc::clone(image),
            });
            // A translucent element (flat-opacity fallback) darkens the image
            // through a normal-alpha fill when needed.
            if alpha_scale < 1.0 {
                list.push(DisplayItem::SolidColor {
                    rect,
                    color: Color::new(0, 0, 0, 0),
                });
            }
        }
        _ => {
            list.push(DisplayItem::SolidColor {
                rect,
                color: scaled(IMAGE_PLACEHOLDER, alpha_scale),
            });
        }
    }
}

/// Emits the background and borders of an element box.
fn paint_background_border(
    layout_box: &LayoutBox,
    list: &mut DisplayList,
    skip: PaintSkip,
    resources: &PaintResources<'_>,
    is_root: bool,
    alpha_scale: f32,
) {
    let is_element = layout_box.node_id().is_some();
    if !is_element {
        return;
    }
    let Some(style) = layout_box.style.as_ref() else {
        return;
    };

    let node_id = layout_box.node_id();
    resources.record(
        layout_box.dimensions.border_box(),
        node_id,
        HitKind::Background,
    );

    let background_is_propagated = (is_root && skip.root_background)
        || layout_box.node_id().is_some() && layout_box.node_id() == skip.body_node;
    let border_box = layout_box.dimensions.border_box();
    let radii = style.border_radius;
    let has_radius = !radii.is_zero();
    let rounded_group = !background_is_propagated && has_radius;
    // `background-clip: text`: the background color/images are composited through
    // the glyph mask of the box's text descendants.
    let text_mask_runs = if !background_is_propagated && background_clips_to_text(style) {
        Some(collect_text_mask_runs(layout_box))
    } else {
        None
    };
    if let Some(runs) = &text_mask_runs {
        list.push(DisplayItem::PushTextMaskGroup {
            rect: border_box,
            runs: runs.clone(),
        });
    }

    // Outer `box-shadow` layers are painted behind the background, last shadow
    // first so the first authored shadow ends up on top (CSS Backgrounds 3 §6).
    if !background_is_propagated {
        for shadow in style
            .box_shadows
            .iter()
            .rev()
            .filter(|shadow| !shadow.inset)
            .take(MAX_SHADOWS)
        {
            list.push(DisplayItem::BoxShadow {
                rect: border_box,
                radii,
                shadow: css::BoxShadow {
                    color: scaled(shadow.color, alpha_scale),
                    ..*shadow
                },
            });
        }
    }
    if rounded_group {
        list.push(DisplayItem::PushRoundedGroup {
            rect: border_box,
            radii,
        });
    }
    if !background_is_propagated && style.background_color.a > 0 {
        if has_radius {
            list.push(DisplayItem::RoundedRect {
                rect: border_box,
                radii,
                color: scaled(style.background_color, alpha_scale),
            });
        } else {
            list.push(DisplayItem::SolidColor {
                rect: border_box,
                color: scaled(style.background_color, alpha_scale),
            });
        }
    }

    // `background-image`, on top of the color; missing URL => nothing painted.
    if !background_is_propagated {
        let border_box = layout_box.dimensions.border_box();
        let padding_box = layout_box.dimensions.padding_box();
        let content_box = layout_box.dimensions.content;
        if style.background_geometry_explicit {
            // Full CSS Backgrounds 3 geometry for every layer, painted
            // bottom-to-top (CSS lists the top layer first).
            let fallback_layer;
            let layers: &[css::BackgroundLayer] = if style.background_layers.is_empty() {
                // Synthetic styles/tests may only populate the legacy scalar
                // fields; treat them as a single layer with CSS initial geometry.
                fallback_layer = vec![css::BackgroundLayer {
                    image: style.background_image.clone(),
                    repeat: style.background_repeat,
                    size: style.background_size,
                    position: style.background_position,
                    ..css::BackgroundLayer::default()
                }];
                &fallback_layer
            } else {
                &style.background_layers
            };
            for layer in layers.iter().rev() {
                let Some(url) = layer.image.as_deref() else {
                    continue;
                };
                let Some(image) = resources.backgrounds.get(url) else {
                    continue;
                };
                if image.is_empty() {
                    continue;
                }
                let positioning_area =
                    if crate::background::uses_viewport_positioning(layer.attachment) {
                        resources.viewport
                    } else {
                        crate::background::background_origin_rect(
                            layer.origin,
                            border_box,
                            padding_box,
                            content_box,
                        )
                    };
                let clip_rect = crate::background::background_clip_rect(
                    layer.clip,
                    border_box,
                    padding_box,
                    content_box,
                );
                let tiles = crate::background::background_tiles_with_origin(
                    positioning_area,
                    clip_rect,
                    image.dimensions(),
                    layer.repeat,
                    layer.size,
                    layer.position,
                );
                if tiles.is_empty() {
                    continue;
                }
                push_clip_item(list, resources, clip_rect);
                for tile in tiles {
                    list.push(DisplayItem::Image {
                        rect: tile,
                        image: std::sync::Arc::clone(image),
                    });
                }
                pop_clip_item(list, resources);
            }
        } else if let Some(url) = style.background_image.as_deref()
            && let Some(image) = resources.backgrounds.get(url)
            && !image.is_empty()
        {
            // Legacy compatibility: documents that only set `background-image`
            // (all Phase 4/5/6 golden fixtures) keep the historical behaviour of
            // stretching the bitmap over the whole padding box. Authored
            // background properties opt into CSS geometry above.
            list.push(DisplayItem::Image {
                rect: padding_box,
                image: std::sync::Arc::clone(image),
            });
        }
    }

    if rounded_group {
        list.push(DisplayItem::PopRoundedGroup);
    }
    if text_mask_runs.is_some() {
        list.push(DisplayItem::PopTextMaskGroup);
    }

    // Inset shadows are painted above the background but below the border and content.
    if !background_is_propagated {
        for shadow in style
            .box_shadows
            .iter()
            .rev()
            .filter(|shadow| shadow.inset)
            .take(MAX_SHADOWS)
        {
            list.push(DisplayItem::BoxShadow {
                rect: border_box,
                radii,
                shadow: css::BoxShadow {
                    color: scaled(shadow.color, alpha_scale),
                    ..*shadow
                },
            });
        }
    }

    let sides = border_sides(style, layout_box.dimensions.border);
    if sides.iter().any(BorderSide::is_visible) {
        resources.record(border_box, node_id, HitKind::Border);
        let sides = sides.map(|side| BorderSide {
            color: scaled(side.color, alpha_scale),
            ..side
        });
        let solid = sides
            .iter()
            .filter(|side| side.is_visible())
            .all(|side| matches!(side.style, BorderStyle::Solid));
        if has_radius && solid {
            list.push(DisplayItem::RoundedBorder {
                rect: border_box,
                sides,
                radii,
            });
        } else {
            list.push(DisplayItem::Border {
                rect: border_box,
                sides,
            });
        }
    }
}

/// Emits the `text-shadow` layers for one text run (CSS Text Decoration 3 §3).
///
/// Shadows are painted last-first so the first authored shadow ends up on top,
/// immediately before the glyphs. Blur is clamped by the rasterizer and the list
/// is capped at [`MAX_SHADOWS`].
#[allow(clippy::too_many_arguments)]
fn push_text_shadows(
    style: Option<&css::ComputedStyle>,
    list: &mut DisplayList,
    text: &str,
    rect: Rect,
    font_size: f32,
    font_family: &str,
    font_weight: u16,
    font_italic: bool,
    advance: f32,
    baseline: f32,
    alpha_scale: f32,
) {
    let Some(style) = style else {
        return;
    };
    for shadow in style.text_shadows.iter().rev().take(MAX_SHADOWS) {
        list.push(DisplayItem::TextShadow {
            text: text.to_string(),
            rect,
            color: scaled(shadow.color, alpha_scale),
            font_size,
            font_family: font_family.to_string(),
            font_weight,
            font_italic,
            advance,
            baseline,
            offset_x: shadow.offset_x,
            offset_y: shadow.offset_y,
            blur: shadow.blur,
        });
    }
}

/// Emits one shaped text run.
fn paint_text_run(
    layout_box: &LayoutBox,
    text: &str,
    list: &mut DisplayList,
    resources: &PaintResources<'_>,
    owner: Option<dom::NodeId>,
    alpha_scale: f32,
) {
    let style = layout_box.style.as_ref();
    let (color, font_size, font_family) = if let Some(style) = style {
        (style.color, style.font_size, style.font_family.clone())
    } else {
        (Color::BLACK, 16.0, "serif".to_string())
    };

    let char_count = text.chars().count();
    let measured_width = layout_box.dimensions.content.width;
    let advance = if char_count > 0 && measured_width > 0.0 {
        measured_width / char_count as f32
    } else {
        0.6 * font_size
    };
    let font_weight = style.map_or(400, |style| style.font_weight);
    let font_italic = style.is_some_and(|style| {
        matches!(
            style.font_style,
            css::FontStyle::Italic | css::FontStyle::Oblique
        )
    });

    resources.record(layout_box.dimensions.content, owner, HitKind::Text);
    push_text_shadows(
        style.map(|style| style.as_ref()),
        list,
        text,
        layout_box.dimensions.content,
        font_size,
        &font_family,
        font_weight,
        font_italic,
        advance,
        layout_box.baseline,
        alpha_scale,
    );
    list.push(DisplayItem::Text {
        text: text.to_string(),
        rect: layout_box.dimensions.content,
        color: scaled(color, alpha_scale),
        font_size,
        font_family,
        font_weight,
        font_italic,
        advance,
        baseline: layout_box.baseline,
    });
}

/// Emits the decoration lines for one text run (CSS 2.2 §16.3.1).
fn paint_text_decorations(
    layout_box: &LayoutBox,
    decoration: TextDecoration,
    list: &mut DisplayList,
    resources: &PaintResources<'_>,
    owner: Option<dom::NodeId>,
    alpha_scale: f32,
) {
    if decoration.is_empty() {
        return;
    }
    let Some(style) = layout_box.style.as_ref() else {
        return;
    };
    let rect = layout_box.dimensions.content;
    if rect.width <= 0.0 {
        return;
    }
    let font_size = style.font_size;
    let color = scaled(style.color, alpha_scale);
    let baseline = rect.y + layout_box.baseline;
    let thickness = (font_size / 14.0).max(1.0);
    let width = rect.width;

    let mut record_line = |y: f32| {
        let line = Rect::new(rect.x, y - thickness / 2.0, width, thickness);
        resources.record(line, owner, HitKind::Decoration);
        list.push(DisplayItem::TextDecoration { rect: line, color });
    };
    if decoration.contains(TextDecoration::UNDERLINE) {
        record_line(baseline + font_size * 0.12);
    }
    if decoration.contains(TextDecoration::OVERLINE) {
        record_line(baseline - font_size * 0.8);
    }
    if decoration.contains(TextDecoration::LINE_THROUGH) {
        record_line(baseline - font_size * 0.3);
    }
}

/// True when the box is a `list-item` (paints a marker).
fn is_list_item(layout_box: &LayoutBox) -> bool {
    layout_box
        .style
        .as_ref()
        .is_some_and(|style| style.display == Display::ListItem)
}

/// Returns the absolute baseline of the first non-empty text fragment inside
/// `layout_box`, used to align list markers with the first line.
///
/// Iterative pre-order walk (no recursion: deep list-item chains are safe).
fn first_text_baseline(layout_box: &LayoutBox) -> Option<f32> {
    let mut stack: Vec<&LayoutBox> = vec![layout_box];
    while let Some(current) = stack.pop() {
        if let BoxType::TextNode(text) = &current.box_type
            && !text.trim().is_empty()
        {
            return Some(current.dimensions.content.y + current.baseline);
        }
        for child in current.children.iter().rev() {
            stack.push(child);
        }
    }
    None
}

/// Paints one list marker in front of the content box (CSS 2.2 §12.5).
///
/// Layout does not expose the marker gap yet, so the marker's right edge is a
/// fixed `0.5 * font-size` to the left of the content box and the bullet size
/// is `0.35 em` (documented approximation, WP-4.9 follow-up when layout expose
/// the marker margin).
fn paint_marker(
    layout_box: &LayoutBox,
    ordinal: u32,
    list: &mut DisplayList,
    resources: &PaintResources<'_>,
    alpha_scale: f32,
) {
    let Some(style) = layout_box.style.as_ref() else {
        return;
    };
    let font_size = style.font_size;
    let baseline = first_text_baseline(layout_box)
        .unwrap_or(layout_box.dimensions.content.y + font_size * 0.8);
    let right = layout_box.dimensions.content.x - font_size * 0.5;
    let color = scaled(style.color, alpha_scale);

    // `list-style-image`: paint the decoded bitmap (~1 em square) in place of
    // the geometric/decimal marker. A missing URL falls back to list-style-type.
    if let Some(url) = style.list_style_image.as_deref()
        && let Some(image) = resources.backgrounds.get(url)
        && !image.is_empty()
    {
        let size = font_size.max(1.0);
        let center_y = baseline - font_size * 0.35;
        let marker_rect = Rect::new(right - size, center_y - size / 2.0, size, size);
        resources.record(marker_rect, layout_box.node_id(), HitKind::Marker);
        list.push(DisplayItem::Image {
            rect: marker_rect,
            image: std::sync::Arc::clone(image),
        });
        return;
    }

    match style.list_style_type {
        ListStyleType::None => {}
        ListStyleType::Decimal => {
            let text = format!("{ordinal}.");
            let width = text.chars().count() as f32 * font_size * 0.5;
            let rect = Rect::new(right - width, baseline - font_size * 0.8, width, font_size);
            resources.record(rect, layout_box.node_id(), HitKind::Marker);
            list.push(DisplayItem::Text {
                text,
                rect,
                color,
                font_size,
                font_family: style.font_family.clone(),
                font_weight: style.font_weight,
                font_italic: matches!(
                    style.font_style,
                    css::FontStyle::Italic | css::FontStyle::Oblique
                ),
                advance: width / 2.0,
                baseline: font_size * 0.8,
            });
        }
        kind => {
            let size = (font_size * 0.35).max(2.0);
            let center_y = baseline - font_size * 0.35;
            let rect = Rect::new(right - size, center_y - size / 2.0, size, size);
            resources.record(rect, layout_box.node_id(), HitKind::Marker);
            let kind = match kind {
                ListStyleType::Disc => MarkerKind::Disc,
                ListStyleType::Circle => MarkerKind::Circle,
                _ => MarkerKind::Square,
            };
            list.push(DisplayItem::Marker { rect, color, kind });
        }
    }
}

/// Union of the painted bounds of `items` (clip items ignored).
fn items_bounds(items: &[DisplayItem]) -> Option<Rect> {
    let mut bounds: Option<Rect> = None;
    for item in items {
        let rect = match item {
            DisplayItem::PushClip { .. } | DisplayItem::PopClip => None,
            DisplayItem::OpacityGroup { rect, .. } => Some(*rect),
            other => other.bounds(),
        };
        if let Some(rect) = rect
            && rect.width > 0.0
            && rect.height > 0.0
        {
            bounds = Some(match bounds {
                None => rect,
                Some(current) => {
                    let x0 = current.x.min(rect.x);
                    let y0 = current.y.min(rect.y);
                    let x1 = (current.x + current.width).max(rect.x + rect.width);
                    let y1 = (current.y + current.height).max(rect.y + rect.height);
                    Rect::new(x0, y0, x1 - x0, y1 - y0)
                }
            });
        }
    }
    bounds
}
