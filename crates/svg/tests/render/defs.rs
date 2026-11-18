use super::*;

#[test]
fn test_use_references_a_definition() {
    let image = render_at(
        &svg_doc(
            10.0,
            10.0,
            "<defs><rect id='r' width='2' height='2' fill='#0000ff'/></defs>\
             <use href='#r' x='4' y='4'/>",
        ),
        10,
        10,
    );
    assert_eq!(pixel(&image, 5, 5), [0, 0, 255, 255]);
    assert_eq!(pixel(&image, 0, 0), [0, 0, 0, 0]);
}

#[test]
fn symbol_instantiated_via_use_renders() {
    // A <symbol> is not painted directly; <use href="#s"> must instantiate its children
    // through the symbol's viewBox mapping (#38).
    let image = render_at(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 20 20">
            <defs><symbol id="s" viewBox="0 0 10 10"><rect width="10" height="10" fill="#00ff00"/></symbol></defs>
            <use href="#s" x="0" y="0" width="20" height="20"/>
        </svg>"##,
        20,
        20,
    );
    assert_eq!(pixel(&image, 5, 5), [0, 255, 0, 255]);
}
