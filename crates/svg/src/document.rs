//! SVG document model: presentation styles, shapes, `defs`/`use` and viewport.
//!
//! The builder walks the XML tree once, resolving inherited presentation
//! attributes (or basic `style="..."` declarations), accumulating transforms and
//! flattening each supported shape to polylines in root user space. Shapes keep
//! their paint state; the viewport (`viewBox`/`preserveAspectRatio`) is resolved
//! into a transform when the rasterizer starts.

use std::collections::HashMap;

use crate::color::{Color, parse_color};
use crate::css::{
    SelectorBudget, StyleRule, matching_declarations, parse_declarations, parse_stylesheet,
};
use crate::gradient::GradientSpec;
use crate::path::{SubPath, parse_path_data};
use crate::pattern::{PatternSpec, Units};
use crate::text::{MAX_TEXT_CHARS, MAX_TEXTS, TextAnchor, TextSpec};
use crate::transform::{Transform, parse_number_list};
use crate::xml::{XmlElement, XmlNode, local_name};
use crate::{Limits, SvgError};

mod build;
mod builder;
mod effects;
mod model;
mod paint;
mod parse;
mod shapes;
mod smil;
mod smil_value;
mod style;
mod text;
mod visit;

// Re-export every submodule item at this level so sibling modules (and the
// sibling `crate::raster`/`crate::pattern`/`crate::text` modules) keep the same
// `crate::document::*` paths as before the split.
pub(crate) use effects::*;
pub(crate) use model::*;
pub(crate) use paint::*;
pub(crate) use parse::*;
pub(crate) use smil::*;
pub(crate) use smil_value::*;
pub(crate) use style::*;

/// Inheritable presentation state.
#[derive(Clone, Debug)]
struct Style {
    /// The inherited `color` property; used to resolve `currentColor` (#1276).
    color: Color,
    /// `display: none` skips the whole subtree (#37).
    display: bool,
    /// Inherited `visibility`; a `visible` descendant still paints (#37).
    visibility: bool,
    fill: Paint,
    stroke: Paint,
    fill_opacity: f32,
    stroke_opacity: f32,
    stroke_width: f32,
    linecap: LineCap,
    linejoin: LineJoin,
    miter_limit: f32,
    fill_rule: FillRule,
    clip_path: Option<String>,
    mask: Option<String>,
    filter: Option<String>,
    font_family: Option<String>,
    font_size: Option<f32>,
    font_weight: Option<u16>,
    font_style: Option<String>,
    letter_spacing: Option<f32>,
    text_anchor: Option<String>,
}

struct Builder<'a> {
    defs: &'a HashMap<String, &'a XmlElement>,
    rules: &'a [StyleRule],
    gradient_ids: &'a HashMap<String, u32>,
    pattern_ids: &'a HashMap<String, u32>,
    clip_ids: &'a HashMap<String, u32>,
    mask_ids: &'a HashMap<String, u32>,
    filter_ids: &'a HashMap<String, u32>,
    clip_paths: &'a [ClipPathSpec],
    masks: &'a [MaskSpec],
    filters: &'a [FilterSpec],
    limits: &'a Limits,
    /// Selector evaluations performed by the in-SVG stylesheet cascade.
    selector_checks_used: usize,
    /// Path segments budget tracker.
    segments_used: usize,
    shapes: Vec<Shape>,
    texts: Vec<TextSpec>,
    use_stack: Vec<String>,
    use_expansions: usize,
    active_clips: Vec<EffectRef>,
    active_masks: Vec<EffectRef>,
    active_filter: Option<EffectRef>,
    /// Incremental `objectBoundingBox` accumulators for the currently open
    /// groups. Each shape is folded into every *active* frame once, instead of
    /// rescanning the whole emitted subtree when each group closes (#442).
    group_bbox_stack: Vec<GroupBboxFrame>,
}

/// Accumulator for one open group's descendant bounding box.
struct GroupBboxFrame {
    /// Running union of the transformed descendant points, if any.
    bbox: Option<(f32, f32, f32, f32)>,
    /// False for groups without effects, where no bbox is ever needed.
    active: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::parse_document;

    fn build(source: &str) -> Document {
        let root = parse_document(source.as_bytes(), &Limits::default()).expect("xml");
        Document::build(&root, &Limits::default(), 0.0).expect("document")
    }

    #[test]
    fn test_intrinsic_size_and_viewbox() {
        let doc = build("<svg width='40' height='20'><rect width='40' height='20'/></svg>");
        assert_eq!((doc.width, doc.height), (40.0, 20.0));
        let doc = build("<svg viewBox='0 0 10 5'><rect width='10' height='5'/></svg>");
        assert_eq!((doc.width, doc.height), (10.0, 5.0));
        let transform = doc.viewport_transform(20, 10);
        assert_eq!(transform.apply(10.0, 5.0), (20.0, 10.0));
    }

    #[test]
    fn test_shapes_and_style_inheritance() {
        let doc = build(
            "<svg width='10' height='10'><g fill='red' stroke='blue' stroke-width='2'>\
             <rect x='1' y='1' width='4' height='4'/><circle cx='6' cy='6' r='2'/></g></svg>",
        );
        assert_eq!(doc.shapes.len(), 2);
        assert_eq!(
            doc.shapes[0].fill,
            Paint::Solid(Color::rgba(255, 0, 0, 255))
        );
        assert_eq!(
            doc.shapes[0].stroke,
            Paint::Solid(Color::rgba(0, 0, 255, 255))
        );
        assert_eq!(doc.shapes[0].stroke_width, 2.0);
    }

    #[test]
    fn test_defs_and_use() {
        let doc = build(
            "<svg width='10' height='10'><defs><rect id='r' width='2' height='2' fill='#00ff00'/></defs>\
             <use href='#r' x='3' y='4'/></svg>",
        );
        assert_eq!(doc.shapes.len(), 1);
        let points = &doc.shapes[0].subpaths[0].points;
        assert!(points.contains(&(3.0, 4.0)));
    }

    #[test]
    fn test_use_cycle_is_ignored() {
        let doc = build(
            "<svg width='10' height='10'><defs><g id='a'><use href='#a'/></g></defs><use href='#a'/></svg>",
        );
        assert!(doc.shapes.is_empty());
    }
}
