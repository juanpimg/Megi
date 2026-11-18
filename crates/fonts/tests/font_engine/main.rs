//! Deterministic integration tests for the Megi font engine.
//!
//! The tests load the bundled DejaVu fonts from `assets/` (Bitstream Vera
//! / DejaVu license in `assets/LICENSE-DejaVu.txt`), so the asserted metrics are
//! reproducible on any machine and do not depend on installed system fonts.

mod common;
mod database;
mod metrics;
mod raster;
mod shaping;
