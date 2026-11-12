//! CSS Animations timing engine.
//!
//! Evaluates the animation list of an element at a time `t` and applies the matching
//! `@keyframes` values by interpolating a clone of the base [`ComputedStyle`]. This is the
//! deterministic core used by the renderer/shell; it performs no I/O and uses no wall clock.
//!
//! Supported animatable properties: `opacity`, `color`, `background-color`, box lengths
//! (`width`/`height`/`min-*`/`max-*`/`top`/`right`/`bottom`/`left`, `margin-*`, `padding-*`,
//! `border-*-width`, `font-size`, `letter-spacing`, `word-spacing`). Other properties (and
//! mismatched value types such as `auto` ↔ `px`) are discrete: they switch at 50 % of the
//! interval, per CSS Animations §5.

use crate::animation::{
    AnimationDirection, AnimationSpec, FillMode, IterationCount, StepPosition, TimingFunction,
    TransitionProperty, TransitionSpec,
};
use crate::properties::{Color, ComputedStyle, Length};
use crate::rule::{Keyframe, KeyframesRule};

/// Maximum number of keyframe intervals/properties evaluated per animation application.
const MAX_KEYFRAME_STEPS: usize = 4096;

/// Evaluates a timing function at progress `x` in `0..=1`.
pub fn evaluate_timing_function(timing: &TimingFunction, x: f32) -> f32 {
    let x = if x.is_finite() {
        x.clamp(0.0, 1.0)
    } else {
        0.0
    };
    match timing {
        TimingFunction::Linear => x,
        TimingFunction::Ease => cubic_bezier(0.25, 0.1, 0.25, 1.0, x),
        TimingFunction::EaseIn => cubic_bezier(0.42, 0.0, 1.0, 1.0, x),
        TimingFunction::EaseOut => cubic_bezier(0.0, 0.0, 0.58, 1.0, x),
        TimingFunction::EaseInOut => cubic_bezier(0.42, 0.0, 0.58, 1.0, x),
        TimingFunction::CubicBezier(x1, y1, x2, y2) => cubic_bezier(*x1, *y1, *x2, *y2, x),
        TimingFunction::Steps(count, position) => steps(*count, *position, x),
    }
}

/// Solves the x component of a cubic Bézier with `P0=(0,0)`, `P3=(1,1)` by bisection.
fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    let bezier = |t: f32, p1: f32, p2: f32| {
        let mt = 1.0 - t;
        3.0 * mt * mt * t * p1 + 3.0 * mt * t * t * p2 + t * t * t
    };
    let mut low = 0.0f32;
    let mut high = 1.0f32;
    for _ in 0..32 {
        let middle = (low + high) * 0.5;
        if bezier(middle, x1, x2) < x {
            low = middle;
        } else {
            high = middle;
        }
    }
    bezier((low + high) * 0.5, y1, y2)
}

/// Evaluates a `steps()` function.
fn steps(count: u32, position: StepPosition, x: f32) -> f32 {
    let count = count.max(1) as f32;
    let scaled = x * count;
    let step = scaled.floor();
    let value = match position {
        StepPosition::Start => step + 1.0,
        StepPosition::End => step,
        StepPosition::JumpNone => {
            if count <= 1.0 {
                0.0
            } else {
                step.min(count - 1.0)
            }
        }
        StepPosition::JumpBoth => step + 1.0,
    };
    let divisor = match position {
        StepPosition::JumpNone => (count - 1.0).max(1.0),
        StepPosition::JumpBoth => count + 1.0,
        _ => count,
    };
    (value / divisor).clamp(0.0, 1.0)
}

/// Local progress (`0..=1`) of `spec` at `time_ms`, honoring delay, iterations, direction
/// and fill mode; `None` when the animation does not affect the element at that time.
pub fn animation_progress(spec: &AnimationSpec, time_ms: f32) -> Option<f32> {
    if !time_ms.is_finite() {
        return None;
    }
    let fill_before = matches!(spec.fill_mode, FillMode::Backwards | FillMode::Both);
    let fill_after = matches!(spec.fill_mode, FillMode::Forwards | FillMode::Both);
    if spec.duration_ms <= 0.0 {
        // Zero-duration animations apply their end state from the delay onward.
        return (time_ms >= spec.delay_ms).then_some(1.0);
    }
    let local = time_ms - spec.delay_ms;
    if local < 0.0 {
        return fill_before.then_some(0.0);
    }
    let iterations = match spec.iteration_count {
        IterationCount::Number(count) => count.max(0.0),
        IterationCount::Infinite => f32::INFINITY,
    };
    if iterations == 0.0 {
        return fill_after.then_some(1.0);
    }
    let total = iterations * spec.duration_ms;
    if total.is_finite() && local >= total {
        return fill_after.then_some(1.0);
    }
    let iteration = (local / spec.duration_ms).floor();
    let mut fraction = (local - iteration * spec.duration_ms) / spec.duration_ms;
    let alternate = matches!(
        spec.direction,
        AnimationDirection::Alternate | AnimationDirection::AlternateReverse
    );
    let reverse = matches!(spec.direction, AnimationDirection::Reverse)
        || (alternate && (iteration as u64) % 2 == 1);
    if reverse {
        fraction = 1.0 - fraction;
    }
    Some(fraction)
}

/// Applies every animation in `specs` to a clone of `base` at `time_ms`.
///
/// Animations are applied in list order (later ones override earlier ones for the same
/// property). Unknown keyframe names, missing keyframes and unsupported properties are
/// ignored; the returned style always has the base value for properties no animation touches.
pub fn apply_animations(
    base: &ComputedStyle,
    specs: &[AnimationSpec],
    rules: &[KeyframesRule],
    time_ms: f32,
) -> ComputedStyle {
    let mut style = base.clone();
    for spec in specs {
        if spec.play_state == crate::animation::PlayState::Paused {
            continue;
        }
        let Some(progress) = animation_progress(spec, time_ms) else {
            continue;
        };
        let Some(rule) = rules.iter().find(|rule| rule.name == spec.name) else {
            continue;
        };
        apply_keyframes(&mut style, base, rule, &spec.timing, progress);
    }
    style
}

/// One evaluated keyframe: normalized offset plus the properties it declares.
struct EvaluatedKeyframe {
    offset: f32,
    values: Vec<(String, String)>,
}

/// Interpolates `rule` into `style` at `progress` using `timing` easing.
fn apply_keyframes(
    style: &mut ComputedStyle,
    base: &ComputedStyle,
    rule: &KeyframesRule,
    timing: &TimingFunction,
    progress: f32,
) {
    let mut frames: Vec<EvaluatedKeyframe> = Vec::new();
    for Keyframe {
        offsets,
        declarations,
    } in &rule.frames
    {
        let expanded = crate::shorthand::expand_declarations(declarations);
        let mut values: Vec<(String, String)> = expanded
            .iter()
            .filter(|declaration| is_animatable(&declaration.name))
            .map(|declaration| (declaration.name.clone(), declaration.raw_value.clone()))
            .collect();
        values.sort_by(|a, b| a.0.cmp(&b.0));
        values.dedup_by(|a, b| a.0 == b.0);
        for offset in offsets {
            frames.push(EvaluatedKeyframe {
                offset: offset.clamp(0.0, 1.0),
                values: values.clone(),
            });
        }
    }
    if frames.is_empty() || frames.len() > MAX_KEYFRAME_STEPS {
        return;
    }
    frames.sort_by(|a, b| {
        a.offset
            .partial_cmp(&b.offset)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let previous = frames.iter().rev().find(|frame| frame.offset <= progress);
    let next = frames.iter().find(|frame| frame.offset > progress);

    match (previous, next) {
        (Some(previous), Some(next)) => {
            let span = (next.offset - previous.offset).max(f32::EPSILON);
            let local = ((progress - previous.offset) / span).clamp(0.0, 1.0);
            let eased = evaluate_timing_function(timing, local);
            for (property, previous_value) in &previous.values {
                let next_value = next
                    .values
                    .iter()
                    .find(|(name, _)| name == property)
                    .map(|(_, value)| value.as_str());
                interpolate_property(
                    style,
                    base,
                    property,
                    Some(previous_value),
                    next_value,
                    eased,
                    local,
                );
            }
            for (property, next_value) in &next.values {
                if previous.values.iter().any(|(name, _)| name == property) {
                    continue;
                }
                interpolate_property(style, base, property, None, Some(next_value), eased, local);
            }
        }
        (Some(previous), None) => {
            // After the last keyframe: blend towards the underlying value at 100 %.
            let span = (1.0 - previous.offset).max(f32::EPSILON);
            let local = ((progress - previous.offset) / span).clamp(0.0, 1.0);
            let eased = evaluate_timing_function(timing, local);
            for (property, value) in &previous.values {
                interpolate_property(style, base, property, Some(value), None, eased, local);
            }
        }
        (None, Some(next)) => {
            // Before the first keyframe: blend from the underlying value towards it.
            let span = next.offset.max(f32::EPSILON);
            let local = (progress / span).clamp(0.0, 1.0);
            let eased = evaluate_timing_function(timing, local);
            for (property, value) in &next.values {
                interpolate_property(style, base, property, None, Some(value), eased, local);
            }
        }
        (None, None) => {}
    }
}

/// Whether the property participates in the animation interpolation.
const ANIMATABLE_PROPERTIES: &[&str] = &[
    "opacity",
    "color",
    "background-color",
    "width",
    "height",
    "min-width",
    "max-width",
    "min-height",
    "max-height",
    "top",
    "right",
    "bottom",
    "left",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
    "padding-top",
    "padding-right",
    "padding-bottom",
    "padding-left",
    "border-top-width",
    "border-right-width",
    "border-bottom-width",
    "border-left-width",
    "font-size",
    "letter-spacing",
    "word-spacing",
];

/// Returns `true` when `property` is one of the supported animatable properties.
fn is_animatable(property: &str) -> bool {
    ANIMATABLE_PROPERTIES.contains(&property)
}

/// Evaluates a transition's eased progress (`0..=1`) at `elapsed_ms`, or `None` when the
/// transition is inactive (zero duration) or already finished (`elapsed >= delay + duration`).
pub fn transition_progress(spec: &TransitionSpec, elapsed_ms: f32) -> Option<(f32, f32)> {
    if !elapsed_ms.is_finite() || spec.duration_ms <= 0.0 || !spec.duration_ms.is_finite() {
        return None;
    }
    let local = elapsed_ms - spec.delay_ms;
    if local < 0.0 {
        return Some((0.0, 0.0));
    }
    if local >= spec.duration_ms {
        return None;
    }
    let linear = (local / spec.duration_ms).clamp(0.0, 1.0);
    Some((evaluate_timing_function(&spec.timing, linear), linear))
}

/// Interpolates CSS transitions between `from` and `to` at `elapsed_ms`.
pub fn apply_transitions(
    from: &ComputedStyle,
    to: &ComputedStyle,
    elapsed_ms: f32,
) -> ComputedStyle {
    let mut style = to.clone();
    if to.transitions.is_empty() || !elapsed_ms.is_finite() {
        return style;
    }
    for spec in &to.transitions {
        let Some((eased, local)) = transition_progress(spec, elapsed_ms) else {
            continue;
        };
        match &spec.property {
            TransitionProperty::None => continue,
            TransitionProperty::All => {
                for &prop in ANIMATABLE_PROPERTIES {
                    if let (Some(a), Some(b)) =
                        (base_animatable(prop, from), base_animatable(prop, to))
                        && a != b
                    {
                        interpolate_animatable(&mut style, prop, a, b, eased, local);
                    }
                }
            }
            TransitionProperty::Property(prop) => {
                if is_animatable(prop)
                    && let (Some(a), Some(b)) =
                        (base_animatable(prop, from), base_animatable(prop, to))
                    && a != b
                {
                    interpolate_animatable(&mut style, prop, a, b, eased, local);
                }
            }
        }
    }
    style
}

/// A value that can be interpolated for the supported properties.
#[derive(Clone, Copy, Debug, PartialEq)]
enum AnimatableValue {
    Number(f32),
    Length(Length),
    Color(Color),
}

/// Parses `raw` for `property` (using `base` for `em`/`rem` resolution).
fn parse_animatable(property: &str, raw: &str, base: &ComputedStyle) -> Option<AnimatableValue> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    match property {
        "opacity" => raw
            .parse::<f32>()
            .ok()
            .map(|value| AnimatableValue::Number(value.clamp(0.0, 1.0))),
        "color" | "background-color" => Color::parse(raw).map(AnimatableValue::Color),
        name if name.ends_with("-width") && !name.starts_with("border-") => {
            // `width`/`min-width`/... lengths.
            Length::parse(raw, base.font_size, 16.0).map(AnimatableValue::Length)
        }
        "border-top-width" | "border-right-width" | "border-bottom-width" | "border-left-width" => {
            Length::parse(raw, base.font_size, 16.0).and_then(|length| match length {
                Length::Px(px) => Some(AnimatableValue::Number(px)),
                _ => None,
            })
        }
        _ => Length::parse(raw, base.font_size, 16.0).map(AnimatableValue::Length),
    }
}

/// Underlying (base) value of a supported property.
fn base_animatable(property: &str, base: &ComputedStyle) -> Option<AnimatableValue> {
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
fn interpolate_property(
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

fn interpolate_animatable(
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

fn lerp_color(a: Color, b: Color, progress: f32) -> Color {
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

fn write_property(style: &mut ComputedStyle, property: &str, value: AnimatableValue) {
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
mod tests {
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
