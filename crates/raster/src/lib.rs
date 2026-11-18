//! # Megi Raster (`megi-raster`)
//!
//! Pure `DisplayList` -> RGBA frame -> deterministic PNG pipeline.
//! No DOM, layout, network or window dependencies.
//!
//! ## Scratch buffers
//!
//! `rasterizer`, `png` and `deflate` keep reusable `thread_local RefCell`
//! scratch buffers (blur planes, shadow masks, encode/deflate workspace). They
//! are per-thread caches, never shared state: a fresh thread always starts from
//! an empty buffer, and results are byte-identical regardless of buffer reuse.
//! A future `scratch.rs` can group them without changing the public API.

#![forbid(unsafe_code)]

pub mod deflate;
pub mod inflate;
pub mod png;
pub mod rasterizer;

pub use deflate::zlib_compress;
pub use inflate::inflate_zlib;
pub use png::{PngRegionEncoder, encode_pixels, encode_png_regions, encode_rgba, filter_scanlines};
pub use rasterizer::{
    MAX_PIXMAP_PIXELS, Pixmap, RasterStats, RenderConfig, bundled_font_database,
    rasterize_display_list, rasterize_display_list_with_config,
    rasterize_display_list_with_font_database, rasterize_display_list_with_font_database_and_stats,
};
