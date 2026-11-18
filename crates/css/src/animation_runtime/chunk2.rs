//! Sized submodule of the parent facade (verbatim move).

use super::*;

/// Underlying (base) value of a supported property.
pub(crate) fn base_animatable(property: &str, base: &ComputedStyle) -> Option<AnimatableValue> {
    Some(match property {
        "opacity" => AnimatableValue::Number(base.opacity),
        "color" => AnimatableValue::Color(base.color),
        "background-color" => AnimatableValue::Color(base.background_color),
        "width" => AnimatableValue::Length(base.width),
        "height" => AnimatableValue::Length(base.height),
        "min-width" => AnimatableValue::Length(base.min_width),
        "max-width" => AnimatableValue::Length(base.max_width),
        "min-height" => AnimatableValue::Length(base.min_height),
        "max-height" => AnimatableValue::Length(base.max_height),
        "top" => AnimatableValue::Length(base.top),
        "right" => AnimatableValue::Length(base.right),
        "bottom" => AnimatableValue::Length(base.bottom),
        "left" => AnimatableValue::Length(base.left),
        "margin-top" => AnimatableValue::Length(base.margin.top),
        "margin-right" => AnimatableValue::Length(base.margin.right),
        "margin-bottom" => AnimatableValue::Length(base.margin.bottom),
        "margin-left" => AnimatableValue::Length(base.margin.left),
        "padding-top" => AnimatableValue::Length(base.padding.top),
        "padding-right" => AnimatableValue::Length(base.padding.right),
        "padding-bottom" => AnimatableValue::Length(base.padding.bottom),
        "padding-left" => AnimatableValue::Length(base.padding.left),
        "border-top-width" => AnimatableValue::Number(base.border_width.top),
        "border-right-width" => AnimatableValue::Number(base.border_width.right),
        "border-bottom-width" => AnimatableValue::Number(base.border_width.bottom),
        "border-left-width" => AnimatableValue::Number(base.border_width.left),
        "font-size" => AnimatableValue::Length(Length::Px(base.font_size)),
        "letter-spacing" => AnimatableValue::Length(base.letter_spacing),
        "word-spacing" => AnimatableValue::Length(base.word_spacing),
        _ => return None,
    })
}

/// Interpolates one property into `style` (discrete at 50 % when the types do not match).
pub(crate) fn interpolate_property(
    style: &mut ComputedStyle,
    base: &ComputedStyle,
    property: &str,
    previous: Option<&str>,
    next: Option<&str>,
    eased: f32,
    local: f32,
) {
    let underlying = base_animatable(property, base);
    let from = previous
        .and_then(|raw| parse_animatable(property, raw, base))
        .or(underlying);
    let to = next
        .and_then(|raw| parse_animatable(property, raw, base))
        .or(underlying);
    let (Some(from), Some(to)) = (from, to) else {
        return;
    };
    interpolate_animatable(style, property, from, to, eased, local);
}

pub(crate) fn interpolate_animatable(
    style: &mut ComputedStyle,
    property: &str,
    from: AnimatableValue,
    to: AnimatableValue,
    eased: f32,
    local: f32,
) {
    let value = match (from, to) {
        (AnimatableValue::Number(a), AnimatableValue::Number(b)) => {
            AnimatableValue::Number(a + (b - a) * eased)
        }
        (AnimatableValue::Color(a), AnimatableValue::Color(b)) => {
            AnimatableValue::Color(lerp_color(a, b, eased))
        }
        (AnimatableValue::Length(a), AnimatableValue::Length(b)) => match (a, b) {
            (Length::Px(a), Length::Px(b)) => {
                AnimatableValue::Length(Length::Px(a + (b - a) * eased))
            }
            (Length::Percent(a), Length::Percent(b)) => {
                AnimatableValue::Length(Length::Percent(a + (b - a) * eased))
            }
            // Discrete fallback for non-interpolable pairs (`auto` ↔ length, px ↔ %).
            _ => {
                if local < 0.5 {
                    AnimatableValue::Length(a)
                } else {
                    AnimatableValue::Length(b)
                }
            }
        },
        (a, _) => {
            if local < 0.5 {
                a
            } else {
                to
            }
        }
    };
    write_property(style, property, value);
}

pub(crate) fn lerp_color(a: Color, b: Color, progress: f32) -> Color {
    let lerp = |start: u8, end: u8| {
        let value = start as f32 + (end as f32 - start as f32) * progress;
        value.round().clamp(0.0, 255.0) as u8
    };
    Color {
        r: lerp(a.r, b.r),
        g: lerp(a.g, b.g),
        b: lerp(a.b, b.b),
        a: lerp(a.a, b.a),
    }
}

pub(crate) fn write_property(style: &mut ComputedStyle, property: &str, value: AnimatableValue) {
    match (property, value) {
        ("opacity", AnimatableValue::Number(number)) => style.opacity = number.clamp(0.0, 1.0),
        ("color", AnimatableValue::Color(color)) => style.color = color,
        ("background-color", AnimatableValue::Color(color)) => style.background_color = color,
        ("width", AnimatableValue::Length(length)) => style.width = length,
        ("height", AnimatableValue::Length(length)) => style.height = length,
        ("min-width", AnimatableValue::Length(length)) => style.min_width = length,
        ("max-width", AnimatableValue::Length(length)) => style.max_width = length,
        ("min-height", AnimatableValue::Length(length)) => style.min_height = length,
        ("max-height", AnimatableValue::Length(length)) => style.max_height = length,
        ("top", AnimatableValue::Length(length)) => style.top = length,
        ("right", AnimatableValue::Length(length)) => style.right = length,
        ("bottom", AnimatableValue::Length(length)) => style.bottom = length,
        ("left", AnimatableValue::Length(length)) => style.left = length,
        ("margin-top", AnimatableValue::Length(length)) => style.margin.top = length,
        ("margin-right", AnimatableValue::Length(length)) => style.margin.right = length,
        ("margin-bottom", AnimatableValue::Length(length)) => style.margin.bottom = length,
        ("margin-left", AnimatableValue::Length(length)) => style.margin.left = length,
        ("padding-top", AnimatableValue::Length(length)) => style.padding.top = length,
        ("padding-right", AnimatableValue::Length(length)) => style.padding.right = length,
        ("padding-bottom", AnimatableValue::Length(length)) => style.padding.bottom = length,
        ("padding-left", AnimatableValue::Length(length)) => style.padding.left = length,
        ("border-top-width", AnimatableValue::Number(width)) => {
            style.border_width.top = width.max(0.0)
        }
        ("border-right-width", AnimatableValue::Number(width)) => {
            style.border_width.right = width.max(0.0)
        }
        ("border-bottom-width", AnimatableValue::Number(width)) => {
            style.border_width.bottom = width.max(0.0)
        }
        ("border-left-width", AnimatableValue::Number(width)) => {
            style.border_width.left = width.max(0.0)
        }
        ("font-size", AnimatableValue::Length(Length::Px(px))) => style.font_size = px.max(1.0),
        ("letter-spacing", AnimatableValue::Length(length)) => style.letter_spacing = length,
        ("word-spacing", AnimatableValue::Length(length)) => style.word_spacing = length,
        _ => {}
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::animation::{
        AnimationDirection, AnimationSpec, FillMode, IterationCount, TimingFunction,
    };
    use crate::parser::parse_stylesheet;

    fn keyframes(source: &str) -> Vec<KeyframesRule> {
        parse_stylesheet(source).keyframes
    }

    fn animation(name: &str, duration_ms: f32) -> AnimationSpec {
        AnimationSpec {
            name: name.to_string(),
            duration_ms,
            timing: TimingFunction::Linear,
            delay_ms: 0.0,
            iteration_count: IterationCount::Number(1.0),
            direction: AnimationDirection::Normal,
            fill_mode: FillMode::None,
            play_state: crate::animation::PlayState::Running,
        }
    }

    #[test]
    fn timing_curves_evaluate() {
        assert!((evaluate_timing_function(&TimingFunction::Linear, 0.25) - 0.25).abs() < 1e-6);
        assert_eq!(evaluate_timing_function(&TimingFunction::Linear, -1.0), 0.0);
        assert_eq!(evaluate_timing_function(&TimingFunction::Linear, 2.0), 1.0);
        let linear_bezier = TimingFunction::CubicBezier(0.0, 0.0, 1.0, 1.0);
        assert!((evaluate_timing_function(&linear_bezier, 0.4) - 0.4).abs() < 1e-3);
        assert!((evaluate_timing_function(&TimingFunction::Ease, 0.0) - 0.0).abs() < 1e-6);
        assert!((evaluate_timing_function(&TimingFunction::Ease, 1.0) - 1.0).abs() < 1e-6);
        assert_eq!(
            evaluate_timing_function(&TimingFunction::Steps(2, StepPosition::End), 0.25),
            0.0
        );
        assert_eq!(
            evaluate_timing_function(&TimingFunction::Steps(2, StepPosition::End), 0.75),
            0.5
        );
        assert_eq!(
            evaluate_timing_function(&TimingFunction::Steps(1, StepPosition::Start), 0.0),
            1.0
        );
    }

    #[test]
    fn progress_honors_delay_iterations_direction_and_fill() {
        let mut spec = animation("fade", 1000.0);
        assert_eq!(animation_progress(&spec, 250.0), Some(0.25));
        assert_eq!(
            animation_progress(&spec, 1000.0),
            None,
            "ended without fill"
        );
        spec.fill_mode = FillMode::Forwards;
        assert_eq!(animation_progress(&spec, 2000.0), Some(1.0));

        let mut delayed = animation("fade", 1000.0);
        delayed.delay_ms = 500.0;
        assert_eq!(animation_progress(&delayed, 250.0), None);
        delayed.fill_mode = FillMode::Backwards;
        assert_eq!(animation_progress(&delayed, 250.0), Some(0.0));
        assert_eq!(animation_progress(&delayed, 1000.0), Some(0.5));

        let mut loops = animation("fade", 1000.0);
        loops.iteration_count = IterationCount::Number(2.0);
        assert_eq!(animation_progress(&loops, 1500.0), Some(0.5));

        let mut reverse = animation("fade", 1000.0);
        reverse.direction = AnimationDirection::Reverse;
        assert_eq!(animation_progress(&reverse, 250.0), Some(0.75));

        let mut alternate = animation("fade", 1000.0);
        alternate.direction = AnimationDirection::Alternate;
        alternate.iteration_count = IterationCount::Number(2.0);
        assert_eq!(animation_progress(&alternate, 250.0), Some(0.25));
        assert_eq!(animation_progress(&alternate, 1250.0), Some(0.75));

        let mut infinite = animation("fade", 1000.0);
        infinite.iteration_count = IterationCount::Infinite;
        assert_eq!(animation_progress(&infinite, 2500.0), Some(0.5));
    }

    #[test]
    fn keyframes_interpolate_opacity_color_and_lengths() {
        let rules = keyframes(
            "@keyframes fade { from { opacity: 0; color: rgb(0, 0, 0); width: 100px } \
             to { opacity: 1; color: rgb(255, 0, 0); width: 200px } }",
        );
        let base = ComputedStyle::initial();
        let spec = animation("fade", 1000.0);
        let half = apply_animations(&base, std::slice::from_ref(&spec), &rules, 500.0);
        assert!((half.opacity - 0.5).abs() < 1e-6);
        assert_eq!(half.width, Length::Px(150.0));
        assert_eq!(half.color.r, 128);
        let start = apply_animations(&base, std::slice::from_ref(&spec), &rules, 0.0);
        assert_eq!(start.opacity, 0.0);
        assert_eq!(start.width, Length::Px(100.0));
    }

    #[test]
    fn implicit_keyframes_blend_with_the_underlying_style() {
        let rules = keyframes("@keyframes half { 50% { opacity: 0 } }");
        let base = ComputedStyle::initial();
        let spec = animation("half", 1000.0);
        let quarter = apply_animations(&base, std::slice::from_ref(&spec), &rules, 250.0);
        assert!((quarter.opacity - 0.5).abs() < 1e-6);
        let three_quarters = apply_animations(&base, std::slice::from_ref(&spec), &rules, 750.0);
        assert!((three_quarters.opacity - 0.5).abs() < 1e-6);
    }

    #[test]
    fn mismatched_lengths_are_discrete() {
        let rules = keyframes("@keyframes grow { from { width: auto } to { width: 100px } }");
        let base = ComputedStyle::initial();
        let spec = animation("grow", 1000.0);
        let before = apply_animations(&base, std::slice::from_ref(&spec), &rules, 250.0);
        assert_eq!(before.width, Length::Auto);
        let after = apply_animations(&base, std::slice::from_ref(&spec), &rules, 750.0);
        assert_eq!(after.width, Length::Px(100.0));
    }

    #[test]
    fn paused_and_unknown_animations_leave_the_base_style() {
        let rules = keyframes("@keyframes fade { from { opacity: 0 } to { opacity: 1 } }");
        let base = ComputedStyle::initial();
        let mut paused = animation("fade", 1000.0);
        paused.play_state = crate::animation::PlayState::Paused;
        let style = apply_animations(&base, &[paused], &rules, 500.0);
        assert_eq!(style.opacity, 1.0);
        let unknown = animation("missing", 1000.0);
        let style = apply_animations(&base, &[unknown], &rules, 500.0);
        assert_eq!(style.opacity, 1.0);
    }

    #[test]
    fn transitions_interpolate_explicit_and_all_properties() {
        let mut from = ComputedStyle::initial();
        from.opacity = 0.0;
        from.width = Length::Px(100.0);

        let mut to = ComputedStyle::initial();
        to.opacity = 1.0;
        to.width = Length::Px(200.0);
        to.transitions = vec![TransitionSpec {
            property: TransitionProperty::All,
            duration_ms: 1000.0,
            delay_ms: 0.0,
            timing: TimingFunction::Linear,
        }];

        let half = apply_transitions(&from, &to, 500.0);
        assert!((half.opacity - 0.5).abs() < 1e-4);
        assert_eq!(half.width, Length::Px(150.0));
    }

    #[test]
    fn transitions_honor_delay_and_completion() {
        let mut from = ComputedStyle::initial();
        from.opacity = 0.0;
        let mut to = ComputedStyle::initial();
        to.opacity = 1.0;
        to.transitions = vec![TransitionSpec {
            property: TransitionProperty::Property("opacity".to_owned()),
            duration_ms: 1000.0,
            delay_ms: 200.0,
            timing: TimingFunction::Linear,
        }];

        let before = apply_transitions(&from, &to, 100.0);
        assert!((before.opacity - 0.0).abs() < 1e-4);
        let mid = apply_transitions(&from, &to, 700.0);
        assert!((mid.opacity - 0.5).abs() < 1e-4);
        let done = apply_transitions(&from, &to, 1200.0);
        assert!((done.opacity - 1.0).abs() < 1e-4);
    }
}
