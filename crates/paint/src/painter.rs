//! # Display List Painter
//!
//! Traverses the CSS [`LayoutBox`] tree in CSS 2.2 Appendix E painting order
//! to produce a decoupled, flat [`DisplayList`].
//!
//! ## Canvas and root/`body` background propagation
//!
//! CSS 2.2 §14.2 (`https://www.w3.org/TR/CSS22/colors.html#background`) and
//! CSS Backgrounds 3 §2.11.2 define the *canvas* as the infinite surface the
//! document is rendered onto. The canvas background is not painted by an
//! element:
//!
//! - If the root element (`html`) has a non-transparent background, that
//!   background is propagated to the canvas and extends over the whole
//!   viewport; the root element does not paint it again.
//! - If the root element is transparent (the initial value), the background of
//!   the first HTML `body` child element is propagated instead, and the `body`
//!   element does not paint it again.
//!
//! [`build_display_list`] emits the propagated color as the first display item,
//! covering `viewport`, and suppresses the corresponding element background.
//! When neither element has a background the display list carries no canvas
//! item and the rasterizer default ([`RenderConfig`](crate::RenderConfig))
//! shows through; the Phase 4 pipeline uses opaque white.

use css::{BorderStyle, Color};
use dom::NodeId;
use layout::{BoxType, LayoutBox, Rect};

use crate::display_list::{BorderSide, DisplayItem, DisplayList};

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

/// Builds a deterministic [`DisplayList`] from a laid-out [`LayoutBox`] tree.
///
/// `viewport` is the canvas rectangle (normally `Rect::new(0.0, 0.0, w, h)`);
/// it is used for the propagated root/`body` canvas background.
///
/// Painting order per box (CSS 2.2 Appendix E, common in-flow case):
/// 1. Background color fill over the border box (CSS 2.2 §8.5.2).
/// 2. Border outlines around the border box, one paint per side.
/// 3. Text runs for inline text fragments.
/// 4. Children traversed recursively in tree order.
///
/// Element backgrounds are skipped for format-only boxes
/// ([`BoxType::LineBox`], [`BoxType::AnonymousBlock`], [`BoxType::TextNode`]):
/// those never have a background of their own in the CSS box model (audit
/// finding F-02).
pub fn build_display_list(root: &LayoutBox, viewport: Rect) -> DisplayList {
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
    paint_box(root, &mut list, skip, true);
    list
}

/// Returns the first block-level child of the root box, interpreted as `body`.
///
/// The layout tree drops `display: none` descendants (including `head`), so
/// the first block-level element child of `html` is the document `body` for
/// HTML documents produced by the HTML parser (`html::parse_document`).
///
/// This is a structural inference: a future WP can thread explicit root/`body`
/// [`NodeId`]s through the painter API once `<body>` can be matched by tag name
/// without making `paint` depend on the `html` crate.
fn first_body_box(root: &LayoutBox) -> Option<&LayoutBox> {
    root.children
        .iter()
        .find(|child| matches!(child.box_type, BoxType::BlockNode(_)))
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
/// bottom, left).
///
/// CSS 2.2 §8.5.3: when `border-style` is `none` or `hidden` the computed
/// border width is `0`, so a `border-width: 5px` without a style paints
/// nothing. The CSS crate already computes that for `border-style`, but paint
/// enforces it here as well so a malformed style map can never paint a phantom
/// frame around every element (audit finding F-07).
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

/// Recursively emits the painting instructions for `layout_box`.
fn paint_box(layout_box: &LayoutBox, list: &mut DisplayList, skip: PaintSkip, is_root: bool) {
    let style = layout_box.style.as_ref();
    let is_element = layout_box.node_id().is_some();

    // 1. Background color (element boxes only, CSS 2.2 §14.2).
    let background_is_propagated =
        (is_root && skip.root_background) || (is_element && layout_box.node_id() == skip.body_node);
    if is_element
        && !background_is_propagated
        && let Some(style) = style
        && style.background_color.a > 0
    {
        // CSS 2.2 §8.5.2 / css-backgrounds-3 §2.7: the background painting area
        // of a box extends under its border (border box), not just the padding
        // box (audit finding F-08).
        list.push(DisplayItem::SolidColor {
            rect: layout_box.dimensions.border_box(),
            color: style.background_color,
        });
    }

    // 2. Borders, with per-side widths, styles and colors (CSS 2.2 §8.5).
    if is_element && let Some(style) = style {
        let sides = border_sides(style, layout_box.dimensions.border);
        if sides.iter().any(BorderSide::is_visible) {
            list.push(DisplayItem::Border {
                rect: layout_box.dimensions.border_box(),
                sides,
            });
        }
    }

    // 3. Text content of inline-level text fragments.
    if let BoxType::TextNode(text) = &layout_box.box_type
        && !text.trim().is_empty()
    {
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

        list.push(DisplayItem::Text {
            text: text.clone(),
            rect: layout_box.dimensions.content,
            color,
            font_size,
            font_family,
            font_weight,
            font_italic,
            advance,
            // Baseline propagated by the IFC (`layout::LayoutBox::baseline`).
            baseline: layout_box.baseline,
        });
    }

    // 4. Children in tree order (Ap. E step 4/7 in the in-flow case).
    for child in &layout_box.children {
        paint_box(child, list, skip, false);
    }
}
