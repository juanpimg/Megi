//! CSS `background-repeat`/`background-size`/`background-position` painting
//! (R89): tile geometry, cover/contain, positions, clipping, raster output and
//! the legacy stretch compatibility path.

mod color_clip;
mod common;
mod layers;
mod tiling;
