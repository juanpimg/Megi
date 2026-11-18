//! # CSS background images & `list-style-image` loading (WP-5.9)
//!
//! After the cascade, [`megi_css::ComputedStyle`] keeps the **raw, unresolved** URL text of
//! `background-image` and `list-style-image` (`Option<String>`). This module:
//!
//! 1. collects those URLs from every styled element, in document order;
//! 2. resolves them against the **document base URL** (see the limitation below),
//!    deduplicating by resolved URL so each bitmap is fetched/decoded once;
//! 3. fetches the bytes with the shared [`megi_net::ResourceLoader`] (honoring
//!    `--no-network`, the HTTP cache, redirects and TLS) under defensive caps:
//!    [`DEFAULT_MAX_BACKGROUNDS`] distinct images, [`DEFAULT_MAX_BACKGROUND_BYTES`] per
//!    image and [`DEFAULT_MAX_TOTAL_BACKGROUND_BYTES`] per page;
//! 4. decodes the bitmap with the in-tree `image` crate — PNG, JPEG, GIF, lossless
//!    WebP (VP8L) and lossy WebP (VP8 keyframe); corrupt/unsupported payloads
//!    produce a warning and no image — and
//! 5. builds the [`megi_paint::BackgroundImages`] map that
//!    [`megi_paint::build_display_list_with_images_and_backgrounds`] consumes: key = the raw
//!    CSS URL string (so both `background-image` and `list-style-image` share the same
//!    entry), value = `Arc<megi_image::RgbaImage>`.
//!
//! Failures are **non-fatal**: the URL is simply absent from the map, so paint draws the
//! background color (or the fallback `list-style-type` marker) and the render records a
//! warning. The same `Arc` is shared when the same URL is used as a background and as a
//! list marker.
//!
//! ## Base-URL limitation (documented)
//!
//! URLs are resolved against the **document** base URL (`<base href>` or the document
//! URL), not against the stylesheet that declared them: `@import` does not propagate its
//! own base. This matches the current stylesheet loader and is called out in the crate
//! docs.
//!
//! ## Paint policy (owned by `paint`)
//!
//! The bitmap is stretched into the element's padding box (no `background-repeat`,
//! `background-size`, `background-position` or `background-clip` yet); the background
//! color paints first, the image on top, then borders/content. `list-style-image`
//! replaces the default marker when present. Canvas propagation (`html`/`body`) only
//! propagates the color.

mod collect;
mod load;
#[cfg(test)]
mod tests;
mod types;

pub use collect::collect_background_references;
pub use load::{load_backgrounds, load_backgrounds_with_pseudos};
pub use types::{
    BackgroundKind, BackgroundLimits, BackgroundReference, DEFAULT_MAX_BACKGROUND_BYTES,
    DEFAULT_MAX_BACKGROUNDS, DEFAULT_MAX_TOTAL_BACKGROUND_BYTES, LoadedBackgrounds,
};
