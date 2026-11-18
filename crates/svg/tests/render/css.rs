use super::*;

#[test]
fn test_css_style_class_id_and_inline_cascade() {
    let image = render_at(
        &svg_doc(
            30.0,
            10.0,
            "<style>rect { fill: #ff0000 } .a { fill: #00ff00 } #b { fill: #0000ff }</style>\
             <rect x='0' width='10' height='10' class='a'/>\
             <rect x='10' width='10' height='10' id='b'/>\
             <rect x='20' width='10' height='10' class='a' style='fill:#ffff00'/>",
        ),
        30,
        10,
    );
    assert_eq!(pixel(&image, 5, 5), [0, 255, 0, 255]);
    assert_eq!(pixel(&image, 15, 5), [0, 0, 255, 255]);
    assert_eq!(pixel(&image, 25, 5), [255, 255, 0, 255]);
}

#[test]
fn test_css_descendant_selector_overrides_presentation_attribute() {
    let image = render_at(
        &svg_doc(
            10.0,
            10.0,
            "<style>g rect { fill: #00ff00 }</style>\
             <g><rect width='10' height='10' fill='#ff0000'/></g>",
        ),
        10,
        10,
    );
    assert_eq!(pixel(&image, 5, 5), [0, 255, 0, 255]);
}

#[test]
fn test_css_inside_cdata_and_inherited_by_group() {
    let image = render_at(
        &svg_doc(
            10.0,
            10.0,
            "<style><![CDATA[.card { fill: #3366cc }]]></style>\
             <g class='card'><rect width='10' height='10'/></g>",
        ),
        10,
        10,
    );
    assert_eq!(pixel(&image, 5, 5), [0x33, 0x66, 0xcc, 255]);
}
