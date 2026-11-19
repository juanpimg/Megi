//! Regression tests for the `before` flag of step timing functions (#1757).
//!
//! CSS Easing 1 §2.3.1: during the delay phase of an animation with backwards
//! fill, the before flag makes a `steps()` easing evaluate to the *bottom* of
//! the first step rather than the top. The normative example
//! `animation: moveRight 5s 1s steps(5, start) backwards` must therefore give
//! output progress 0 during the delay, not 0.2.

use megi_css::animation::{
    AnimationDirection, AnimationSpec, FillMode, IterationCount, PlayState, StepPosition,
    TimingFunction,
};
use megi_css::animation_runtime::{apply_animations, evaluate_timing_function};
use megi_css::parser::parse_stylesheet;
use megi_css::properties::ComputedStyle;

fn animation(name: &str, duration_ms: f32) -> AnimationSpec {
    AnimationSpec {
        name: name.to_string(),
        duration_ms,
        timing: TimingFunction::Linear,
        delay_ms: 0.0,
        iteration_count: IterationCount::Number(1.0),
        direction: AnimationDirection::Normal,
        fill_mode: FillMode::None,
        play_state: PlayState::Running,
    }
}

#[test]
fn steps_before_flag_uses_bottom_of_first_step() {
    // Without the flag, `steps(5, start)` yields 0.2 at input 0.
    let timing = TimingFunction::Steps(5, StepPosition::Start);
    assert_eq!(evaluate_timing_function(&timing, 0.0), 0.2);
    // The before-aware entry point added by the fix: with the flag, input 0
    // must yield the bottom of the first step (0).
    assert_eq!(
        megi_css::animation_runtime::evaluate_timing_function_before(&timing, 0.0, true),
        0.0
    );

    // `jump-end` needs the flag at input 1 (the reverse/past-the-finish case):
    // 1.0 without the flag, 0.8 (bottom of the last step) with it.
    let end = TimingFunction::Steps(5, StepPosition::End);
    assert_eq!(evaluate_timing_function(&end, 1.0), 1.0);
    assert_eq!(
        megi_css::animation_runtime::evaluate_timing_function_before(&end, 1.0, true),
        0.8
    );
}

#[test]
fn animation_delay_with_backwards_fill_starts_at_first_step_bottom() {
    let rules = parse_stylesheet("@keyframes moveRight { from { opacity: 0 } to { opacity: 1 } }")
        .keyframes;
    let mut spec = animation("moveRight", 5000.0);
    spec.delay_ms = 1000.0;
    spec.timing = TimingFunction::Steps(5, StepPosition::Start);
    spec.fill_mode = FillMode::Backwards;
    let base = ComputedStyle::initial();

    // During the delay phase the animation must not jump to the top of the first step.
    let during_delay = apply_animations(&base, std::slice::from_ref(&spec), &rules, 500.0);
    assert!(
        (during_delay.opacity - 0.0).abs() < 1e-6,
        "backwards fill with steps(start) must start at 0 during delay, got {}",
        during_delay.opacity
    );

    // At the exact start of the animation interval the before flag clears: the
    // top of the first step (0.2) applies.
    let at_start = apply_animations(&base, std::slice::from_ref(&spec), &rules, 1000.0);
    assert!(
        (at_start.opacity - 0.2).abs() < 1e-6,
        "at animation start steps(start) should yield 0.2, got {}",
        at_start.opacity
    );
}
