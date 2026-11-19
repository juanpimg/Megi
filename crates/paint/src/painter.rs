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

use megi_core::NodeId;
use megi_css::{BorderStyle, Color, Display, ListStyleType, Position, TextDecoration, Visibility};
use megi_layout::{BoxType, LayoutBox, Rect};

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use megi_display::{
    BackgroundImages, BorderSide, DisplayItem, DisplayList, ImageStore, MAX_SHADOWS,
    MAX_TEXT_MASK_RUNS, MarkerKind, TextMaskRun,
};
use megi_display::{HitKind, HitRegion, RegionCollector};

// Cohesive submodules of the display-list painter. Declared here (not in
// `lib.rs`) so the crate's public surface stays a single `painter` module.
mod backgrounds;
mod blocks;
mod display_list;
mod effects;
mod hit_regions;
mod images;
mod inline;
mod stacking;
mod text;

// Keep the crate-public entry points at `painter::*` (unchanged public API).
pub use display_list::{
    build_display_list, build_display_list_and_hit_regions, build_display_list_with_hit_regions,
    build_display_list_with_images, build_display_list_with_images_and_backgrounds,
};

// Private re-exports: every submodule does `use super::*`, so these names are
// visible to all siblings without widening the crate's public API.
use backgrounds::paint_background_border;
use blocks::{paint_block_backgrounds, paint_floats};
use effects::{
    MAX_TEXT_MASK_COLLECTORS, TEXT_MASK_COLLECTORS, collect_text_mask_runs, effect_padding,
    has_effects, inline_filtered_solid, overflow_rounded_clip, tree_has_effects,
};
use hit_regions::{
    pop_clip_item, pop_rounded_group, pop_text_mask_group, push_clip_item, push_rounded_group,
    push_text_mask_group,
};
use images::paint_replaced_element;
use inline::paint_inline_content;
use stacking::{index_stacking_contexts, paint_stacking_context};
use text::{is_list_item, paint_marker, paint_text_decorations, paint_text_run};

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
/// elements are keyed by [`megi_core::NodeId`] ([`ImageStore`]) while CSS background
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
    /// True when any box in the layout tree authors `filter`/`mix-blend-mode`.
    ///
    /// Precomputed once per pass so the hot `phase()` classification can skip
    /// per-box style checks when the whole document has no effects (R141).
    effects_present: bool,
}

impl PaintResources<'_> {
    /// Records a painted primitive for hit-testing (no-op when regions are off).
    #[inline]
    fn record(&self, rect: Rect, node_id: Option<megi_core::NodeId>, kind: HitKind) {
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
    /// Top-layer modal element rendered after the entire document stacking tree (#1546).
    TopLayer,
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

/// Appendix E phase of `layout_box` inside its nearest stacking context.
#[inline]
fn phase(layout_box: &LayoutBox, effects_present: bool) -> Phase {
    if layout_box.is_top_layer {
        return Phase::TopLayer;
    }
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
    } else if effects_present && has_effects(layout_box) {
        // `filter`/`mix-blend-mode` create a stacking context (CSS Filter
        // Effects 1 §2 / Compositing 1 §5): paint it in the zero/negative/
        // positive phase instead of the in-flow passes (R137).
        if layout_box.z_index < 0 {
            Phase::Negative
        } else if layout_box.z_index > 0 {
            Phase::Positive
        } else {
            Phase::Zero
        }
    } else if layout_box.is_float {
        Phase::Float
    } else {
        Phase::InFlow
    }
}

/// Applies `alpha_scale` to a color's alpha (flat-opacity fallback).
#[inline]
fn scaled(color: Color, alpha_scale: f32) -> Color {
    if alpha_scale >= 1.0 {
        return color;
    }
    let alpha = (f32::from(color.a) * alpha_scale.max(0.0)).round() as u8;
    Color::new(color.r, color.g, color.b, alpha)
}

/// Clip that applies to a box's **own** background/border (#1713).
///
/// A box that establishes clipping carries its own padding box in `clip`; CSS
/// 2.2 §11.1.1 excludes that part from clipping its own background, so use only
/// the clip inherited from ancestors in that case.
#[inline]
pub(super) fn own_background_clip(layout_box: &LayoutBox) -> Option<Rect> {
    let own_clip = layout_box
        .style
        .as_ref()
        .is_some_and(|style| !matches!(style.overflow, megi_css::Overflow::Visible));
    if own_clip {
        layout_box.inherited_clip
    } else {
        layout_box.clip
    }
}

/// True when the box's own style is hidden (CSS 2.2 §11.2 `visibility`).
#[inline]
fn is_hidden(layout_box: &LayoutBox) -> bool {
    layout_box
        .style
        .as_ref()
        .is_some_and(|style| matches!(style.visibility, Visibility::Hidden | Visibility::Collapse))
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
