//! The raster crate must expose a deterministic DisplayList -> PNG pipeline.

use megi_display::DisplayList;
use megi_raster::{Pixmap, rasterize_display_list};

#[test]
fn empty_display_list_rasterizes_to_a_valid_png() {
    let list = DisplayList::new();
    let pixmap: Pixmap = rasterize_display_list(&list, 4, 4);
    assert_eq!(pixmap.width, 4);
    assert_eq!(pixmap.height, 4);
    let png = pixmap.to_png();
    assert_eq!(&png[1..4], b"PNG");
}
