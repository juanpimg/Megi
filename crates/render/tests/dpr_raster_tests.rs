//! DPR raster boundary: physical frame, CSS viewport/hit regions (ADR: DPR-RASTER-BOUNDARY).

use megi_render::{RenderOptions, ScriptingMode, Viewport, render_url};

const PAGE: &str = "data:text/html,<style>body{margin:0}</style><h1>Hello Megi</h1><a href='https://example.com'>link</a>";

fn options(raster_scale: f32) -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(800, 600).expect("viewport"),
        scripting: ScriptingMode::Disabled,
        raster_scale,
        ..RenderOptions::default()
    }
}

fn png_size(png: &[u8]) -> (u32, u32) {
    assert_eq!(&png[1..4], b"PNG", "PNG signature");
    assert_eq!(&png[12..16], b"IHDR", "IHDR chunk");
    (
        u32::from_be_bytes([png[16], png[17], png[18], png[19]]),
        u32::from_be_bytes([png[20], png[21], png[22], png[23]]),
    )
}

#[test]
fn dpr_raster_frame_is_physical_while_display_list_and_hits_stay_css() {
    let css = render_url(PAGE, &options(1.0)).expect("scale 1 render");
    let hidpi = render_url(PAGE, &options(1.25)).expect("scale 1.25 render");
    assert_eq!((css.pixmap.width, css.pixmap.height), (800, 600));
    assert_eq!((hidpi.pixmap.width, hidpi.pixmap.height), (1000, 750));
    assert_eq!(png_size(&hidpi.png), (1000, 750));
    assert_eq!(
        hidpi.display_list, css.display_list,
        "display list must stay in CSS coordinates"
    );
    assert!(!hidpi.hit_regions.is_empty(), "page has hit regions");
    assert_eq!(hidpi.hit_regions.len(), css.hit_regions.len());
    for (scaled, plain) in hidpi.hit_regions.iter().zip(&css.hit_regions) {
        // NodeId generations differ between independent renders (and differ
        // with the `js` feature); the CSS geometry/kind contract is what must
        // not change with raster scale.
        assert_eq!(scaled.rect, plain.rect, "hit rect stays in CSS px");
        assert_eq!(scaled.kind, plain.kind);
    }
}

#[test]
fn dpr_raster_scale_one_is_byte_identical() {
    let baseline = render_url(PAGE, &options(1.0)).expect("baseline");
    let explicit = render_url(PAGE, &options(1.0)).expect("explicit scale 1");
    assert_eq!(baseline.png, explicit.png);
}
