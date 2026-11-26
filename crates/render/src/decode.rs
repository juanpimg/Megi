//! Small-image decoding shared with the browser shell (favicons).
//!
//! SVG payloads are rasterized by the dependency-free `svg` crate at the requested
//! size; every other supported format goes through the in-tree `image` decoder.
//! Both paths are fallible and return `None` for unsupported or corrupt bytes.

/// Largest payload accepted by [`decode_image_with_svg`] (1 MiB).
///
/// Favicons are capped further by the shell at 64 KiB; this bound protects the
/// direct render API from parsing or rasterizing arbitrarily large documents.
const MAX_SVG_IMAGE_BYTES: usize = 1024 * 1024;

/// Decodes `bytes` into straight-RGBA pixels, rasterizing SVG at `size`×`size`.
///
/// SVG detection sniffs the document prolog (UTF-8 BOM, ASCII whitespace and
/// `<?xml`/`<svg` are tolerated); non-SVG bytes are decoded as a raster image.
/// Returns `None` if the payload exceeds 1 MiB, is not a supported image, or
/// cannot be rasterized within the `svg` crate's anti-DoS limits.
pub fn decode_image_with_svg(bytes: &[u8], size: u32) -> Option<megi_image::RgbaImage> {
    if bytes.len() > MAX_SVG_IMAGE_BYTES {
        return None;
    }
    if crate::svg_assets::sniff_svg(bytes) {
        let options = megi_svg::SvgOptions {
            width: Some(size),
            height: Some(size),
        };
        let limits = megi_svg::Limits::default();
        return megi_svg::render_svg_with_limits(bytes, &options, &limits)
            .ok()?
            .into_rgba_image()
            .ok();
    }
    megi_image::decode(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::{MAX_SVG_IMAGE_BYTES, decode_image_with_svg};

    #[test]
    fn svg_with_bom_and_xml_prolog_is_rasterized_at_requested_size() {
        let mut bytes = b"\xEF\xBB\xBF  <?xml version=\"1.0\"?>\n".to_vec();
        bytes.extend_from_slice(
            b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"4\" height=\"4\">\
              <rect width=\"4\" height=\"4\" fill=\"#ff0000\"/></svg>",
        );
        let image = decode_image_with_svg(&bytes, 16).expect("svg raster");
        assert_eq!(image.dimensions(), (16, 16));
        assert!(image.data.iter().any(|&channel| channel != 0));
    }

    #[test]
    fn oversized_svg_payload_is_rejected_before_parsing() {
        let mut bytes = b"<svg xmlns=\"http://www.w3.org/2000/svg\">".to_vec();
        bytes.resize(MAX_SVG_IMAGE_BYTES + 1, b' ');
        bytes.extend_from_slice(b"</svg>");
        assert!(decode_image_with_svg(&bytes, 16).is_none());
    }
}
