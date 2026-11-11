//! R201: `RenderOptions::encode_png` skips the eager page PNG so overlay callers
//! (`--chrome`/`--find`) encode exactly once at the end, with byte-identical output.
use render::{RenderOptions, Viewport, render_url};

fn page() -> String {
    let html = "<!DOCTYPE html><html><body><h1>encode png</h1><p style=\"color:#c33\">text</p></body></html>";
    let encoded: String = html
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-_.~ ".contains(&byte) {
                if byte == b' ' {
                    "%20".to_string()
                } else {
                    (byte as char).to_string()
                }
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect();
    format!("data:text/html,{encoded}")
}

fn options(encode_png: bool) -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(200, 120).expect("viewport"),
        encode_png,
        ..RenderOptions::default()
    }
}

#[test]
fn skipped_png_leaves_the_pixmap_intact_and_lazy_encode_is_identical() {
    let eager = render_url(&page(), &options(true)).expect("eager render");
    let mut lazy = render_url(&page(), &options(false)).expect("lazy render");
    assert!(!eager.png.is_empty(), "default pipeline encodes eagerly");
    assert!(lazy.png.is_empty(), "encode_png=false skips the PNG");
    assert_eq!(lazy.pixmap.pixels, eager.pixmap.pixels, "pixels unchanged");
    assert_eq!(lazy.pixmap.width, eager.pixmap.width);
    assert_eq!(lazy.pixmap.height, eager.pixmap.height);
    lazy.ensure_png();
    assert_eq!(lazy.png, eager.png, "lazy encode is byte-identical");
    let again = lazy.png_bytes().to_vec();
    assert_eq!(again, lazy.png, "ensure_png is idempotent");
}

#[test]
fn skipping_png_does_not_change_dom_layout_or_stats() {
    let eager = render_url(&page(), &options(true)).expect("eager");
    let lazy = render_url(&page(), &options(false)).expect("lazy");
    assert_eq!(
        html::serialize_html(&eager.document),
        html::serialize_html(&lazy.document)
    );
    assert_eq!(
        eager.display_list.items.len(),
        lazy.display_list.items.len()
    );
    assert_eq!(eager.stats.images_loaded, lazy.stats.images_loaded);
    assert_eq!(eager.stats.warnings, lazy.stats.warnings);
    assert_eq!(eager.stats.document_bytes, lazy.stats.document_bytes);
}
