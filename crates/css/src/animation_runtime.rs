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

mod chunk1;
#[allow(unused_imports)]
pub(crate) use chunk1::ANIMATABLE_PROPERTIES;
#[allow(unused_imports)]
pub(crate) use chunk1::AnimatableValue;
#[allow(unused_imports)]
pub(crate) use chunk1::EvaluatedKeyframe;
#[allow(unused_imports)]
pub(crate) use chunk1::MAX_KEYFRAME_STEPS;
pub use chunk1::animation_progress;
pub use chunk1::apply_animations;
#[allow(unused_imports)]
pub(crate) use chunk1::apply_keyframes;
pub use chunk1::apply_transitions;
#[allow(unused_imports)]
pub(crate) use chunk1::cubic_bezier;
pub use chunk1::evaluate_timing_function;
#[allow(unused_imports)]
pub(crate) use chunk1::is_animatable;
#[allow(unused_imports)]
pub(crate) use chunk1::parse_animatable;
#[allow(unused_imports)]
pub(crate) use chunk1::steps;
pub use chunk1::transition_progress;
mod chunk2;
#[allow(unused_imports)]
pub(crate) use chunk2::base_animatable;
#[allow(unused_imports)]
pub(crate) use chunk2::interpolate_animatable;
#[allow(unused_imports)]
pub(crate) use chunk2::interpolate_property;
#[allow(unused_imports)]
pub(crate) use chunk2::lerp_color;
#[allow(unused_imports)]
pub(crate) use chunk2::write_property;
mod chunk3;
pub use chunk3::{
    AnimationEventKind, ScheduledAnimationEvent, ScheduledTransitionEvent, TransitionEventKind,
    collect_animation_events, collect_transition_events,
};
