//! R216: byte-identical PNG region/row-cache encoding.

use css::Color;
use layout::Rect;
use paint::{Pixmap, PngRegionEncoder, encode_png_regions};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 48;

fn flat() -> Pixmap {
    let mut pixmap = Pixmap::new(WIDTH, HEIGHT);
    for pixel in &mut pixmap.pixels {
        *pixel = Color::new(0x33, 0x66, 0x99, 0xFF);
    }
    pixmap
}

fn photo() -> Pixmap {
    let mut pixmap = Pixmap::new(WIDTH, HEIGHT);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let r = ((x * 7 + y * 3) % 256) as u8;
            let g = ((x * 2 + y * 11) % 256) as u8;
            let b = ((x * 5 + y * 5) % 256) as u8;
            pixmap.set_pixel(x, y, Color::new(r, g, b, 0xFF));
        }
    }
    pixmap
}

fn alpha() -> Pixmap {
    let mut pixmap = Pixmap::new(WIDTH, HEIGHT);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let a = ((x * 13 + y * 17) % 256) as u8;
            pixmap.set_pixel(
                x,
                y,
                Color::new((x * 4) as u8, (y * 5) as u8, ((x + y) * 3) as u8, a),
            );
        }
    }
    pixmap
}

fn whole() -> Rect {
    Rect::new(0.0, 0.0, WIDTH as f32, HEIGHT as f32)
}

fn band(y: f32, height: f32) -> Rect {
    Rect::new(0.0, y, WIDTH as f32, height)
}

#[test]
fn fresh_region_encoder_matches_full_png_flat() {
    let pixmap = flat();
    assert_eq!(
        encode_png_regions(&pixmap, &[whole()]),
        pixmap.to_png(),
        "flat"
    );
    assert_eq!(encode_png_regions(&pixmap, &[]), pixmap.to_png());
}

#[test]
fn fresh_region_encoder_matches_full_png_photo() {
    let pixmap = photo();
    assert_eq!(
        encode_png_regions(&pixmap, &[whole()]),
        pixmap.to_png(),
        "photo"
    );
    assert_eq!(
        encode_png_regions(&pixmap, &[band(0.0, 8.0), band(20.0, 5.0)]),
        pixmap.to_png()
    );
}

#[test]
fn fresh_region_encoder_matches_full_png_alpha() {
    let pixmap = alpha();
    assert_eq!(
        encode_png_regions(&pixmap, &[whole()]),
        pixmap.to_png(),
        "alpha"
    );
}

#[test]
fn cached_encoder_matches_full_png_after_partial_redraw() {
    let mut pixmap = photo();
    let mut encoder = PngRegionEncoder::new();
    let first = encoder.encode_regions(&pixmap, &[whole()]);
    assert_eq!(first, pixmap.to_png(), "first frame");

    // Redraw a band + one isolated row.
    for y in 10..18 {
        for x in 0..WIDTH {
            pixmap.set_pixel(x, y, Color::new(255, 0, 0, 255));
        }
    }
    for x in 0..WIDTH {
        pixmap.set_pixel(x, 30, Color::new(0, 255, 0, 255));
    }
    let dirty = [band(10.0, 8.0), band(30.0, 1.0)];
    let second = encoder.encode_regions(&pixmap, &dirty);
    assert_eq!(second, pixmap.to_png(), "partial redraw");
}

#[test]
fn cached_encoder_reuses_png_when_nothing_changed() {
    let pixmap = alphabet_photo();
    let mut encoder = PngRegionEncoder::new();
    let first = encoder.encode_regions(&pixmap, &[whole()]);
    let second = encoder.encode_regions(&pixmap, &[]);
    assert_eq!(second, first);
    assert_eq!(second, pixmap.to_png());
}

#[test]
fn dirty_hint_is_only_an_optimisation_not_a_correctness_requirement() {
    let mut pixmap = alpha();
    let mut encoder = PngRegionEncoder::new();
    let _ = encoder.encode_regions(&pixmap, &[whole()]);
    // Mutate without telling the encoder (empty hint): content comparison must
    // still detect the changed rows and produce the full-encode bytes.
    for y in 0..HEIGHT {
        pixmap.set_pixel(5, y, Color::new(7, 8, 9, 255));
    }
    let encoded = encoder.encode_regions(&pixmap, &[]);
    assert_eq!(encoded, pixmap.to_png(), "missing dirty hint still correct");
}

#[test]
fn alpha_is_preserved_across_region_encodes() {
    let mut pixmap = alpha();
    let mut encoder = PngRegionEncoder::new();
    let first = encoder.encode_regions(&pixmap, &[whole()]);
    assert_eq!(first, pixmap.to_png());
    for y in 0..HEIGHT {
        pixmap.set_pixel(WIDTH - 1, y, Color::new(0, 0, 0, 0));
    }
    let second = encoder.encode_regions(&pixmap, &[band(0.0, HEIGHT as f32)]);
    assert_eq!(second, pixmap.to_png());
}

fn alphabet_photo() -> Pixmap {
    photo()
}

#[test]
fn large_fixture_region_encoding_is_byte_identical() {
    const W: u32 = 800;
    const H: u32 = 600;
    let mut pixmap = Pixmap::new(W, H);
    for y in 0..H {
        for x in 0..W {
            let r = ((x * 7 + y * 3) % 256) as u8;
            let g = ((x * 2 + y * 11) % 256) as u8;
            let b = ((x * 5 + y * 5) % 256) as u8;
            pixmap.set_pixel(x, y, Color::new(r, g, b, 255));
        }
    }
    let mut encoder = PngRegionEncoder::new();
    let first = encoder.encode_regions(&pixmap, &[Rect::new(0.0, 0.0, W as f32, H as f32)]);
    assert_eq!(first, pixmap.to_png(), "large first frame");
    for pct in [10u32, 25, 50, 100] {
        let rows = ((u64::from(H) * u64::from(pct)) / 100) as usize;
        let color = if pct % 2 == 0 {
            Color::new(11, 22, 33, 255)
        } else {
            Color::new(44, 55, 66, 255)
        };
        pixmap.pixels[..rows * W as usize].fill(color);
        let encoded =
            encoder.encode_regions(&pixmap, &[Rect::new(0.0, 0.0, W as f32, rows as f32)]);
        assert_eq!(encoded, pixmap.to_png(), "large {pct}% dirty");
    }
}

#[test]
fn dirty_rects_outside_the_pixmap_are_ignored() {
    let pixmap = photo();
    let expected = pixmap.to_png();
    let rects = [
        Rect::new(0.0, 120.0, 10.0, 10.0), // below the 48-row pixmap
        Rect::new(0.0, 1e30, 10.0, 10.0),  // absurdly far below
        Rect::new(0.0, -50.0, 10.0, 10.0), // above the pixmap
        Rect::new(0.0, 40.0, 4.0, 100.0),  // starts inside, ends past the bottom
    ];

    assert_eq!(encode_png_regions(&pixmap, &rects), expected);

    let mut encoder = PngRegionEncoder::new();
    assert_eq!(encoder.encode_regions(&pixmap, &[]), expected);
    assert_eq!(encoder.encode_regions(&pixmap, &rects), expected);
}
