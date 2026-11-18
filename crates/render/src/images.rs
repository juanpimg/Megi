//! # `<img>` discovery, loading, decoding and intrinsic sizes (WP-5.7)
//!
//! `render` resolves every `<img src>` against the document base URL, fetches it
//! with the shared [`megi_net::ResourceLoader`] (HTTP cache, redirects, TLS, caps and
//! `--no-network`) and decodes it with the dependency-free `image` crate.
//!
//! ## Policy (mirrors the stylesheet loader)
//!
//! - Missing/blocked/failed subresources are **non-fatal**: the render yields a
//!   gray placeholder box (painted by `megi_paint::IMAGE_PLACEHOLDER`) and a warning.
//! - PNG, baseline/progressive JPEG, GIF (first frame), **lossless WebP (VP8L)** and
//!   **lossy WebP (VP8 keyframe)** are decoded with the in-tree `image` crate;
//!   unsupported JPEG variants and corrupt/unsupported WebP payloads produce a
//!   non-fatal warning and a placeholder, never a hard error.
//! - Defensive caps, configurable through [`crate::RenderOptions`]:
//!   [`DEFAULT_MAX_IMAGES`] images, [`DEFAULT_MAX_IMAGE_BYTES`] per image and
//!   [`DEFAULT_MAX_TOTAL_IMAGE_BYTES`] of downloaded image bytes per page.
//! - Remote images require network access: under `--no-network`, `http(s)`
//!   images are skipped with a warning (offline tests use `file:`/`data:`).
//!
//! The decoded RGBA pixels and the intrinsic `(width, height)` sizes are handed
//! to `paint` ([`megi_paint::ImageStore`]) and `layout`
//! (`layout_document_with_measurer_and_replaced_sizes`), respectively.

mod load;
mod selection;
mod sources;
#[cfg(test)]
mod tests;
mod types;

pub use load::{load_images, load_images_with_context};
pub use selection::{
    is_supported_image_mime, parse_srcset, resolve_sizes, sanitize_dpr, select_image_candidate,
};
pub use sources::{
    collect_image_sources, collect_responsive_image_sources, resolve_element_image_source,
    resolve_img_source,
};
pub use types::{
    DEFAULT_MAX_IMAGE_BYTES, DEFAULT_MAX_IMAGES, DEFAULT_MAX_TOTAL_DECODED_PIXELS,
    DEFAULT_MAX_TOTAL_IMAGE_BYTES, ImageLimits, LoadedImages, SrcsetCandidate, SrcsetDescriptor,
};
