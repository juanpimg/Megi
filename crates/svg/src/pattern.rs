//! SVG `<pattern>` model and tile placement helpers.
//!
//! Patterns are parsed once (their child shapes are flattened in pattern-local user space);
//! the rasterizer repeats them over the shape being painted, clipped by the shape's
//! coverage and by the pattern tile. Anti-DoS limits: `MAX_PATTERN_TILES` per paint and a
//! minimum tile size; pattern→pattern references are depth-limited (cycles are dropped,
//! never panic).

use crate::document::Shape;
use crate::transform::Transform;

/// Anti-DoS: maximum pattern tiles rendered for one painted shape.
pub(crate) const MAX_PATTERN_TILES: u64 = 10_000;
/// Minimum pattern tile width/height in user units (avoids infinite tile counts).
pub(crate) const MIN_PATTERN_TILE: f32 = 0.01;
/// Maximum pattern reference recursion depth (pattern → pattern → ...).
pub(crate) const MAX_PATTERN_DEPTH: usize = 4;

/// `patternUnits` / `patternContentUnits`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum Units {
    /// `userSpaceOnUse`.
    #[default]
    UserSpaceOnUse,
    /// `objectBoundingBox`.
    ObjectBoundingBox,
}

impl Units {
    pub(crate) fn parse(raw: Option<&str>) -> Self {
        match raw.map(str::trim) {
            Some(value) if value.eq_ignore_ascii_case("objectBoundingBox") => {
                Self::ObjectBoundingBox
            }
            _ => Self::UserSpaceOnUse,
        }
    }
}

/// A parsed `<pattern>`.
#[derive(Clone, Debug)]
pub(crate) struct PatternSpec {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub pattern_units: Units,
    pub content_units: Units,
    pub transform: Transform,
    /// Pattern content shapes flattened in pattern-local user space.
    pub shapes: Vec<Shape>,
}

impl PatternSpec {
    /// Resolves the pattern tile rectangle in the referencing element's user space.
    pub(crate) fn tile_box(&self, bbox: (f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
        let map = |value: f32, base: f32, size: f32| match self.pattern_units {
            Units::UserSpaceOnUse => value,
            Units::ObjectBoundingBox => base + value * size,
        };
        let map_size = |value: f32, size: f32| match self.pattern_units {
            Units::UserSpaceOnUse => value,
            Units::ObjectBoundingBox => value * size,
        };
        let x = map(self.x, bbox.0, bbox.2);
        let y = map(self.y, bbox.1, bbox.3);
        let width = map_size(self.width, bbox.2);
        let height = map_size(self.height, bbox.3);
        (x, y, width, height)
    }

    /// Maps pattern-content coordinates into the referencing element's user space units.
    pub(crate) fn content_transform(&self, bbox: (f32, f32, f32, f32)) -> Transform {
        match self.content_units {
            Units::UserSpaceOnUse => Transform::IDENTITY,
            Units::ObjectBoundingBox => {
                let sx = if bbox.2.abs() > f32::EPSILON {
                    bbox.2
                } else {
                    1.0
                };
                let sy = if bbox.3.abs() > f32::EPSILON {
                    bbox.3
                } else {
                    1.0
                };
                Transform::translate(bbox.0, bbox.1).then(Transform::scale(sx, sy))
            }
        }
    }

    pub(crate) fn usable(&self) -> bool {
        self.width.is_finite()
            && self.height.is_finite()
            && self.x.is_finite()
            && self.y.is_finite()
            && self.width.abs() >= MIN_PATTERN_TILE
            && self.height.abs() >= MIN_PATTERN_TILE
            && !self.shapes.is_empty()
    }
}
