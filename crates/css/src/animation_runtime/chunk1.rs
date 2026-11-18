//! Sized submodule of the parent facade (verbatim move).

use super::*;

/// Maximum number of keyframe intervals/properties evaluated per animation application.
pub(crate) const MAX_KEYFRAME_STEPS: usize = 4096;

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
pub(crate) fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
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
pub(crate) fn steps(count: u32, position: StepPosition, x: f32) -> f32 {
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
pub(crate) struct EvaluatedKeyframe {
    pub(crate) offset: f32,
    pub(crate) values: Vec<(String, String)>,
}

/// Interpolates `rule` into `style` at `progress` using `timing` easing.
pub(crate) fn apply_keyframes(
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
pub(crate) const ANIMATABLE_PROPERTIES: &[&str] = &[
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
pub(crate) fn is_animatable(property: &str) -> bool {
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
pub(crate) enum AnimatableValue {
    Number(f32),
    Length(Length),
    Color(Color),
}

/// Parses `raw` for `property` (using `base` for `em`/`rem` resolution).
pub(crate) fn parse_animatable(
    property: &str,
    raw: &str,
    base: &ComputedStyle,
) -> Option<AnimatableValue> {
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
