//! Public data types and defensive budgets for background image loading.

use megi_core::NodeId;

/// Default maximum number of distinct background/list-style images per page.
pub const DEFAULT_MAX_BACKGROUNDS: usize = 32;
/// Default maximum downloaded size of one background image (8 MiB).
pub const DEFAULT_MAX_BACKGROUND_BYTES: usize = 8 * 1024 * 1024;
/// Default maximum total downloaded background bytes per page (16 MiB).
pub const DEFAULT_MAX_TOTAL_BACKGROUND_BYTES: usize = 16 * 1024 * 1024;

/// Defensive caps applied while loading background images.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackgroundLimits {
    /// Maximum number of distinct (deduplicated) background URLs fetched.
    pub max_backgrounds: usize,
    /// Maximum downloaded bytes for a single background image.
    pub max_background_bytes: usize,
    /// Maximum total downloaded background bytes for the page.
    pub max_total_bytes: usize,
    /// Maximum total decoded pixels remaining for distinct background bitmaps.
    pub max_total_decoded_pixels: u64,
    /// Optional decode downscale cap (`RenderOptions::max_image_dim`).
    pub max_image_dim: Option<u32>,
}

impl Default for BackgroundLimits {
    fn default() -> Self {
        Self {
            max_backgrounds: DEFAULT_MAX_BACKGROUNDS,
            max_background_bytes: DEFAULT_MAX_BACKGROUND_BYTES,
            max_total_bytes: DEFAULT_MAX_TOTAL_BACKGROUND_BYTES,
            max_total_decoded_pixels: crate::images::DEFAULT_MAX_TOTAL_DECODED_PIXELS,
            max_image_dim: None,
        }
    }
}

/// One `background-image`/`list-style-image` reference found in the computed styles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackgroundReference {
    /// Element carrying the declaration.
    pub node: NodeId,
    /// Raw CSS URL string exactly as stored in [`megi_css::ComputedStyle`] (the paint map key).
    pub raw_url: String,
    /// Which property produced the reference.
    pub kind: BackgroundKind,
}

/// Property that referenced a background image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackgroundKind {
    /// `background-image` on the element's padding box.
    Background,
    /// `list-style-image` marker for `display: list-item` boxes.
    ListStyleImage,
    /// `mask-image` / `-webkit-mask-image` alpha source for the box.
    MaskImage,
}

/// Result of the background-loading stage.
pub struct LoadedBackgrounds {
    /// Paint-ready map: raw CSS URL → decoded RGBA bitmap (shared between both kinds).
    pub images: megi_paint::BackgroundImages,
    /// References found in the cascade (before deduplication).
    pub references: usize,
    /// Distinct URLs successfully fetched and decoded.
    pub loaded: usize,
    /// Distinct URLs that failed (invalid, blocked, too large, unsupported format, ...).
    pub failed: usize,
    /// Background images that were SVGs rasterized through the `svg` crate.
    pub svgs_rendered: usize,
    /// Downloaded (encoded) bytes accepted by the caps.
    pub downloaded_bytes: usize,
    /// Total decoded pixels (`width * height`) across distinct decoded background bitmaps.
    pub decoded_pixels: u64,
    /// Non-fatal warnings produced by this stage.
    pub warnings: Vec<String>,
}

impl LoadedBackgrounds {
    /// Returns `true` when no background image is available for paint.
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }
}
