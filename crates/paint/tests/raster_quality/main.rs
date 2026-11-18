//! Rasterizer quality tests for WP-4.11 + WP-4.3:
//!
//! - antialiasing of fractional rectangle edges (and no round-out over-paint),
//! - per-side borders (colors, styles, mitred corners),
//! - clip semantics and safe `pop_clip`,
//! - text-cache telemetry (shaped runs and glyph masks are reused),
//! - culling of off-screen items,
//! - `Pixmap` dimension validation.

mod antialiasing;
mod borders;
mod clipping;
mod common;
mod regressions;
mod text;
