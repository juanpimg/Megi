use super::*;

#[test]
fn test_group_inheritance_and_style_attribute() {
    let image = render_at(
        &svg_doc(
            10.0,
            10.0,
            "<g fill='#00ff00' style='stroke:black;stroke-width:2'>\
             <rect x='1' y='1' width='8' height='8'/></g>",
        ),
        10,
        10,
    );
    assert_eq!(pixel(&image, 5, 5), [0, 255, 0, 255]);
    assert_eq!(pixel(&image, 1, 5)[3], 255); // stroke on the border
}

#[test]
fn currentcolor_uses_inherited_color() {
    // `currentColor` must resolve to the inherited CSS `color`, not to black (#1276).
    let image = render_at(
        &svg_doc(
            8.0,
            8.0,
            "<g color='red'><rect width='8' height='8' fill='currentColor'/></g>",
        ),
        8,
        8,
    );
    assert_eq!(pixel(&image, 4, 4), [255, 0, 0, 255]);
}

#[test]
fn display_none_skips_subtree_and_visibility_can_be_overridden() {
    let image = render_at(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
            <rect width="20" height="20" fill="#ff0000" display="none"/>
            <g visibility="hidden">
                <rect width="10" height="10" fill="#00ff00"/>
                <rect x="10" width="10" height="10" fill="#0000ff" visibility="visible"/>
            </g>
        </svg>"##,
        20,
        20,
    );
    assert_eq!(
        pixel(&image, 2, 2),
        [0, 0, 0, 0],
        "display=none must skip (#37)"
    );
    assert_eq!(
        pixel(&image, 5, 5),
        [0, 0, 0, 0],
        "inherited hidden must skip (#37)"
    );
    assert_eq!(
        pixel(&image, 15, 5),
        [0, 0, 255, 255],
        "visible override must paint"
    );
}
