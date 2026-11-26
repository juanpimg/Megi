//! CSS → raster geometry transform (owned, no new crate edges).
//!
//! The display list is authored in **CSS pixels** by `paint` and hit-testing
//! consumes those same CSS-pixel rectangles. When the host rasterizes at a
//! device pixel ratio other than 1, only the *raster copy* is scaled here; the
//! CSS display list and the hit regions are never mutated. See
//! `docs/DPR-RASTER-BOUNDARY.md`.

use std::sync::Arc;

use crate::{
    BorderRadii, BorderSide, BoxShadow, DisplayItem, DisplayList, FilterFunction, GradientKind,
    Length, RadialExtent, Radius, Rect, TextMaskRun,
};

/// Smallest accepted raster scale (guards hostile/host values).
pub const MIN_RASTER_SCALE: f32 = 0.25;
/// Largest accepted raster scale (covers 8x HiDPI displays).
pub const MAX_RASTER_SCALE: f32 = 8.0;

/// An owned CSS→raster geometry transform. `Copy`, no lifetimes, no shared mutability.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RasterScale {
    factor: f32,
}

impl RasterScale {
    /// No scaling: every geometry value is preserved exactly.
    pub const ONE: Self = Self { factor: 1.0 };

    /// Sanitizes a host-provided factor: non-finite/non-positive values fall
    /// back to 1.0, others are clamped to [`MIN_RASTER_SCALE`]..=[`MAX_RASTER_SCALE`].
    pub fn new(factor: f32) -> Self {
        if !factor.is_finite() || factor <= 0.0 {
            Self::ONE
        } else {
            Self {
                factor: factor.clamp(MIN_RASTER_SCALE, MAX_RASTER_SCALE),
            }
        }
    }

    /// The effective factor, always finite and positive.
    pub const fn factor(&self) -> f32 {
        self.factor
    }

    /// Whether this transform is a byte-preserving no-op.
    pub const fn is_identity(&self) -> bool {
        self.factor == 1.0
    }

    /// Scales one CSS length to raster pixels.
    pub fn length(&self, css_px: f32) -> f32 {
        css_px * self.factor
    }

    /// Scales one CSS rectangle to raster pixels.
    pub fn rect(&self, rect: Rect) -> Rect {
        Rect::new(
            self.length(rect.x),
            self.length(rect.y),
            self.length(rect.width),
            self.length(rect.height),
        )
    }

    /// Returns an **owned** raster-space copy of `list`.
    ///
    /// Identity returns a plain clone: all existing scale-1 renders stay
    /// byte-identical (golden PNGs included). Nested groups are transformed
    /// recursively; text, images and colors keep their payloads.
    pub fn apply(&self, list: &DisplayList) -> DisplayList {
        if self.is_identity() {
            return list.clone();
        }
        DisplayList {
            items: list
                .items
                .iter()
                .map(|item| self.scale_item(item))
                .collect(),
        }
    }

    fn scale_item(&self, item: &DisplayItem) -> DisplayItem {
        let s = self.factor;
        match item {
            DisplayItem::SolidColor { rect, color } => DisplayItem::SolidColor {
                rect: self.rect(*rect),
                color: *color,
            },
            DisplayItem::Border { rect, sides } => DisplayItem::Border {
                rect: self.rect(*rect),
                sides: sides.map(|side| scale_side(side, s)),
            },
            DisplayItem::Text {
                text,
                rect,
                color,
                font_size,
                font_family,
                font_weight,
                font_italic,
                advance,
                baseline,
            } => DisplayItem::Text {
                text: text.clone(),
                rect: self.rect(*rect),
                color: *color,
                font_size: self.length(*font_size),
                font_family: font_family.clone(),
                font_weight: *font_weight,
                font_italic: *font_italic,
                advance: self.length(*advance),
                baseline: self.length(*baseline),
            },
            DisplayItem::TextDecoration { rect, color } => DisplayItem::TextDecoration {
                rect: self.rect(*rect),
                color: *color,
            },
            DisplayItem::Marker { rect, color, kind } => DisplayItem::Marker {
                rect: self.rect(*rect),
                color: *color,
                kind: *kind,
            },
            DisplayItem::BackdropFilter { rect, filters } => DisplayItem::BackdropFilter {
                rect: self.rect(*rect),
                filters: filters
                    .iter()
                    .map(|filter| scale_filter(*filter, s))
                    .collect(),
            },
            DisplayItem::EffectGroup {
                rect,
                filters,
                blend,
                items,
            } => DisplayItem::EffectGroup {
                rect: self.rect(*rect),
                filters: filters
                    .iter()
                    .map(|filter| scale_filter(*filter, s))
                    .collect(),
                blend: *blend,
                items: items.iter().map(|item| self.scale_item(item)).collect(),
            },
            DisplayItem::OpacityGroup {
                rect,
                opacity,
                items,
            } => DisplayItem::OpacityGroup {
                rect: self.rect(*rect),
                opacity: *opacity,
                items: items.iter().map(|item| self.scale_item(item)).collect(),
            },
            DisplayItem::Gradient { rect, gradient } => DisplayItem::Gradient {
                rect: self.rect(*rect),
                gradient: scale_gradient(gradient, s),
            },
            DisplayItem::Image { rect, image } => DisplayItem::Image {
                rect: self.rect(*rect),
                image: Arc::clone(image),
            },
            DisplayItem::RoundedRect { rect, radii, color } => DisplayItem::RoundedRect {
                rect: self.rect(*rect),
                radii: scale_radii(*radii, s),
                color: *color,
            },
            DisplayItem::RoundedBorder { rect, sides, radii } => DisplayItem::RoundedBorder {
                rect: self.rect(*rect),
                sides: sides.map(|side| scale_side(side, s)),
                radii: scale_radii(*radii, s),
            },
            DisplayItem::PushRoundedGroup { rect, radii } => DisplayItem::PushRoundedGroup {
                rect: self.rect(*rect),
                radii: scale_radii(*radii, s),
            },
            DisplayItem::PopRoundedGroup => DisplayItem::PopRoundedGroup,
            DisplayItem::BoxShadow {
                rect,
                radii,
                shadow,
            } => DisplayItem::BoxShadow {
                rect: self.rect(*rect),
                radii: scale_radii(*radii, s),
                shadow: scale_shadow(*shadow, s),
            },
            DisplayItem::TextShadow {
                text,
                rect,
                color,
                font_size,
                font_family,
                font_weight,
                font_italic,
                advance,
                baseline,
                offset_x,
                offset_y,
                blur,
            } => DisplayItem::TextShadow {
                text: text.clone(),
                rect: self.rect(*rect),
                color: *color,
                font_size: self.length(*font_size),
                font_family: font_family.clone(),
                font_weight: *font_weight,
                font_italic: *font_italic,
                advance: self.length(*advance),
                baseline: self.length(*baseline),
                offset_x: self.length(*offset_x),
                offset_y: self.length(*offset_y),
                blur: self.length(*blur),
            },
            DisplayItem::PushTextMaskGroup { rect, runs } => DisplayItem::PushTextMaskGroup {
                rect: self.rect(*rect),
                runs: Arc::new(
                    runs.iter()
                        .map(|run| TextMaskRun {
                            text: run.text.clone(),
                            rect: self.rect(run.rect),
                            font_size: self.length(run.font_size),
                            font_family: run.font_family.clone(),
                            font_weight: run.font_weight,
                            font_italic: run.font_italic,
                            advance: self.length(run.advance),
                            baseline: self.length(run.baseline),
                        })
                        .collect(),
                ),
            },
            DisplayItem::PopTextMaskGroup => DisplayItem::PopTextMaskGroup,
            DisplayItem::PushMaskGroup { rect, mask } => DisplayItem::PushMaskGroup {
                rect: self.rect(*rect),
                mask: Arc::clone(mask),
            },
            DisplayItem::PopMaskGroup => DisplayItem::PopMaskGroup,
            DisplayItem::PushClip { rect } => DisplayItem::PushClip {
                rect: self.rect(*rect),
            },
            DisplayItem::PopClip => DisplayItem::PopClip,
        }
    }
}

fn scale_side(side: BorderSide, factor: f32) -> BorderSide {
    BorderSide {
        width: side.width * factor,
        ..side
    }
}

fn scale_length(length: Length, factor: f32) -> Length {
    match length {
        Length::Px(px) => Length::Px(px * factor),
        // Percentages and `auto` are resolved against the (already scaled)
        // rectangle by the rasterizer; they must not be multiplied here.
        other => other,
    }
}

fn scale_radii(radii: BorderRadii, factor: f32) -> BorderRadii {
    let scale_radius = |radius: Radius| Radius {
        x: scale_length(radius.x, factor),
        y: scale_length(radius.y, factor),
    };
    BorderRadii {
        top_left: scale_radius(radii.top_left),
        top_right: scale_radius(radii.top_right),
        bottom_right: scale_radius(radii.bottom_right),
        bottom_left: scale_radius(radii.bottom_left),
    }
}

fn scale_shadow(shadow: BoxShadow, factor: f32) -> BoxShadow {
    BoxShadow {
        offset_x: shadow.offset_x * factor,
        offset_y: shadow.offset_y * factor,
        blur: shadow.blur * factor,
        spread: shadow.spread * factor,
        ..shadow
    }
}

fn scale_filter(filter: FilterFunction, factor: f32) -> FilterFunction {
    match filter {
        // `blur()` and `drop-shadow()` lengths are CSS px; the other filter
        // functions are dimensionless and must stay untouched.
        FilterFunction::Blur(radius) => FilterFunction::Blur(radius * factor),
        FilterFunction::DropShadow {
            offset_x,
            offset_y,
            blur,
            color,
        } => FilterFunction::DropShadow {
            offset_x: offset_x * factor,
            offset_y: offset_y * factor,
            blur: blur * factor,
            color,
        },
        other => other,
    }
}

fn scale_gradient(gradient: &Arc<crate::Gradient>, factor: f32) -> Arc<crate::Gradient> {
    let mut scaled = (**gradient).clone();
    for stop in &mut scaled.stops {
        if let Some(position) = stop.position {
            stop.position = Some(scale_length(position, factor));
        }
    }
    for hint in &mut scaled.hints {
        if let Some(length) = *hint {
            *hint = Some(scale_length(length, factor));
        }
    }
    match &mut scaled.kind {
        GradientKind::Linear(_) => {}
        GradientKind::Radial(radial) => {
            if let Some(extent) = radial.extent {
                radial.extent = Some(match extent {
                    RadialExtent::Explicit(rx, ry) => {
                        RadialExtent::Explicit(rx * factor, ry * factor)
                    }
                    other => other,
                });
            }
            radial.position.x = scale_length(radial.position.x, factor);
            radial.position.y = scale_length(radial.position.y, factor);
        }
        GradientKind::Conic(conic) => {
            conic.position.x = scale_length(conic.position.x, factor);
            conic.position.y = scale_length(conic.position.y, factor);
        }
    }
    Arc::new(scaled)
}

#[cfg(test)]
mod dpr_raster_scale_tests;
