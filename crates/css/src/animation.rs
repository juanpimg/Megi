//! CSS Animations 1 and CSS Transitions 1 longhand values.
//!
//! This module only parses and normalizes the longhand declarations
//! (`animation-name/-duration/-timing-function/-delay/-iteration-count/-direction/
//! -fill-mode/-play-state` and `transition-property/-duration/-timing-function/-delay`).
//! Resolving `@keyframes` (already available in [`crate::rule::Stylesheet::keyframes`]) and
//! applying the timeline is the render layer's job.

use crate::rule::Declaration;

/// Maximum accepted duration/delay (10,000 s) to keep frame math finite.
pub const MAX_TIME_MS: f32 = 10_000_000.0;

/// Maximum accepted `animation-iteration-count`.
pub const MAX_ITERATIONS: f32 = 1_000_000.0;

/// `animation-timing-function` / `transition-timing-function` value.
#[derive(Clone, Debug, PartialEq)]
pub enum TimingFunction {
    /// `linear`
    Linear,
    /// `ease` (the initial value).
    Ease,
    /// `ease-in`
    EaseIn,
    /// `ease-out`
    EaseOut,
    /// `ease-in-out`
    EaseInOut,
    /// `cubic-bezier(x1, y1, x2, y2)` with `x1`/`x2` in `0..=1`.
    CubicBezier(f32, f32, f32, f32),
    /// `steps(n, <position>)`.
    Steps(u32, StepPosition),
}

/// Step position of `steps()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepPosition {
    /// `jump-start` / `start`
    Start,
    /// `jump-end` / `end` (initial)
    End,
    /// `jump-none`
    JumpNone,
    /// `jump-both`
    JumpBoth,
}

/// `animation-direction`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationDirection {
    /// `normal` (initial)
    Normal,
    /// `reverse`
    Reverse,
    /// `alternate`
    Alternate,
    /// `alternate-reverse`
    AlternateReverse,
}

/// `animation-fill-mode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillMode {
    /// `none` (initial)
    None,
    /// `forwards`
    Forwards,
    /// `backwards`
    Backwards,
    /// `both`
    Both,
}

/// `animation-play-state`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayState {
    /// `running` (initial)
    Running,
    /// `paused`
    Paused,
}

/// `animation-iteration-count` value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IterationCount {
    /// A finite number of iterations.
    Number(f32),
    /// `infinite`
    Infinite,
}

/// One animation described by the longhand property lists (matched by index, cycling).
#[derive(Clone, Debug, PartialEq)]
pub struct AnimationSpec {
    /// `animation-name` (case-sensitive, as authored).
    pub name: String,
    /// `animation-duration` in milliseconds (initial `0`).
    pub duration_ms: f32,
    /// Timing function (initial `ease`).
    pub timing: TimingFunction,
    /// `animation-delay` in milliseconds (may be negative).
    pub delay_ms: f32,
    /// Iteration count (initial `1`).
    pub iteration_count: IterationCount,
    /// Direction (initial `normal`).
    pub direction: AnimationDirection,
    /// Fill mode (initial `none`).
    pub fill_mode: FillMode,
    /// Play state (initial `running`).
    pub play_state: PlayState,
}

/// `transition-property` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransitionProperty {
    /// `all`
    All,
    /// `none` (no transitions).
    None,
    /// One longhand property name (as authored, lowercased by the parser).
    Property(String),
}

/// One transition described by the longhand property lists (matched by index, cycling).
#[derive(Clone, Debug, PartialEq)]
pub struct TransitionSpec {
    /// Property this transition applies to.
    pub property: TransitionProperty,
    /// Duration in milliseconds (initial `0`).
    pub duration_ms: f32,
    /// Timing function (initial `ease`).
    pub timing: TimingFunction,
    /// Delay in milliseconds (may be negative).
    pub delay_ms: f32,
}

/// Parses a `<time>` (`0`, `300ms`, `0.3s`); returns milliseconds.
pub fn parse_time_ms(value: &str) -> Option<f32> {
    let value = value.trim().to_ascii_lowercase();
    let time = if let Some(number) = value.strip_suffix("ms") {
        number.trim().parse::<f32>().ok()?
    } else if let Some(number) = value.strip_suffix('s') {
        number.trim().parse::<f32>().ok()? * 1000.0
    } else if value == "0" || value == "0.0" {
        0.0
    } else {
        return None;
    };
    time.is_finite().then_some(time)
}

/// Parses a `<timing-function>`.
pub fn parse_timing_function(value: &str) -> Option<TimingFunction> {
    let value = value.trim();
    match value.to_ascii_lowercase().as_str() {
        "linear" => return Some(TimingFunction::Linear),
        "ease" => return Some(TimingFunction::Ease),
        "ease-in" => return Some(TimingFunction::EaseIn),
        "ease-out" => return Some(TimingFunction::EaseOut),
        "ease-in-out" => return Some(TimingFunction::EaseInOut),
        "step-start" => return Some(TimingFunction::Steps(1, StepPosition::Start)),
        "step-end" => return Some(TimingFunction::Steps(1, StepPosition::End)),
        _ => {}
    }
    if let Some(args) = function_args(value, "cubic-bezier") {
        let numbers: Vec<f32> = args
            .split([',', ' ', '\t'])
            .filter(|part| !part.trim().is_empty())
            .filter_map(|part| part.trim().parse::<f32>().ok())
            .filter(|number| number.is_finite())
            .collect();
        if numbers.len() == 4
            && (0.0..=1.0).contains(&numbers[0])
            && (0.0..=1.0).contains(&numbers[2])
        {
            return Some(TimingFunction::CubicBezier(
                numbers[0], numbers[1], numbers[2], numbers[3],
            ));
        }
        return None;
    }
    if let Some(args) = function_args(value, "steps") {
        let mut parts = args
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty());
        let count: u32 = parts.next()?.parse().ok()?;
        if !(1..=10_000).contains(&count) {
            return None;
        }
        let position = match parts.next().map(str::to_ascii_lowercase).as_deref() {
            None | Some("end") | Some("jump-end") => StepPosition::End,
            Some("start") | Some("jump-start") => StepPosition::Start,
            Some("jump-none") => StepPosition::JumpNone,
            Some("jump-both") => StepPosition::JumpBoth,
            Some(_) => return None,
        };
        if parts.next().is_some() {
            return None;
        }
        return Some(TimingFunction::Steps(count, position));
    }
    None
}

/// Parses `animation-iteration-count`.
pub fn parse_iteration_count(value: &str) -> Option<IterationCount> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("infinite") {
        return Some(IterationCount::Infinite);
    }
    let count = value.parse::<f32>().ok()?;
    (count.is_finite() && count >= 0.0).then(|| IterationCount::Number(count.min(MAX_ITERATIONS)))
}

/// Parses `animation-direction`.
pub fn parse_direction(value: &str) -> Option<AnimationDirection> {
    Some(match value.trim().to_ascii_lowercase().as_str() {
        "normal" => AnimationDirection::Normal,
        "reverse" => AnimationDirection::Reverse,
        "alternate" => AnimationDirection::Alternate,
        "alternate-reverse" => AnimationDirection::AlternateReverse,
        _ => return None,
    })
}

/// Parses `animation-fill-mode`.
pub fn parse_fill_mode(value: &str) -> Option<FillMode> {
    Some(match value.trim().to_ascii_lowercase().as_str() {
        "none" => FillMode::None,
        "forwards" => FillMode::Forwards,
        "backwards" => FillMode::Backwards,
        "both" => FillMode::Both,
        _ => return None,
    })
}

/// Parses `animation-play-state`.
pub fn parse_play_state(value: &str) -> Option<PlayState> {
    Some(match value.trim().to_ascii_lowercase().as_str() {
        "running" => PlayState::Running,
        "paused" => PlayState::Paused,
        _ => return None,
    })
}

/// Parses a `transition-property` entry.
pub fn parse_transition_property(value: &str) -> Option<TransitionProperty> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("all") {
        return Some(TransitionProperty::All);
    }
    if value.eq_ignore_ascii_case("none") {
        return Some(TransitionProperty::None);
    }
    (!value.is_empty()).then(|| TransitionProperty::Property(value.to_ascii_lowercase()))
}

/// Extracts the arguments of `name(...)` (case-insensitive) when `value` is exactly that call.
fn function_args<'a>(value: &'a str, name: &str) -> Option<&'a str> {
    let value = value.trim();
    if value.len() <= name.len() + 1 || !value.is_char_boundary(name.len() + 1) {
        return None;
    }
    if !value[..name.len()].eq_ignore_ascii_case(name) || !value.ends_with(')') {
        return None;
    }
    if value.as_bytes().get(name.len()) != Some(&b'(') {
        return None;
    }
    Some(&value[name.len() + 1..value.len() - 1])
}

/// Splits a comma-separated list at top level (commas inside functions do not split).
fn split_top_level_commas(value: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (index, ch) in value.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&value[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(&value[start..]);
    parts
}

fn declaration_value<'a>(entries: &[(&str, &'a Declaration)], name: &str) -> Option<&'a str> {
    entries
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, declaration)| declaration.raw_value.trim())
        .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("initial"))
}

/// Value of the `name` list at `index`, cycling the list as CSS requires.
fn list_item<'a>(entries: &[(&str, &'a Declaration)], name: &str, index: usize) -> Option<&'a str> {
    let raw = declaration_value(entries, name)?;
    let items = split_top_level_commas(raw);
    if items.is_empty() {
        return None;
    }
    Some(items[index % items.len()].trim())
}

/// Builds the animation list from the longhand declarations.
pub(crate) fn parse_animations(entries: &[(&str, &Declaration)]) -> Vec<AnimationSpec> {
    let Some(raw_names) = declaration_value(entries, "animation-name") else {
        return Vec::new();
    };
    let mut animations = Vec::new();
    for (index, raw_name) in split_top_level_commas(raw_names).iter().enumerate() {
        let name = raw_name.trim();
        if name.is_empty()
            || name.eq_ignore_ascii_case("none")
            || name.eq_ignore_ascii_case("initial")
        {
            continue;
        }
        let duration_ms = list_item(entries, "animation-duration", index)
            .and_then(parse_time_ms)
            .map_or(0.0, |ms| ms.clamp(0.0, MAX_TIME_MS));
        let delay_ms = list_item(entries, "animation-delay", index)
            .and_then(parse_time_ms)
            .map_or(0.0, |ms| ms.clamp(-MAX_TIME_MS, MAX_TIME_MS));
        let timing = list_item(entries, "animation-timing-function", index)
            .and_then(parse_timing_function)
            .unwrap_or(TimingFunction::Ease);
        let iteration_count = list_item(entries, "animation-iteration-count", index)
            .and_then(parse_iteration_count)
            .unwrap_or(IterationCount::Number(1.0));
        let direction = list_item(entries, "animation-direction", index)
            .and_then(parse_direction)
            .unwrap_or(AnimationDirection::Normal);
        let fill_mode = list_item(entries, "animation-fill-mode", index)
            .and_then(parse_fill_mode)
            .unwrap_or(FillMode::None);
        let play_state = list_item(entries, "animation-play-state", index)
            .and_then(parse_play_state)
            .unwrap_or(PlayState::Running);
        animations.push(AnimationSpec {
            name: name.to_string(),
            duration_ms,
            timing,
            delay_ms,
            iteration_count,
            direction,
            fill_mode,
            play_state,
        });
    }
    animations
}

/// Builds the transition list from the longhand declarations (last entry per property wins).
pub(crate) fn parse_transitions(entries: &[(&str, &Declaration)]) -> Vec<TransitionSpec> {
    let Some(raw_properties) = declaration_value(entries, "transition-property") else {
        return Vec::new();
    };
    let mut transitions: Vec<TransitionSpec> = Vec::new();
    for (index, raw_property) in split_top_level_commas(raw_properties).iter().enumerate() {
        let Some(property) = parse_transition_property(raw_property) else {
            continue;
        };
        if property == TransitionProperty::None {
            continue;
        }
        let duration_ms = list_item(entries, "transition-duration", index)
            .and_then(parse_time_ms)
            .map_or(0.0, |ms| ms.clamp(0.0, MAX_TIME_MS));
        let timing = list_item(entries, "transition-timing-function", index)
            .and_then(parse_timing_function)
            .unwrap_or(TimingFunction::Ease);
        let delay_ms = list_item(entries, "transition-delay", index)
            .and_then(parse_time_ms)
            .map_or(0.0, |ms| ms.clamp(-MAX_TIME_MS, MAX_TIME_MS));
        let spec = TransitionSpec {
            property,
            duration_ms,
            timing,
            delay_ms,
        };
        // A later entry for the same property replaces the earlier one (CSS Transitions §2).
        if let Some(existing) = transitions
            .iter_mut()
            .find(|existing| existing.property == spec.property)
        {
            *existing = spec;
        } else {
            transitions.push(spec);
        }
    }
    transitions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_list_of_declarations;

    fn entries(source: &str) -> Vec<Declaration> {
        parse_list_of_declarations(source)
    }

    fn references(declarations: &[Declaration]) -> Vec<(&str, &Declaration)> {
        declarations
            .iter()
            .map(|declaration| (declaration.name.as_str(), declaration))
            .collect()
    }

    #[test]
    fn timing_functions_parse() {
        assert_eq!(
            parse_timing_function("linear"),
            Some(TimingFunction::Linear)
        );
        assert_eq!(
            parse_timing_function("EASE-IN"),
            Some(TimingFunction::EaseIn)
        );
        assert_eq!(
            parse_timing_function("cubic-bezier(0.1, 0.2, 0.9, 1)"),
            Some(TimingFunction::CubicBezier(0.1, 0.2, 0.9, 1.0))
        );
        assert_eq!(
            parse_timing_function("cubic-bezier(1.2, 0, 0, 1)"),
            None,
            "x coordinates must be in 0..=1"
        );
        assert_eq!(
            parse_timing_function("steps(4, jump-start)"),
            Some(TimingFunction::Steps(4, StepPosition::Start))
        );
        assert_eq!(parse_timing_function("steps(0)"), None);
        assert_eq!(parse_timing_function("bogus"), None);
    }

    #[test]
    fn times_and_counts_parse() {
        assert_eq!(parse_time_ms("0.3s"), Some(300.0));
        assert_eq!(parse_time_ms("250ms"), Some(250.0));
        assert_eq!(parse_time_ms("0"), Some(0.0));
        assert_eq!(parse_time_ms("bogus"), None);
        assert_eq!(
            parse_iteration_count("infinite"),
            Some(IterationCount::Infinite)
        );
        assert_eq!(parse_iteration_count("-1"), None);
        assert_eq!(
            parse_iteration_count("2.5"),
            Some(IterationCount::Number(2.5))
        );
    }

    #[test]
    fn animation_longhands_cycle_by_index() {
        let declarations = entries(
            "animation-name: fade, slide; animation-duration: 1s; \
             animation-delay: 0s, 100ms; animation-iteration-count: infinite; \
             animation-direction: alternate; animation-timing-function: linear;",
        );
        let animations = parse_animations(&references(&declarations));
        assert_eq!(animations.len(), 2);
        assert_eq!(animations[0].name, "fade");
        assert_eq!(animations[1].name, "slide");
        assert_eq!(animations[0].duration_ms, 1000.0);
        assert_eq!(animations[1].duration_ms, 1000.0, "duration list cycles");
        assert_eq!(animations[0].delay_ms, 0.0);
        assert_eq!(animations[1].delay_ms, 100.0);
        assert_eq!(animations[0].iteration_count, IterationCount::Infinite);
        assert_eq!(animations[0].direction, AnimationDirection::Alternate);
        assert_eq!(animations[0].timing, TimingFunction::Linear);
        assert_eq!(animations[0].fill_mode, FillMode::None);
    }

    #[test]
    fn transitions_parse_and_last_wins() {
        let declarations = entries(
            "transition-property: opacity, transform, opacity; \
             transition-duration: 300ms; transition-timing-function: ease-out; \
             transition-delay: 50ms;",
        );
        let transitions = parse_transitions(&references(&declarations));
        assert_eq!(transitions.len(), 2);
        assert_eq!(
            transitions[0].property,
            TransitionProperty::Property("opacity".into())
        );
        assert_eq!(transitions[0].duration_ms, 300.0);
        assert_eq!(transitions[0].timing, TimingFunction::EaseOut);
        assert_eq!(transitions[0].delay_ms, 50.0);
        assert_eq!(
            transitions[1].property,
            TransitionProperty::Property("transform".into())
        );
    }

    #[test]
    fn none_animations_and_transitions_are_empty() {
        let declarations = entries("animation-name: none; transition-property: none;");
        let references = references(&declarations);
        assert!(parse_animations(&references).is_empty());
        assert!(parse_transitions(&references).is_empty());
    }
}
