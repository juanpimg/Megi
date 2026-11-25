//! Browser raster boundary: physical frames with CSS hit regions (ADR: DPR-RASTER-BOUNDARY).

use megi_browser::worker::{RenderRequest, render_document};

const PAGE: &str = "data:text/html,<style>body{margin:0}</style><h1>Hello Megi</h1><a href='https://example.com'>link</a>";

fn request(raster_scale: f32) -> RenderRequest {
    RenderRequest {
        generation: 1,
        input: PAGE.to_owned(),
        css_width: 1280,
        css_height: 720,
        raster_scale,
        base_override: None,
        scripting: false,
        find_query: None,
        host_events: Vec::new(),
        host_dom_events: Vec::new(),
        form_post: None,
    }
}

#[test]
fn dpr_raster_worker_frame_is_physical_and_hit_regions_are_css() {
    let base = render_document(&request(1.0)).expect("scale 1 render");
    let hidpi = render_document(&request(1.25)).expect("scale 1.25 render");
    assert_eq!((base.width, base.height), (1280, 720));
    assert_eq!((hidpi.width, hidpi.height), (1600, 900));
    assert_eq!(
        hidpi.pixels.len(),
        (1600_usize) * 900,
        "framebuffer must hold physical pixels"
    );
    assert_eq!(
        hidpi.content_height, base.content_height,
        "content height is CSS and must not be scaled"
    );
    assert!(!base.links.is_empty(), "page has a link hit region");
    assert_eq!(hidpi.links.len(), base.links.len());
    for (scaled, plain) in hidpi.links.iter().zip(&base.links) {
        // NodeId generations differ between the two independent renders; the
        // CSS geometry/href contract is what must not change with raster scale.
        assert_eq!(scaled.rect, plain.rect, "link rect stays in CSS px");
        assert_eq!(scaled.href, plain.href);
        assert_eq!(scaled.is_blocker, plain.is_blocker);
    }
}
