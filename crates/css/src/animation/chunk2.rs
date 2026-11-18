//! Sized submodule of the parent facade (verbatim move).

use super::*;

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
pub(crate) mod boundary_tests {
    use super::*;

    #[test]
    fn function_args_does_not_panic_on_multibyte_name_prefix() {
        // byte index name.len() (=5) falls inside 'é' in "aaaaé(1)"; checking only
        // name.len()+1 let the old code slice value[..5] and panic.
        assert_eq!(function_args("aaaaé(1)", "steps"), None);
        assert_eq!(function_args("aaaaaaaaaaaé(0,0,0,0)", "cubic-bezier"), None);
        assert_eq!(function_args("steps(4)", "steps"), Some("4"));
    }
}

#[cfg(test)]
pub(crate) mod tests {
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
