//! CSS value parsing helpers (extracted from `properties`).

use crate::rule::Declaration;

use super::values::*;

mod chunk1;
#[allow(unused_imports)]
pub use chunk1::MAX_BOX_SHADOWS;
#[allow(unused_imports)]
pub use chunk1::MAX_TEXT_SHADOWS;
#[allow(unused_imports)]
pub(crate) use chunk1::expand_radius_values;
#[allow(unused_imports)]
pub(crate) use chunk1::parse_border_radius_shorthand;
#[allow(unused_imports)]
pub(crate) use chunk1::parse_box_shadows;
#[allow(unused_imports)]
pub(crate) use chunk1::parse_shadow_length;
#[allow(unused_imports)]
pub(crate) use chunk1::parse_text_shadows;
#[allow(unused_imports)]
pub(crate) use chunk1::split_border_radius_value;
#[allow(unused_imports)]
pub(crate) use chunk1::split_top_level_commas_strict;
mod chunk2;
#[allow(unused_imports)]
pub(crate) use chunk2::BackgroundShorthandTokens;
#[allow(unused_imports)]
pub(crate) use chunk2::cycle_property_list;
#[allow(unused_imports)]
pub(crate) use chunk2::first_background_layer;
#[allow(unused_imports)]
pub(crate) use chunk2::is_background_attachment_keyword;
#[allow(unused_imports)]
pub(crate) use chunk2::is_background_box_keyword;
#[allow(unused_imports)]
pub(crate) use chunk2::is_background_image_token;
#[allow(unused_imports)]
pub(crate) use chunk2::is_background_position_keyword;
#[allow(unused_imports)]
pub(crate) use chunk2::is_background_repeat_keyword;
#[allow(unused_imports)]
pub(crate) use chunk2::looks_like_length;
#[allow(unused_imports)]
pub(crate) use chunk2::parse_background_shorthand_layers;
#[allow(unused_imports)]
pub(crate) use chunk2::parse_background_shorthand_tokens;
#[allow(unused_imports)]
pub(crate) use chunk2::split_background_layer_at_slash;
#[allow(unused_imports)]
pub(crate) use chunk2::split_top_level_commas;
mod chunk3;
#[allow(unused_imports)]
pub(crate) use chunk3::GradientForm;
#[allow(unused_imports)]
pub(crate) use chunk3::parse_aspect_ratio;
#[allow(unused_imports)]
pub(crate) use chunk3::parse_background_image_component;
#[allow(unused_imports)]
pub(crate) use chunk3::parse_border_spacing;
#[allow(unused_imports)]
pub(crate) use chunk3::parse_filter_function;
#[allow(unused_imports)]
pub(crate) use chunk3::parse_filter_length;
#[allow(unused_imports)]
pub(crate) use chunk3::parse_filter_list;
#[allow(unused_imports)]
pub(crate) use chunk3::parse_image_set_url;
#[allow(unused_imports)]
pub(crate) use chunk3::parse_number_or_percent;
#[allow(unused_imports)]
pub(crate) use chunk3::split_components;
#[allow(unused_imports)]
pub(crate) use chunk3::split_function_token;
#[allow(unused_imports)]
pub(crate) use chunk3::strip_quoted_string;
mod chunk4;
#[allow(unused_imports)]
pub(crate) use chunk4::parse_conic_prefix;
#[allow(unused_imports)]
pub(crate) use chunk4::parse_gradient_component;
#[allow(unused_imports)]
pub(crate) use chunk4::parse_gradient_stop_list;
#[allow(unused_imports)]
pub(crate) use chunk4::parse_gradient_stop_position;
#[allow(unused_imports)]
pub(crate) use chunk4::parse_linear_direction;
#[allow(unused_imports)]
pub(crate) use chunk4::parse_stop_color;
#[allow(unused_imports)]
pub(crate) use chunk4::push_gradient_stop;
#[allow(unused_imports)]
pub(crate) use chunk4::starts_with_color_stop;
#[allow(unused_imports)]
pub(crate) use chunk4::take_interpolation_space;
mod chunk5;
#[allow(unused_imports)]
pub(crate) use chunk5::BorderSpec;
#[allow(unused_imports)]
pub(crate) use chunk5::expand_four;
#[allow(unused_imports)]
pub(crate) use chunk5::parse_angle_degrees;
#[allow(unused_imports)]
pub(crate) use chunk5::parse_border_color_edges;
#[allow(unused_imports)]
pub(crate) use chunk5::parse_border_shorthand;
#[allow(unused_imports)]
pub(crate) use chunk5::parse_border_style_edges;
#[allow(unused_imports)]
pub(crate) use chunk5::parse_border_width;
#[allow(unused_imports)]
pub(crate) use chunk5::parse_border_width_edges;
#[allow(unused_imports)]
pub(crate) use chunk5::parse_color_component;
#[allow(unused_imports)]
pub(crate) use chunk5::parse_image_value;
#[allow(unused_imports)]
pub(crate) use chunk5::parse_radial_prefix;
#[allow(unused_imports)]
pub(crate) use chunk5::parse_url_component;
