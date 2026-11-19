//! Sized submodule of the parent facade: deterministic CSS animation/transition DOM events.
//!
//! The event list is derived only from animation specs, keyframes and the current timeline
//! (`time_ms`), never from a wall clock. This keeps event dispatch reproducible in headless
//! renders and under the shell's fixed-epoch event loop (#941, #942).

use super::*;

/// Lifecycle phase of a scheduled CSS animation DOM event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationEventKind {
    Start,
    Iteration,
    End,
}

/// A deterministic CSS `@keyframes` animation DOM event at `time_ms`.
#[derive(Clone, Debug, PartialEq)]
pub struct ScheduledAnimationEvent {
    pub event_type: &'static str,
    pub animation_name: String,
    pub elapsed_time_secs: f64,
    pub kind: AnimationEventKind,
}

/// Lifecycle phase of a scheduled CSS transition DOM event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionEventKind {
    Run,
    Start,
    End,
}

/// A deterministic CSS transition DOM event at `elapsed_ms`.
#[derive(Clone, Debug, PartialEq)]
pub struct ScheduledTransitionEvent {
    pub event_type: &'static str,
    pub property_name: String,
    pub elapsed_time_secs: f64,
    pub kind: TransitionEventKind,
}

/// Maximum `animationiteration` events emitted per animation spec so huge `time_ms` / tiny
/// durations stay bounded.
const MAX_ITERATION_EVENTS_PER_ANIM: usize = 32;

/// Returns deterministic CSS animation events (`animationstart`, `animationiteration`,
/// `animationend`) whose trigger thresholds have been reached at `time_ms`.
pub fn collect_animation_events(
    specs: &[AnimationSpec],
    rules: &[KeyframesRule],
    time_ms: f32,
) -> Vec<ScheduledAnimationEvent> {
    let mut events = Vec::new();
    if !time_ms.is_finite() || specs.is_empty() || rules.is_empty() {
        return events;
    }
    for spec in specs {
        if spec.play_state == crate::animation::PlayState::Paused {
            continue;
        }
        if !rules.iter().any(|rule| rule.name == spec.name) {
            continue;
        }
        let iterations = match spec.iteration_count {
            IterationCount::Number(count) => count.max(0.0),
            IterationCount::Infinite => f32::INFINITY,
        };
        if iterations == 0.0 || time_ms < spec.delay_ms {
            continue;
        }
        let duration_ms = spec.duration_ms.max(0.0);
        let duration_ms_f64 = f64::from(duration_ms);
        let total_ms = if iterations.is_finite() {
            duration_ms * iterations
        } else {
            f32::INFINITY
        };
        let total_ms_f64 = if iterations.is_finite() {
            duration_ms_f64 * f64::from(iterations)
        } else {
            f64::INFINITY
        };
        let start_elapsed_secs = if spec.delay_ms < 0.0 {
            f64::from(-spec.delay_ms).min(total_ms_f64) / 1000.0
        } else {
            0.0
        };
        events.push(ScheduledAnimationEvent {
            event_type: "animationstart",
            animation_name: spec.name.clone(),
            elapsed_time_secs: start_elapsed_secs,
            kind: AnimationEventKind::Start,
        });
        let active_elapsed = time_ms - spec.delay_ms;
        if duration_ms > 0.0 && iterations > 1.0 && active_elapsed >= duration_ms {
            let max_completed = (active_elapsed / duration_ms).floor();
            let capped_last_iter = if iterations.is_finite() {
                // `animationiteration` does not fire on the iteration that finishes the animation.
                let last_interior = (iterations - f32::EPSILON).floor();
                max_completed.min(last_interior)
            } else {
                max_completed
            };
            let count = capped_last_iter.max(0.0) as usize;
            let emit_from = count
                .saturating_sub(MAX_ITERATION_EVENTS_PER_ANIM - 1)
                .max(1);
            for k in emit_from..=count {
                events.push(ScheduledAnimationEvent {
                    event_type: "animationiteration",
                    animation_name: spec.name.clone(),
                    elapsed_time_secs: (k as f64 * duration_ms_f64) / 1000.0,
                    kind: AnimationEventKind::Iteration,
                });
            }
        }
        if total_ms.is_finite() && active_elapsed >= total_ms {
            events.push(ScheduledAnimationEvent {
                event_type: "animationend",
                animation_name: spec.name.clone(),
                elapsed_time_secs: total_ms_f64 / 1000.0,
                kind: AnimationEventKind::End,
            });
        }
    }
    events
}

/// Returns deterministic CSS transition events (`transitionrun`, `transitionstart`,
/// `transitionend`) for properties that changed from `from` to `to` at `elapsed_ms`.
pub fn collect_transition_events(
    from: &ComputedStyle,
    to: &ComputedStyle,
    elapsed_ms: f32,
) -> Vec<ScheduledTransitionEvent> {
    let mut events = Vec::new();
    if to.transitions.is_empty() || !elapsed_ms.is_finite() || elapsed_ms < 0.0 {
        return events;
    }
    for &prop in ANIMATABLE_PROPERTIES {
        let Some(spec) = super::chunk1::effective_transition_spec(&to.transitions, prop) else {
            continue;
        };
        if spec.duration_ms <= 0.0 || !spec.duration_ms.is_finite() {
            continue;
        }
        let Some(a) = base_animatable(prop, from) else {
            continue;
        };
        let Some(b) = base_animatable(prop, to) else {
            continue;
        };
        if a == b {
            continue;
        }
        let duration_ms_f64 = f64::from(spec.duration_ms);
        let start_elapsed_secs = if spec.delay_ms < 0.0 {
            f64::from(-spec.delay_ms).min(duration_ms_f64) / 1000.0
        } else {
            0.0
        };
        events.push(ScheduledTransitionEvent {
            event_type: "transitionrun",
            property_name: prop.to_string(),
            elapsed_time_secs: start_elapsed_secs,
            kind: TransitionEventKind::Run,
        });
        if elapsed_ms >= spec.delay_ms {
            events.push(ScheduledTransitionEvent {
                event_type: "transitionstart",
                property_name: prop.to_string(),
                elapsed_time_secs: start_elapsed_secs,
                kind: TransitionEventKind::Start,
            });
        }
        if elapsed_ms >= spec.delay_ms + spec.duration_ms {
            events.push(ScheduledTransitionEvent {
                event_type: "transitionend",
                property_name: prop.to_string(),
                elapsed_time_secs: duration_ms_f64 / 1000.0,
                kind: TransitionEventKind::End,
            });
        }
    }
    events
}
