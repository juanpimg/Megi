//! Hit-region bookkeeping must be additive (no PNG change).

use super::common::*;

// ---------------------------------------------------------------------------
// Hit regions (follow-up A)
// ---------------------------------------------------------------------------

#[test]
fn hit_regions_are_exposed_without_changing_the_png() {
    let page = "data:text/html,<body><a href=\"next.html\">link</a><p>text</p></body>";
    let output = render_url(page, &options(200, 100)).expect("render");
    assert!(!output.hit_regions.is_empty(), "no hit regions recorded");
    assert_eq!(output.stats.hit_regions, output.hit_regions.len());
    assert!(output.stats.hit_regions > 1, "expected several primitives");
    // The same display list produces byte-identical PNG bytes with/without regions: the
    // regions are additive bookkeeping, verified by the golden md5 tests elsewhere.
    let document = &output.document;
    let mut found_link = false;
    for region in &output.hit_regions {
        let cx = region.rect.x + region.rect.width / 2.0;
        let cy = region.rect.y + region.rect.height / 2.0;
        if let Some(link) = megi_paint::find_link_at(document, &output.hit_regions, cx, cy) {
            assert_eq!(link.href, "next.html");
            found_link = true;
            break;
        }
    }
    assert!(
        found_link,
        "megi_paint::find_link_at did not find the anchor"
    );
}
