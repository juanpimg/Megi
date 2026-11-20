//! CSS Transforms Level 1 §3 — `transform-origin` parsing (#1934).
//!
//! The origin is stored as two `<length-percentage>` components relative to the
//! element's **border box** (the CSS reference box for HTML elements). Keywords
//! map to percentages per spec (`left`/`top` = 0%, `center` = 50%,
//! `right`/`bottom` = 100%). The initial value is `50% 50%`.
//!
//! Layout resolves the components against the border-box size and composes the
//! used transform as `T(origin) · M · T(-origin)`.

use crate::Length;

/// The origin of an element's transform, as offsets from the border box's
/// top-left corner. Both components are `Px` or `Percent` (`Auto` is rejected).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransformOrigin {
    /// Horizontal offset from the left border edge.
    pub x: Length,
    /// Vertical offset from the top border edge.
    pub y: Length,
}

impl TransformOrigin {
    /// CSS Transforms 1 §3 initial value: the centre of the reference box.
    pub const INITIAL: Self = Self {
        x: Length::Percent(50.0),
        y: Length::Percent(50.0),
    };

    /// Parses one or two whitespace-separated components. With a single
    /// component the other axis defaults to `center` (50%), per spec.
    pub fn parse(value: &str, font_size: f32, root_font_size: f32) -> Option<Self> {
        let mut tokens = value.split_ascii_whitespace();
        let x = parse_component(tokens.next()?, font_size, root_font_size)?;
        let y = match tokens.next() {
            Some(token) => parse_component(token, font_size, root_font_size)?,
            None => Length::Percent(50.0),
        };
        if tokens.next().is_some() {
            return None; // transform-origin takes at most two components
        }
        Some(Self { x, y })
    }

    /// Resolves the origin to pixel offsets from the border box's top-left
    /// corner. Percentages are relative to `width`/`height` (CSS Transforms 1 §3).
    pub fn resolve(&self, width: f32, height: f32) -> (f32, f32) {
        (
            resolve_component(self.x, width),
            resolve_component(self.y, height),
        )
    }
}

/// Maps a single `<length-percentage>` or positional keyword to a `Length`.
fn parse_component(token: &str, font_size: f32, root_font_size: f32) -> Option<Length> {
    match token.to_ascii_lowercase().as_str() {
        "left" | "top" => Some(Length::Percent(0.0)),
        "center" => Some(Length::Percent(50.0)),
        "right" | "bottom" => Some(Length::Percent(100.0)),
        other => {
            let length = Length::parse(other, font_size, root_font_size)?;
            match length {
                Length::Px(_) | Length::Percent(_) => Some(length),
                Length::Auto => None,
            }
        }
    }
}

fn resolve_component(value: Length, basis: f32) -> f32 {
    match value {
        Length::Px(px) => px,
        Length::Percent(pct) => basis * pct / 100.0,
        Length::Auto => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keywords_and_percentages_are_equivalent() {
        let keyword = TransformOrigin::parse("left top", 16.0, 16.0).expect("keyword");
        let percent = TransformOrigin::parse("0% 0%", 16.0, 16.0).expect("percent");
        assert_eq!(keyword.resolve(60.0, 20.0), percent.resolve(60.0, 20.0));
        let centre = TransformOrigin::parse("center", 16.0, 16.0).expect("centre");
        assert_eq!(centre.resolve(60.0, 20.0), (30.0, 10.0));
    }

    #[test]
    fn too_many_components_are_rejected() {
        assert!(TransformOrigin::parse("1px 2px 3px", 16.0, 16.0).is_none());
        assert!(TransformOrigin::parse("bogus", 16.0, 16.0).is_none());
    }
}
