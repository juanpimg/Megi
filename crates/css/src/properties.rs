//! # CSS Properties & Computed Style Engine
//!
//! Normative computation of property values per CSS Cascading and Inheritance Level 4,
//! CSS Values and Units Level 4, and CSS Box Model Level 3.
//!
//! The implementation lives in cohesive submodules (`values`, `computed`, `parse` and
//! `calc`); this file keeps the module's public surface and the in-file unit tests.

mod calc;
mod computed;
mod parse;
mod values;

pub use computed::ComputedStyle;
pub use parse::{MAX_BOX_SHADOWS, MAX_TEXT_SHADOWS};
pub use values::{
    AspectRatio, BackgroundAttachment, BackgroundClip, BackgroundLayer, BackgroundOrigin,
    BackgroundPosition, BackgroundRepeat, BackgroundRepeatAxis, BackgroundSize, BorderCollapse,
    BorderRadii, BorderStyle, BoxShadow, BoxSizing, CalcLengths, CaptionSide, Clear, Color,
    ConicGradient, ContainerType, Content, ContentItem, Direction, Display, EmptyCells,
    FilterFunction, Float, FontStyle, Gradient, GradientKind, GradientStop, InterpolationSpace,
    Length, LinearDirection, LinearGradient, ListStyleType, MAX_FILTER_FUNCTIONS,
    MAX_GRADIENT_STOPS, MAX_GRADIENTS, MixBlendMode, ObjectFit, Overflow, Position, RadialExtent,
    RadialGradient, RadialShape, Radius, RectEdges, TableLayout, TextAlign, TextDecoration,
    TextOverflow, TextShadow, TextTransform, UnicodeBidi, VerticalAlign, Visibility, WhiteSpace,
};

// Crate-internal helpers other modules rely on.
pub(crate) use computed::find_declaration;
pub(crate) use parse::{
    parse_background_shorthand_layers, parse_border_width, split_border_radius_value,
    split_components, split_top_level_commas,
};

// Imports used only by the unit tests at the bottom of this file.
#[cfg(test)]
use crate::rule::Declaration;
#[cfg(test)]
pub(crate) use calc::expand_calc_four;
#[cfg(test)]
pub(crate) use parse::{parse_angle_degrees, parse_number_or_percent};

#[cfg(test)]
mod unicode_boundary_tests {
    use super::*;

    #[test]
    fn expand_calc_four_returns_none_for_empty_input() {
        assert_eq!(expand_calc_four::<i32>(&[]), None);
        assert_eq!(expand_calc_four(&[1]), Some([1, 1, 1, 1]));
        assert_eq!(expand_calc_four(&[1, 2]), Some([1, 2, 1, 2]));
        assert_eq!(expand_calc_four(&[1, 2, 3]), Some([1, 2, 3, 2]));
        assert_eq!(expand_calc_four(&[1, 2, 3, 4]), Some([1, 2, 3, 4]));
    }

    #[test]
    fn color_parse_does_not_panic_on_non_ascii() {
        // "aééx" has byte 4 (and 5) inside a multi-byte code point; the old byte
        // slices panicked before the ASCII prefix comparison ran.
        assert_eq!(Color::parse("aééx"), None);
        assert_eq!(Color::parse("rgbé(1,2,3)"), None);
        assert_eq!(Color::parse("rgbaé(1,2,3,1)"), None);
        assert_eq!(Color::parse("rgb(1,2,3)"), Some(Color::new(1, 2, 3, 255)));
    }

    #[test]
    fn non_finite_numeric_values_are_rejected() {
        assert_eq!(parse_angle_degrees("1e400deg"), None);
        assert_eq!(parse_angle_degrees("1e400turn"), None);
        assert_eq!(parse_angle_degrees("90deg"), Some(90.0));
        assert_eq!(parse_number_or_percent("1e400"), None);
        assert_eq!(parse_number_or_percent("1e400%"), None);
        assert_eq!(parse_number_or_percent("50%"), Some(0.5));
    }

    #[test]
    fn background_position_rejects_two_keywords_on_one_axis() {
        assert_eq!(BackgroundPosition::parse("left right", 16.0, 16.0), None);
        assert_eq!(BackgroundPosition::parse("top bottom", 16.0, 16.0), None);
        assert_eq!(BackgroundPosition::parse("left left", 16.0, 16.0), None);
        assert_eq!(BackgroundPosition::parse("center left", 16.0, 16.0), None);
        assert!(BackgroundPosition::parse("left center", 16.0, 16.0).is_some());
        assert!(BackgroundPosition::parse("center center", 16.0, 16.0).is_some());
        assert!(BackgroundPosition::parse("left 10px", 16.0, 16.0).is_some());
    }
}

#[cfg(test)]
mod animation_property_tests {
    use super::*;
    use crate::parser::parse_list_of_declarations;
    use std::collections::HashMap;

    fn compute_inline(source: &str) -> ComputedStyle {
        let declarations = parse_list_of_declarations(source);
        let mut cascaded: HashMap<String, &Declaration> = HashMap::new();
        for declaration in &declarations {
            cascaded.insert(declaration.name.clone(), declaration);
        }
        ComputedStyle::compute(&cascaded, None, None)
    }

    #[test]
    fn animation_and_transition_longhands_reach_the_computed_style() {
        let style = compute_inline(
            "animation-name: fade, slide; animation-duration: 2s; \
             animation-delay: 100ms; animation-iteration-count: infinite; \
             animation-direction: alternate; animation-timing-function: linear; \
             transition-property: opacity, transform; transition-duration: 300ms; \
             transition-timing-function: ease-out; transition-delay: 50ms;",
        );
        assert_eq!(style.animations.len(), 2);
        assert_eq!(style.animations[0].name, "fade");
        assert_eq!(style.animations[1].name, "slide");
        assert_eq!(style.animations[0].duration_ms, 2000.0);
        assert_eq!(style.animations[1].duration_ms, 2000.0, "lists cycle");
        assert_eq!(style.animations[0].delay_ms, 100.0);
        assert_eq!(
            style.animations[0].iteration_count,
            crate::animation::IterationCount::Infinite
        );
        assert_eq!(
            style.animations[0].direction,
            crate::animation::AnimationDirection::Alternate
        );
        assert_eq!(
            style.animations[0].timing,
            crate::animation::TimingFunction::Linear
        );
        assert_eq!(style.transitions.len(), 2);
        assert_eq!(style.transitions[0].duration_ms, 300.0);
        assert_eq!(style.transitions[0].delay_ms, 50.0);
        assert_eq!(
            style.transitions[0].property,
            crate::animation::TransitionProperty::Property("opacity".into())
        );
    }

    #[test]
    fn animation_properties_are_not_inherited() {
        let declarations =
            parse_list_of_declarations("animation-name: fade; animation-duration: 1s;");
        let mut cascaded: HashMap<String, &Declaration> = HashMap::new();
        for declaration in &declarations {
            cascaded.insert(declaration.name.clone(), declaration);
        }
        let parent = ComputedStyle::compute(&cascaded, None, None);
        let child = ComputedStyle::compute(&HashMap::new(), Some(&parent), None);
        assert_eq!(parent.animations.len(), 1);
        assert!(
            child.animations.is_empty(),
            "CSS Animations properties are not inherited"
        );
    }
}
