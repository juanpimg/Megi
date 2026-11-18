//! CSS Animations 1 and CSS Transitions 1 longhand values.
//!
//! This module only parses and normalizes the longhand declarations
//! (`animation-name/-duration/-timing-function/-delay/-iteration-count/-direction/
//! -fill-mode/-play-state` and `transition-property/-duration/-timing-function/-delay`).
//! Resolving `@keyframes` (already available in [`crate::rule::Stylesheet::keyframes`]) and
//! applying the timeline is the render layer's job.

use crate::rule::Declaration;

mod chunk1;
pub use chunk1::AnimationDirection;
pub use chunk1::AnimationSpec;
pub use chunk1::FillMode;
pub use chunk1::IterationCount;
pub use chunk1::MAX_ITERATIONS;
pub use chunk1::MAX_TIME_MS;
pub use chunk1::PlayState;
pub use chunk1::StepPosition;
pub use chunk1::TimingFunction;
pub use chunk1::TransitionProperty;
pub use chunk1::TransitionSpec;
#[allow(unused_imports)]
pub(crate) use chunk1::declaration_value;
#[allow(unused_imports)]
pub(crate) use chunk1::function_args;
#[allow(unused_imports)]
pub(crate) use chunk1::list_item;
#[allow(unused_imports)]
pub(crate) use chunk1::parse_animations;
pub use chunk1::parse_direction;
pub use chunk1::parse_fill_mode;
pub use chunk1::parse_iteration_count;
pub use chunk1::parse_play_state;
pub use chunk1::parse_time_ms;
pub use chunk1::parse_timing_function;
pub use chunk1::parse_transition_property;
#[allow(unused_imports)]
pub(crate) use chunk1::split_top_level_commas;
#[allow(unused_imports)]
pub(crate) use chunk1::split_top_level_components;
mod chunk2;
#[allow(unused_imports)]
pub(crate) use chunk2::parse_transitions;
