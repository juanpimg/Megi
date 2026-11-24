//! Public data types and defensive budgets for `<img>` loading.

use std::collections::HashMap;

use megi_core::NodeId;

/// Default maximum number of `<img>` elements loaded per page.
pub const DEFAULT_MAX_IMAGES: usize = 64;
/// Default maximum downloaded size of one image (8 MiB).
pub const DEFAULT_MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
/// Default maximum total downloaded image bytes per page (32 MiB).
pub const DEFAULT_MAX_TOTAL_IMAGE_BYTES: usize = 32 * 1024 * 1024;
/// Default maximum total decoded pixels across `<img>` and CSS background images (64 Mpx ≈ 256 MiB RGBA).
pub const DEFAULT_MAX_TOTAL_DECODED_PIXELS: u64 = 64 * 1024 * 1024;

/// Defensive caps and display metrics applied while loading images.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageLimits {
    /// Maximum number of `<img>` elements fetched.
    pub max_images: usize,
    /// Maximum downloaded bytes for a single image.
    pub max_image_bytes: usize,
    /// Maximum total downloaded image bytes for the page.
    pub max_total_bytes: usize,
    /// Maximum total decoded pixels across all distinct `<img>` bitmaps.
    pub max_total_decoded_pixels: u64,
    /// Optional decode downscale cap (`RenderOptions::max_image_dim`).
    pub max_image_dim: Option<u32>,
    /// Device pixel ratio used when selecting `srcset` / `<picture>` candidates (default `1.0`).
    pub device_pixel_ratio: f32,
}

impl Default for ImageLimits {
    fn default() -> Self {
        Self {
            max_images: DEFAULT_MAX_IMAGES,
            max_image_bytes: DEFAULT_MAX_IMAGE_BYTES,
            max_total_bytes: DEFAULT_MAX_TOTAL_IMAGE_BYTES,
            max_total_decoded_pixels: DEFAULT_MAX_TOTAL_DECODED_PIXELS,
            max_image_dim: None,
            device_pixel_ratio: 1.0,
        }
    }
}

/// Result of the image-loading stage.
pub struct LoadedImages {
    /// Decoded images keyed by their `<img>` DOM node.
    pub store: megi_paint::ImageStore,
    /// Intrinsic `(width, height)` in CSS px, ready for layout's replaced sizes.
    pub sizes: HashMap<NodeId, (f32, f32)>,
    /// Images successfully fetched and decoded.
    pub loaded: usize,
    /// Images that failed (fetch error, too large, unsupported format, ...).
    pub failed: usize,
    /// `<img>` references discarded solely because the `max_images` cap was reached (#2250).
    pub capped: usize,
    /// Downloaded (compressed) image bytes actually accepted.
    pub downloaded_bytes: usize,
    /// Total decoded pixels (`width * height`) across distinct decoded bitmaps.
    pub decoded_pixels: u64,
    /// `<img>` images that were SVGs rasterized through the `svg` crate.
    pub svgs_rendered: usize,
    /// Non-fatal warnings produced by this stage.
    pub warnings: Vec<String>,
}

/// Parsed descriptor on a single `srcset` image candidate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SrcsetDescriptor {
    /// Pixel density multiplier (`1x`, `2x`, or default `1x`).
    Density(f32),
    /// Physical width descriptor (`400w`, `800w`, ...).
    Width(u32),
}

/// One parsed candidate entry from an HTML `srcset` attribute.
#[derive(Clone, Debug, PartialEq)]
pub struct SrcsetCandidate {
    /// Raw (unjoined) URL string for the candidate.
    pub url: String,
    /// Parsed `w` or `x` descriptor.
    pub descriptor: SrcsetDescriptor,
}
