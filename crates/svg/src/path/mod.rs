//! SVG path-data parsing (`d` attribute) and curve flattening.
//!
//! Implements all SVG path commands (`M/m L/l H/h V/v C/c S/s Q/q T/t A/a Z/z`),
//! implicit command repetition, compact numbers, elliptical arcs converted to
//! cubic Béziers and adaptive flattening into polylines used by the rasterizer.
//! Every loop is bounded by the caller's segment budget, so hostile input cannot
//! cause unbounded work or allocation.

mod flatten;
mod parser;
#[cfg(test)]
mod tests;

pub(crate) use flatten::{flatten_cubic, flatten_quadratic};
pub(crate) use parser::parse_path_data;

/// One flattened sub-path (a polyline, optionally closed).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SubPath {
    /// Points in user space, in order.
    pub points: Vec<(f32, f32)>,
    /// `true` when the sub-path is explicitly closed (`Z`).
    pub closed: bool,
}
