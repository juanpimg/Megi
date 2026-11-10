//! `<clipPath>`, `<mask>` and basic filter coverage.

use svg::{SvgOptions, render_svg};

fn render(source: &str) -> svg::SvgImage {
    render_svg(source.as_bytes(), &SvgOptions::default()).expect("render")
}

fn px(image: &svg::SvgImage, x: u32, y: u32) -> [u8; 4] {
    image.pixel(x, y).expect("pixel")
}

#[test]
fn clip_path_user_space_recorta_fuera() {
    let image = render(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="40">
            <defs><clipPath id="c"><rect x="10" y="10" width="20" height="20"/></clipPath></defs>
            <rect width="40" height="40" fill="red" clip-path="url(#c)"/>
        </svg>"##,
    );
    assert_eq!(px(&image, 5, 5)[3], 0, "outside clip is transparent");
    assert_eq!(px(&image, 20, 20), [255, 0, 0, 255]);
}

#[test]
fn clip_path_object_bounding_box() {
    let image = render(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="40">
            <defs><clipPath id="c" clipPathUnits="objectBoundingBox">
                <circle cx="0.5" cy="0.5" r="0.4"/>
            </clipPath></defs>
            <rect x="10" y="10" width="20" height="20" fill="green" clip-path="url(#c)"/>
        </svg>"##,
    );
    assert_eq!(px(&image, 20, 20), [0, 128, 0, 255]);
    assert_eq!(
        px(&image, 11, 11)[3],
        0,
        "bbox corners are outside the circle"
    );
}

#[test]
fn clip_path_on_group_applies_to_children() {
    let image = render(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="40">
            <defs><clipPath id="c"><rect x="0" y="0" width="20" height="40"/></clipPath></defs>
            <g clip-path="url(#c)">
                <rect width="40" height="40" fill="blue"/>
                <circle cx="20" cy="20" r="10" fill="yellow"/>
            </g>
        </svg>"##,
    );
    assert_eq!(px(&image, 5, 20), [0, 0, 255, 255]);
    assert_eq!(px(&image, 30, 20)[3], 0, "right half clipped");
}

#[test]
fn cyclic_clip_references_do_not_panic() {
    let image = render(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
            <defs><clipPath id="a"><use href="#a"/></clipPath></defs>
            <rect width="20" height="20" fill="red" clip-path="url(#a)"/>
        </svg>"##,
    );
    // The cycle is dropped: the clip has no geometry, so nothing is painted.
    assert_eq!(px(&image, 10, 10)[3], 0);
}

#[test]
fn mask_white_keeps_and_black_clears() {
    let white = render(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
            <defs><mask id="m"><rect width="20" height="20" fill="white"/></mask></defs>
            <rect width="20" height="20" fill="blue" mask="url(#m)"/>
        </svg>"##,
    );
    assert_eq!(px(&white, 10, 10), [0, 0, 255, 255]);
    let black = render(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
            <defs><mask id="m"><rect width="20" height="20" fill="black"/></mask></defs>
            <rect width="20" height="20" fill="blue" mask="url(#m)"/>
        </svg>"##,
    );
    assert_eq!(px(&black, 10, 10)[3], 0);
}

#[test]
fn mask_half_region_limits_coverage() {
    let image = render(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
            <defs><mask id="m" x="0" y="0" width="0.5" height="1">
                <rect width="20" height="20" fill="white"/>
            </mask></defs>
            <rect width="20" height="20" fill="red" mask="url(#m)"/>
        </svg>"##,
    );
    assert_eq!(px(&image, 5, 10)[3], 255);
    assert_eq!(px(&image, 15, 10)[3], 0);
}

#[test]
fn gaussian_blur_softens_edges() {
    let image = render(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="60" height="60">
            <defs><filter id="f"><feGaussianBlur stdDeviation="2"/></filter></defs>
            <rect x="25" y="25" width="10" height="10" fill="black" filter="url(#f)"/>
        </svg>"##,
    );
    assert_eq!(px(&image, 30, 30), [0, 0, 0, 255], "center stays opaque");
    let edge = px(&image, 24, 30)[3];
    assert!(
        edge > 0 && edge < 255,
        "edge is partially transparent: {edge}"
    );
    assert_eq!(px(&image, 0, 0)[3], 0);
}

#[test]
fn fe_offset_moves_the_shape() {
    let image = render(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20">
            <defs><filter id="f" filterUnits="userSpaceOnUse" x="0" y="0" width="40" height="20">
                <feOffset dx="10" dy="0"/></filter></defs>
            <rect width="10" height="10" fill="red" filter="url(#f)"/>
        </svg>"##,
    );
    assert_eq!(px(&image, 2, 5)[3], 0, "original position is cleared");
    assert_eq!(px(&image, 15, 5), [255, 0, 0, 255]);
}

#[test]
fn fe_flood_and_composite_over() {
    let image = render(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
            <defs><filter id="f">
                <feFlood flood-color="lime" flood-opacity="1"/>
                <feComposite operator="over"/>
            </filter></defs>
            <rect width="20" height="20" fill="black" filter="url(#f)"/>
        </svg>"##,
    );
    assert_eq!(px(&image, 10, 10), [0, 255, 0, 255]);
}

#[test]
fn excessive_blur_is_bounded() {
    let image = render(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32">
            <defs><filter id="f">
                <feGaussianBlur stdDeviation="10000"/>
                <feGaussianBlur stdDeviation="10000"/>
            </filter></defs>
            <circle cx="16" cy="16" r="6" fill="red" filter="url(#f)"/>
        </svg>"##,
    );
    assert!(px(&image, 16, 16)[3] > 0);
}

#[test]
fn mask_gradient_gives_partial_coverage() {
    let image = render(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20">
            <defs>
                <linearGradient id="g"><stop offset="0" stop-color="black"/>
                    <stop offset="1" stop-color="white"/></linearGradient>
                <mask id="m"><rect width="40" height="20" fill="url(#g)"/></mask>
            </defs>
            <rect width="40" height="20" fill="red" mask="url(#m)"/>
        </svg>"##,
    );
    let left = px(&image, 1, 10)[3];
    let right = px(&image, 38, 10)[3];
    assert!(
        left < right,
        "gradient mask is dark on the left: {left} < {right}"
    );
}

#[test]
fn fixtures_render_with_effects() {
    let clip = render(std::str::from_utf8(include_bytes!("fixtures/clip_path.svg")).unwrap());
    assert_eq!((clip.width, clip.height), (120, 80));
    assert_eq!(px(&clip, 0, 0), [255, 255, 255, 255]);
    assert!(px(&clip, 60, 40)[3] > 0);
    let mask = render(std::str::from_utf8(include_bytes!("fixtures/mask.svg")).unwrap());
    assert_eq!((mask.width, mask.height), (120, 80));
    // The dark side of the gradient mask lets the white background through, while the
    // bright side shows the masked red rectangle.
    assert!(px(&mask, 2, 40)[1] > px(&mask, 117, 40)[1]);
    let filter = render(std::str::from_utf8(include_bytes!("fixtures/filter_blur.svg")).unwrap());
    assert_eq!((filter.width, filter.height), (120, 80));
    assert!(px(&filter, 45, 40)[3] > 0);
    assert!(px(&filter, 2, 2) == [255, 255, 255, 255]);
}
