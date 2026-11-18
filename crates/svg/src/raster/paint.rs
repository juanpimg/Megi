//! Submodule of [`super`]: gradient/pattern paint resolution.

use super::*;

/// A paint prepared for compositing: solid colour, or a gradient evaluator
/// (inverse device → gradient-parameter matrix precomputed once per shape).
pub(crate) enum PreparedPaint<'a> {
    Solid(Color),
    Gradient {
        spec: &'a GradientSpec,
        inverse: crate::transform::Transform,
    },
    Pattern {
        spec: &'a PatternSpec,
    },
}

/// Resolves a paint value against the document gradients for one shape.
pub(crate) fn prepare_paint<'a>(
    paint: Paint,
    gradients: &'a [GradientSpec],
    patterns: &'a [PatternSpec],
    shape_transform: crate::transform::Transform,
    bbox: (f32, f32, f32, f32),
    viewport: crate::transform::Transform,
) -> Option<PreparedPaint<'a>> {
    match paint {
        Paint::None => None,
        // Resolved by Style::resolve_current_color during document building; defensive
        // fallback for any style constructed elsewhere.
        Paint::CurrentColor => Some(PreparedPaint::Solid(Color::BLACK)),
        Paint::Solid(color) => (color.a > 0).then_some(PreparedPaint::Solid(color)),
        Paint::Pattern(index) => patterns
            .get(index as usize)
            .filter(|pattern| pattern.usable())
            .map(|spec| PreparedPaint::Pattern { spec }),
        Paint::Gradient(index) => {
            let spec = gradients.get(index as usize)?;
            // device = viewport * shape_transform * units_matrix * gradient_transform
            let matrix = viewport
                .then(shape_transform)
                .then(spec.parameter_to_user(bbox));
            let inverse = matrix.inverse()?;
            Some(PreparedPaint::Gradient { spec, inverse })
        }
    }
}
