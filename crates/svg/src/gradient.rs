//! SVG gradients (`linearGradient` / `radialGradient`) parsing and evaluation.
//!
//! Supported: `x1/y1/x2/y2`, `cx/cy/r/fx/fy`, `gradientUnits`
//! (`objectBoundingBox` default and `userSpaceOnUse`), `spreadMethod`
//! (`pad`/`reflect`/`repeat`), `gradientTransform`, `<stop>` children
//! (`offset`/`stop-color`/`stop-opacity`) and `href`/`xlink:href` inheritance
//! between gradient definitions (with cycle detection and an anti-DoS cap).
//!
//! Percentages in `userSpaceOnUse` mode are treated as fractions of the current
//! user space (documented approximation); the common `objectBoundingBox` mode
//! maps them exactly to the element bounding box.

use std::collections::HashMap;

use crate::color::{Color, parse_color};
use crate::transform::Transform;
use crate::xml::{XmlElement, XmlNode, local_name};

/// Maximum number of `<stop>` entries considered per gradient.
pub(crate) const MAX_STOPS: usize = 256;
/// Maximum `href` inheritance depth between gradient definitions.
pub(crate) const MAX_REFERENCE_DEPTH: usize = 32;

/// How gradient parameter space maps to the element user space.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum GradientUnits {
    /// Coordinates are fractions of the element bounding box (SVG default).
    #[default]
    ObjectBoundingBox,
    /// Coordinates are in the element's current user space.
    UserSpaceOnUse,
}

/// Behaviour outside the `[0, 1]` gradient interval.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SpreadMethod {
    /// Clamp the last stop (default).
    #[default]
    Pad,
    Reflect,
    Repeat,
}

/// One resolved gradient stop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Stop {
    /// Clamped offset in `[0, 1]`.
    pub offset: f32,
    /// Stop colour with `stop-opacity` folded into the alpha channel.
    pub color: Color,
}

/// Gradient geometry in gradient parameter space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum GradientGeometry {
    Linear {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
    },
    Radial {
        cx: f32,
        cy: f32,
        r: f32,
        fx: f32,
        fy: f32,
    },
}

/// A fully resolved gradient definition (independent of the referencing shape).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GradientSpec {
    pub geometry: GradientGeometry,
    pub units: GradientUnits,
    pub spread: SpreadMethod,
    pub transform: Transform,
    pub stops: Vec<Stop>,
}

impl GradientSpec {
    /// True when the referenced element is a gradient.
    pub fn is_gradient_element(element: &XmlElement) -> bool {
        matches!(
            local_name(&element.name).to_ascii_lowercase().as_str(),
            "lineargradient" | "radialgradient"
        )
    }

    /// Resolves `#id` (or a bare id) into a gradient, following `href` chains.
    pub fn resolve(map: &HashMap<String, &XmlElement>, reference: &str) -> Option<Self> {
        let id = reference
            .trim()
            .strip_prefix('#')
            .unwrap_or(reference.trim());
        let mut stack: Vec<&str> = Vec::new();
        Self::resolve_inner(map, id, &mut stack)
    }

    fn resolve_inner<'a>(
        map: &'a HashMap<String, &'a XmlElement>,
        id: &'a str,
        stack: &mut Vec<&'a str>,
    ) -> Option<Self> {
        if stack.contains(&id) || stack.len() >= MAX_REFERENCE_DEPTH {
            // Cyclic (or too deep) gradient inheritance: no paint, no panic.
            return None;
        }
        let element = map.get(id).copied()?;
        if !Self::is_gradient_element(element) {
            return None;
        }
        stack.push(id);
        let base = match element
            .attribute("href")
            .or_else(|| element.attribute("xlink:href"))
        {
            Some(reference) => {
                let reference = reference.trim().trim_start_matches('#');
                match Self::resolve_inner(map, reference, stack) {
                    Some(base) => Some(base),
                    None => {
                        // Broken or cyclic reference chain: the gradient is unusable.
                        stack.pop();
                        return None;
                    }
                }
            }
            None => None,
        };
        let spec = Self::parse(element, base.as_ref());
        stack.pop();
        spec
    }

    fn parse(element: &XmlElement, base: Option<&GradientSpec>) -> Option<Self> {
        let is_linear = local_name(&element.name).eq_ignore_ascii_case("linearGradient");
        let inherited = base.filter(|base| {
            matches!(
                (base.geometry, is_linear),
                (_, true) if is_linear
            ) || (!is_linear && matches!(base.geometry, GradientGeometry::Radial { .. }))
        });
        let geometry = if is_linear {
            let (x1, y1, x2, y2) = match inherited.map(|base| base.geometry) {
                Some(GradientGeometry::Linear { x1, y1, x2, y2 }) => (x1, y1, x2, y2),
                _ => (0.0, 0.0, 1.0, 0.0),
            };
            GradientGeometry::Linear {
                x1: coordinate(element.attribute("x1"), x1),
                y1: coordinate(element.attribute("y1"), y1),
                x2: coordinate(element.attribute("x2"), x2),
                y2: coordinate(element.attribute("y2"), y2),
            }
        } else {
            let (cx, cy, r, fx, fy) = match inherited.map(|base| base.geometry) {
                Some(GradientGeometry::Radial { cx, cy, r, fx, fy }) => (cx, cy, r, fx, fy),
                _ => (0.5, 0.5, 0.5, 0.5, 0.5),
            };
            let cx = coordinate(element.attribute("cx"), cx);
            let cy = coordinate(element.attribute("cy"), cy);
            let radius = coordinate(element.attribute("r"), r);
            GradientGeometry::Radial {
                cx,
                cy,
                r: radius,
                fx: coordinate(element.attribute("fx"), fx),
                fy: coordinate(element.attribute("fy"), fy),
            }
        };
        let units = match element
            .attribute("gradientUnits")
            .unwrap_or("objectBoundingBox")
            .trim()
        {
            value if value.eq_ignore_ascii_case("userSpaceOnUse") => GradientUnits::UserSpaceOnUse,
            _ => GradientUnits::ObjectBoundingBox,
        };
        let spread = match element.attribute("spreadMethod").unwrap_or("pad").trim() {
            value if value.eq_ignore_ascii_case("reflect") => SpreadMethod::Reflect,
            value if value.eq_ignore_ascii_case("repeat") => SpreadMethod::Repeat,
            _ => SpreadMethod::Pad,
        };
        let transform = element
            .attribute("gradientTransform")
            .and_then(Transform::parse)
            .filter(|transform| transform.is_finite())
            .or_else(|| inherited.map(|base| base.transform))
            .unwrap_or(Transform::IDENTITY);
        let own_stops = parse_stops(element);
        let stops = if own_stops.is_empty() {
            inherited.map_or_else(Vec::new, |base| base.stops.clone())
        } else {
            own_stops
        };
        Some(Self {
            geometry,
            units,
            spread,
            transform,
            stops,
        })
    }

    /// Evaluates the gradient colour at parameter `t` (before spread handling).
    pub fn evaluate(&self, t: f32) -> Color {
        if self.stops.is_empty() {
            return Color::TRANSPARENT;
        }
        if self.stops.len() == 1 || !t.is_finite() {
            return self
                .stops
                .last()
                .map_or(Color::TRANSPARENT, |stop| stop.color);
        }
        let t = self.apply_spread(t);
        let first = &self.stops[0];
        let last = &self.stops[self.stops.len() - 1];
        if t <= first.offset {
            return first.color;
        }
        if t >= last.offset {
            return last.color;
        }
        let mut previous = first;
        for stop in &self.stops[1..] {
            if t <= stop.offset {
                let span = stop.offset - previous.offset;
                let local = if span > f32::EPSILON {
                    ((t - previous.offset) / span).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                return Color::rgba(
                    lerp_channel(previous.color.r, stop.color.r, local),
                    lerp_channel(previous.color.g, stop.color.g, local),
                    lerp_channel(previous.color.b, stop.color.b, local),
                    lerp_channel(previous.color.a, stop.color.a, local),
                );
            }
            previous = stop;
        }
        last.color
    }

    fn apply_spread(&self, t: f32) -> f32 {
        match self.spread {
            SpreadMethod::Pad => t.clamp(0.0, 1.0),
            SpreadMethod::Repeat => {
                let mut fraction = t - t.floor();
                if fraction < 0.0 {
                    fraction += 1.0;
                }
                fraction
            }
            SpreadMethod::Reflect => {
                let period = t.rem_euclid(2.0);
                if period <= 1.0 { period } else { 2.0 - period }
            }
        }
    }

    /// Computes the gradient parameter for a point already mapped to gradient
    /// parameter space.
    pub fn parameter(&self, x: f32, y: f32) -> f32 {
        match self.geometry {
            GradientGeometry::Linear { x1, y1, x2, y2 } => {
                let dx = x2 - x1;
                let dy = y2 - y1;
                let length_squared = dx * dx + dy * dy;
                if length_squared <= f32::EPSILON {
                    return 1.0;
                }
                (((x - x1) * dx + (y - y1) * dy) / length_squared).clamp(-1.0e6, 1.0e6)
            }
            GradientGeometry::Radial { cx, cy, r, fx, fy } => {
                let radius = r.abs();
                if radius <= f32::EPSILON {
                    return 1.0;
                }
                let dx = x - fx;
                let dy = y - fy;
                let fx_to_center_x = fx - cx;
                let fx_to_center_y = fy - cy;
                let a = dx * dx + dy * dy;
                let b = 2.0 * (fx_to_center_x * dx + fx_to_center_y * dy);
                let c = fx_to_center_x * fx_to_center_x + fx_to_center_y * fx_to_center_y
                    - radius * radius;
                if a <= f32::EPSILON {
                    return 0.0;
                }
                let discriminant = b * b - 4.0 * a * c;
                if discriminant < 0.0 {
                    return 1.0;
                }
                let root = discriminant.sqrt();
                let mut s = (-b + root) / (2.0 * a);
                if s <= f32::EPSILON {
                    s = (-b - root) / (2.0 * a);
                }
                if s > f32::EPSILON && s.is_finite() {
                    (1.0 / s).clamp(-1.0e6, 1.0e6)
                } else {
                    1.0
                }
            }
        }
    }

    /// Gradient-parameter → element user space transform for a shape with the
    /// given local bounding box (used for `objectBoundingBox`).
    pub fn units_matrix(&self, bbox: (f32, f32, f32, f32)) -> Transform {
        match self.units {
            GradientUnits::UserSpaceOnUse | GradientUnits::ObjectBoundingBox => {
                let (x, y, width, height) = bbox;
                if matches!(self.units, GradientUnits::ObjectBoundingBox)
                    && width.abs() > f32::EPSILON
                    && height.abs() > f32::EPSILON
                {
                    Transform::translate(x, y).then(Transform::scale(width, height))
                } else {
                    Transform::IDENTITY
                }
            }
        }
    }

    /// Maps a point in gradient parameter space to the shape's local user space.
    ///
    /// `gradientTransform` is applied in the gradient coordinate system (before
    /// the bounding-box mapping, per SVG), i.e. `B * G`.
    pub fn parameter_to_user(&self, bbox: (f32, f32, f32, f32)) -> Transform {
        self.units_matrix(bbox).then(self.transform)
    }
}

fn parse_stops(element: &XmlElement) -> Vec<Stop> {
    let mut stops: Vec<Stop> = Vec::new();
    for child in &element.children {
        let XmlNode::Element(child) = child else {
            continue;
        };
        if !local_name(&child.name).eq_ignore_ascii_case("stop") || stops.len() >= MAX_STOPS {
            continue;
        }
        let offset = child
            .attribute("offset")
            .and_then(parse_fraction)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        let color = child
            .attribute("stop-color")
            .and_then(parse_color)
            .unwrap_or(Color::BLACK);
        let opacity = child
            .attribute("stop-opacity")
            .and_then(parse_fraction)
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        stops.push(Stop {
            offset,
            color: color.with_opacity(opacity),
        });
    }
    stops.sort_by(|left, right| left.offset.total_cmp(&right.offset));
    stops
}

fn coordinate(raw: Option<&str>, fallback: f32) -> f32 {
    raw.and_then(parse_fraction).unwrap_or(fallback)
}

fn parse_fraction(raw: &str) -> Option<f32> {
    let raw = raw.trim();
    let (number, scale) = if let Some(percent) = raw.strip_suffix('%') {
        (percent.trim().parse::<f32>().ok()?, 0.01)
    } else {
        (raw.parse::<f32>().ok()?, 1.0)
    };
    let value = number * scale;
    value.is_finite().then_some(value)
}

fn lerp_channel(start: u8, end: u8, t: f32) -> u8 {
    let start = f32::from(start);
    let end = f32::from(end);
    (start + (end - start) * t.clamp(0.0, 1.0))
        .clamp(0.0, 255.0)
        .round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::parse_document;

    fn gradient(source: &str, id: &str) -> Option<GradientSpec> {
        let root = parse_document(source.as_bytes(), &crate::Limits::default()).expect("xml");
        let mut defs = HashMap::new();
        let mut stack = vec![&root];
        while let Some(element) = stack.pop() {
            if let Some(element_id) = element.attribute("id") {
                defs.insert(element_id.to_owned(), element);
            }
            for child in element.children.iter().rev() {
                if let XmlNode::Element(child) = child {
                    stack.push(child);
                }
            }
        }
        if !defs.contains_key(id) {
            return None;
        }
        GradientSpec::resolve(&defs, id)
    }

    #[test]
    fn test_linear_defaults_and_stops_sorted() {
        let source = "<svg><linearGradient id='g'><stop offset='100%' stop-color='#0000ff'/>\
                      <stop offset='0' stop-color='#ff0000'/></linearGradient></svg>";
        let spec = gradient(source, "g").expect("gradient");
        assert_eq!(
            spec.geometry,
            GradientGeometry::Linear {
                x1: 0.0,
                y1: 0.0,
                x2: 1.0,
                y2: 0.0
            }
        );
        assert_eq!(spec.stops[0].offset, 0.0);
        assert_eq!(spec.stops[1].offset, 1.0);
        assert_eq!(spec.evaluate(0.0), Color::rgba(255, 0, 0, 255));
        assert_eq!(spec.evaluate(1.0), Color::rgba(0, 0, 255, 255));
    }

    #[test]
    fn test_interpolation_and_stop_opacity() {
        let source = "<svg><linearGradient id='g'>\
                      <stop offset='0' stop-color='red' stop-opacity='0.5'/>\
                      <stop offset='1' stop-color='blue'/></linearGradient></svg>";
        let spec = gradient(source, "g").expect("gradient");
        let middle = spec.evaluate(0.5);
        assert!((middle.r as i32 - 128).abs() <= 2);
        assert!((middle.b as i32 - 128).abs() <= 2);
        assert!((middle.a as i32 - 191).abs() <= 2);
    }

    #[test]
    fn test_spread_methods_clamp_reflect_repeat() {
        let source = "<svg><linearGradient id='g' spreadMethod='reflect'>\
                      <stop offset='0' stop-color='black'/>\
                      <stop offset='1' stop-color='white'/></linearGradient></svg>";
        let spec = gradient(source, "g").expect("gradient");
        assert_eq!(spec.apply_spread(1.25), 0.75);
        assert_eq!(spec.apply_spread(-0.25), 0.25);
        let source = source.replace("reflect", "repeat");
        let repeat = gradient(&source, "g").expect("gradient");
        assert!((repeat.apply_spread(1.25) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn test_href_inheritance_and_cycles() {
        let source = "<svg>\
            <linearGradient id='base'><stop offset='0' stop-color='red'/>\
            <stop offset='1' stop-color='blue'/></linearGradient>\
            <linearGradient id='child' href='#base' y2='100%'/></svg>";
        let child = gradient(source, "child").expect("inherited gradient");
        assert_eq!(child.stops.len(), 2);
        assert_eq!(
            child.geometry,
            GradientGeometry::Linear {
                x1: 0.0,
                y1: 0.0,
                x2: 1.0,
                y2: 1.0
            }
        );
        let cycle =
            "<svg><linearGradient id='a' href='#b'/><linearGradient id='b' href='#a'/></svg>";
        assert!(gradient(cycle, "a").is_none());
    }

    #[test]
    fn test_radial_parameter_and_focus() {
        let source = "<svg><radialGradient id='g'><stop offset='0' stop-color='red'/>\
                      <stop offset='1' stop-color='blue'/></radialGradient></svg>";
        let spec = gradient(source, "g").expect("gradient");
        assert!((spec.parameter(0.5, 0.5) - 0.0).abs() < 1e-4);
        assert!((spec.parameter(1.0, 0.5) - 1.0).abs() < 1e-3);
        let far = spec.evaluate(1.5);
        assert_eq!(far, Color::rgba(0, 0, 255, 255));
    }
}
