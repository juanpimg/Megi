//! Regression for #2373: nested browsing contexts were dropped when the page
//! used a non-identity DPR. The child was rasterised at the inherited
//! `raster_scale`, then `RgbaImage::new(logical_width, logical_height, bytes)`
//! rejected the physically-sized buffer as a `raster mismatch`. The child must
//! rasterise at CSS scale (1.0); the parent pipeline scales the frame's display
//! item rect via `RasterScale::apply`, keeping the image payload logical-sized.

use megi_css::Color;
use megi_display::DisplayItem;
use megi_render::{RenderOptions, ScriptingMode, Viewport, render_url};

const PAGE: &str = "data:text/html,<html><body style='margin:0'>\
    <iframe width='40' height='20' srcdoc='<body style=\"margin:0;background:red\"></body>'></iframe>\
    </body></html>";

const RED: Color = Color::new(0xFF, 0x00, 0x00, 0xFF);

fn options(raster_scale: f32) -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(400, 300).expect("viewport"),
        scripting: ScriptingMode::Disabled,
        raster_scale,
        ..RenderOptions::default()
    }
}

fn count_color(pixmap: &megi_raster::Pixmap, color: Color) -> usize {
    pixmap
        .pixels
        .iter()
        .filter(|pixel| **pixel == color)
        .count()
}

#[test]
fn iframe_survives_non_identity_raster_scale() {
    let output = render_url(PAGE, &options(2.0)).expect("scale 2 render");

    // Physical frame, CSS-sized display list (DPR boundary contract).
    assert_eq!(
        (output.pixmap.width, output.pixmap.height),
        (800, 600),
        "frame must be physical"
    );

    // #2373: the child must not be discarded as a raster mismatch.
    assert!(
        !output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("raster mismatch")),
        "child frame must not be dropped at DPR 2: {:?}",
        output.stats.warnings
    );

    // The frame image payload stays logical-sized (40x20); only its rect is
    // scaled by the parent pipeline. `object-fit: fill` keeps the rect equal to
    // the content box, so the image dimensions are the contract here.
    let frame_images: Vec<&DisplayItem> = output
        .display_list
        .items
        .iter()
        .filter(|item| matches!(item, DisplayItem::Image { .. }))
        .collect();
    assert!(
        frame_images.iter().any(|item| match item {
            DisplayItem::Image { image, .. } => image.width == 40 && image.height == 20,
            _ => false,
        }),
        "iframe image must be logical-sized 40x20, got {:?}",
        frame_images
            .iter()
            .map(|item| match item {
                DisplayItem::Image { rect, image } =>
                    (rect.width, rect.height, image.width, image.height),
                _ => (0.0, 0.0, 0, 0),
            })
            .collect::<Vec<_>>()
    );

    // Child content is painted (red background from srcdoc).
    assert!(
        count_color(&output.pixmap, RED) > 100,
        "child red background must paint at DPR 2"
    );
}
