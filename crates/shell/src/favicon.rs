//! Tab favicons and deterministic fallback dots.
//!
//! The shell looks for `<link rel="...icon...">` in the rendered document, resolves it
//! against the document URL, and (for `http(s)`/`file:`) fetches the bytes with the
//! renderer's fetch options (64 KiB cap). The icon is decoded with the `image` crate and
//! nearest-neighbour resized to 16x16; any failure (missing link, network error, corrupt
//! or unsupported image) falls back to a deterministic color derived from the origin.
//!
//! Limits: only the first icon link is used (no `sizes`/`type` selection, no SVG favicon
//! rasterization), the fallback dot is a plain square, and network favicons are only
//! fetched when networking is enabled for the render.

use dom::Document;

/// Favicon size in CSS pixels (drawn in the tab strip).
pub const ICON_SIZE: u32 = 16;

/// Maximum favicon payload accepted (64 KiB; hostile files cannot request large fetches).
pub const MAX_FAVICON_BYTES: usize = 64 * 1024;

/// A decoded 16x16 favicon or a deterministic fallback color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Favicon {
    /// RGBA8 pixels, row-major 16x16 (`None` when no usable image was decoded).
    pub pixels: Option<[u8; (ICON_SIZE * ICON_SIZE * 4) as usize]>,
    /// Fallback dot color (always present; also the background behind transparent icons).
    pub color: [u8; 3],
}

impl Favicon {
    /// Deterministic fallback dot for an origin/key.
    pub fn fallback(key: &str) -> Self {
        Self {
            pixels: None,
            color: color_for(key),
        }
    }

    /// Decodes `bytes` into a 16x16 nearest-neighbour favicon; falls back to `key`'s dot.
    pub fn from_bytes(bytes: &[u8], key: &str) -> Self {
        Self {
            pixels: decode_icon(bytes),
            color: color_for(key),
        }
    }
}

/// Extracts the first `<link rel="...icon...">` URL from `document`, or the conventional
/// `/favicon.ico` of `page`'s origin as a fallback candidate.
pub fn icon_url(document: &Document, page: &net::Url) -> Option<net::Url> {
    for node_id in document.descendants(document.root()) {
        let Some(element) = document.node(node_id).as_element() else {
            continue;
        };
        if element.local_name != "link" {
            continue;
        }
        let Some(rel) = element.get_attribute("rel") else {
            continue;
        };
        let is_icon = rel
            .split_ascii_whitespace()
            .any(|token| token.eq_ignore_ascii_case("icon"));
        if !is_icon {
            continue;
        }
        let Some(href) = element.get_attribute("href") else {
            continue;
        };
        if let Ok(url) = page.join(href) {
            return Some(url);
        }
    }
    // Conventional fallback: /favicon.ico next to the document.
    page.join("favicon.ico").ok()
}

/// Fetches favicon bytes for a URL: `file:` reads the filesystem, `http(s)` uses the
/// renderer's fetch options. Returns `None` on any failure or when the payload exceeds
/// [`MAX_FAVICON_BYTES`].
pub fn fetch(icon_url: &net::Url, options: &render::RenderOptions) -> Option<Vec<u8>> {
    match icon_url.scheme() {
        net::url::Scheme::File => {
            let path = std::path::PathBuf::from(net::url::percent_decode(icon_url.path()));
            let metadata = std::fs::metadata(&path).ok()?;
            if metadata.len() > MAX_FAVICON_BYTES as u64 {
                return None;
            }
            std::fs::read(path).ok()
        }
        net::url::Scheme::Http | net::url::Scheme::Https => {
            if options.no_network {
                return None;
            }
            let response = net::fetch(icon_url, &options.fetch_options).ok()?;
            if response.status != 200 || response.body.len() > MAX_FAVICON_BYTES {
                return None;
            }
            Some(response.body)
        }
        net::url::Scheme::Data => None,
    }
}

/// Builds the tab favicon for a rendered page: decoded `<link rel=icon>`/`favicon.ico`
/// when available, otherwise the deterministic origin dot.
pub fn for_page(
    document: &Document,
    document_url: &net::Url,
    options: &render::RenderOptions,
) -> Favicon {
    let key = origin_key(document_url);
    let Some(icon_url) = icon_url(document, document_url) else {
        return Favicon::fallback(&key);
    };
    let Some(bytes) = fetch(&icon_url, options) else {
        return Favicon::fallback(&key);
    };
    Favicon::from_bytes(&bytes, &key)
}

/// Stable cache key for a page (scheme + host + port, or `file` directory).
pub fn origin_key(url: &net::Url) -> String {
    match url.scheme() {
        net::url::Scheme::Http | net::url::Scheme::Https => {
            let mut key = url.scheme().as_str().to_owned();
            key.push_str("://");
            key.push_str(url.host());
            if let Some(port) = url.port_or_default() {
                key.push(':');
                key.push_str(&port.to_string());
            }
            key
        }
        net::url::Scheme::File => {
            let path = net::url::percent_decode(url.path());
            let parent = std::path::Path::new(&path)
                .parent()
                .map_or_else(|| "/".to_owned(), |p| p.display().to_string());
            format!("file://{parent}")
        }
        net::url::Scheme::Data => "data:".to_owned(),
    }
}

/// Deterministic color derived from a key (FNV-1a; kept away from near-white/near-black).
pub fn color_for(key: &str) -> [u8; 3] {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in key.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let r = 48 + (hash & 0x7F) as u8;
    let g = 48 + ((hash >> 8) & 0x7F) as u8;
    let b = 48 + ((hash >> 16) & 0x7F) as u8;
    [r, g, b]
}

/// Decodes image bytes and nearest-neighbour resizes them to 16x16 RGBA8.
pub fn decode_icon(bytes: &[u8]) -> Option<[u8; (ICON_SIZE * ICON_SIZE * 4) as usize]> {
    let image = image::decode(bytes).ok()?;
    if image.width == 0 || image.height == 0 {
        return None;
    }
    let expected = (image.width as usize)
        .checked_mul(image.height as usize)?
        .checked_mul(4)?;
    if image.data.len() < expected {
        return None;
    }
    let mut pixels = [0u8; (ICON_SIZE * ICON_SIZE * 4) as usize];
    for y in 0..ICON_SIZE {
        let source_y = (y as u64 * image.height as u64 / ICON_SIZE as u64) as u32;
        let source_y = source_y.min(image.height.saturating_sub(1));
        for x in 0..ICON_SIZE {
            let source_x = (x as u64 * image.width as u64 / ICON_SIZE as u64) as u32;
            let source_x = source_x.min(image.width.saturating_sub(1));
            let offset = ((source_y as usize) * (image.width as usize) + source_x as usize) * 4;
            if offset + 4 > image.data.len() {
                return None;
            }
            let target = ((y as usize) * (ICON_SIZE as usize) + x as usize) * 4;
            pixels[target..target + 4].copy_from_slice(&image.data[offset..offset + 4]);
        }
    }
    Some(pixels)
}

/// Maximum number of cached favicons.
const MAX_FAVICON_CACHE_ENTRIES: usize = 64;

/// In-memory favicon cache keyed by [`origin_key`].
#[derive(Clone, Debug, Default)]
pub struct FaviconCache {
    entries: std::collections::HashMap<String, Favicon>,
    order: std::collections::VecDeque<String>,
}

impl FaviconCache {
    /// Creates an empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of cached origins.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` when no origin is cached.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Looks up a cached favicon.
    pub fn get(&self, key: &str) -> Option<Favicon> {
        self.entries.get(key).copied()
    }

    /// Inserts a favicon, evicting the oldest entry when the cap is reached.
    pub fn insert(&mut self, key: String, favicon: Favicon) -> Option<Favicon> {
        if self.entries.len() >= MAX_FAVICON_CACHE_ENTRIES
            && !self.entries.contains_key(&key)
            && let Some(oldest) = self.order.pop_front()
        {
            self.entries.remove(&oldest);
        }
        self.order.push_back(key.clone());
        self.entries.insert(key, favicon)
    }

    /// Returns the cached favicon or computes/stores it with `load`.
    pub fn get_or_insert_with(&mut self, key: &str, load: impl FnOnce() -> Favicon) -> Favicon {
        if let Some(cached) = self.entries.get(key) {
            return *cached;
        }
        let favicon = load();
        self.insert(key.to_owned(), favicon);
        favicon
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(input: &str) -> net::Url {
        net::Url::parse(input).expect("url")
    }

    fn document_with(html: &str) -> Document {
        html::parse_document(html, false)
    }

    #[test]
    fn link_rel_icon_is_extracted_and_resolved() {
        let document = document_with(
            "<html><head><link rel=\"shortcut icon\" href=\"icons/app.png\"></head></html>",
        );
        let url = icon_url(&document, &page("https://example.test/dir/page.html")).expect("icon");
        assert_eq!(url.as_str(), "https://example.test/dir/icons/app.png");
    }

    #[test]
    fn missing_link_falls_back_to_favicon_ico() {
        let document = document_with("<html><head></head></html>");
        let url = icon_url(&document, &page("https://example.test/dir/")).expect("favicon.ico");
        assert_eq!(url.as_str(), "https://example.test/dir/favicon.ico");
    }

    #[test]
    fn fallback_color_is_deterministic_and_distinct() {
        let a = color_for("https://a.test");
        assert_eq!(a, color_for("https://a.test"));
        assert_ne!(a, color_for("https://b.test"));
        assert!(a.iter().all(|channel| *channel >= 48));
    }

    #[test]
    fn decode_icon_resizes_to_16x16_nearest() {
        // 2x2 red/green/blue/white PNG generated by the image fixture generator.
        let bytes = include_bytes!("../../../crates/image/tests/fixtures/png/rgba8.png");
        let pixels = decode_icon(bytes).expect("decode");
        assert_eq!(pixels.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
        assert!(pixels.iter().skip(3).step_by(4).any(|alpha| *alpha > 0));
    }

    #[test]
    fn corrupt_bytes_fall_back_to_dot() {
        let favicon = Favicon::from_bytes(b"not an image", "https://example.test");
        assert!(favicon.pixels.is_none());
        assert_eq!(favicon.color, color_for("https://example.test"));
    }

    #[test]
    fn origin_keys_are_stable() {
        assert_eq!(
            origin_key(&page("https://example.test:8443/a/b.html")),
            "https://example.test:8443"
        );
        assert!(origin_key(&page("file:///tmp/dir/page.html")).starts_with("file:///tmp"));
    }

    #[test]
    fn cache_stores_and_reuses_entries() {
        let mut cache = FaviconCache::new();
        assert!(cache.is_empty());
        let favicon = Favicon::fallback("https://example.test");
        assert_eq!(cache.get_or_insert_with("k", || favicon), favicon);
        assert_eq!(
            cache.get_or_insert_with("k", || Favicon::fallback("other")),
            favicon
        );
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn file_favicon_is_fetched_with_cap() {
        let temp = std::env::temp_dir().join("megi-favicon-test");
        let _ = std::fs::create_dir_all(&temp);
        let path = temp.join("favicon.ico");
        std::fs::write(&path, b"icon-bytes").expect("write");
        let url = page(&format!("file://{}/page.html", temp.display()));
        let options = render::RenderOptions::default();
        let fetched = fetch(
            &icon_url(&document_with("<html></html>"), &url).unwrap(),
            &options,
        );
        assert_eq!(fetched.as_deref(), Some(b"icon-bytes".as_slice()));
        std::fs::write(&path, vec![0u8; MAX_FAVICON_BYTES + 1]).expect("write big");
        let fetched = fetch(
            &icon_url(&document_with("<html></html>"), &url).unwrap(),
            &options,
        );
        assert!(fetched.is_none());
        let _ = std::fs::remove_dir_all(temp);
    }
}
