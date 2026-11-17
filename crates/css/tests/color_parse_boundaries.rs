//! `Color::parse` must never slice inside a UTF-8 code point (#1445).

use css::Color;

#[test]
fn non_ascii_input_does_not_panic_and_is_not_a_color() {
    // `é` occupies bytes 1..3 and `€` bytes 3..6, so a naive `&s[..4]` split index 4 inside `€`.
    assert_eq!(Color::parse("aé€"), None);
    // Full-width capitals: each is three bytes, so byte 4 is inside the second character.
    assert_eq!(Color::parse("ＲＧＢ(1,2,3)"), None);
    assert_eq!(Color::parse("rgb ▒(1,2,3)"), None);
}

#[test]
fn ascii_color_functions_still_parse() {
    let rgb = Color::parse("rgb(10, 20, 30)").expect("rgb() parses");
    assert_eq!((rgb.r, rgb.g, rgb.b), (10, 20, 30));
    let rgba = Color::parse("RGBA(1, 2, 3, 0.5)").expect("rgba() parses");
    assert_eq!((rgba.r, rgba.g, rgba.b), (1, 2, 3));
    assert_eq!(rgba.a, 128);
    assert!(Color::parse("#0f0").is_some());
}
