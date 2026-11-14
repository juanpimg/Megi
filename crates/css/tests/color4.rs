//! CSS Color 4/5 parsing and sRGB conversion (R159).

use css::Color;

fn assert_color(actual: Option<Color>, expected: (u8, u8, u8), tolerance: u8, context: &str) {
    let color = actual.unwrap_or_else(|| panic!("{context}: expected a color"));
    let near = |a: u8, b: u8| a.abs_diff(b) <= tolerance;
    assert!(
        near(color.r, expected.0) && near(color.g, expected.1) && near(color.b, expected.2),
        "{context}: got {color:?}, expected ≈{expected:?}"
    );
}

#[test]
fn test_color_srgb_and_srgb_linear() {
    assert_color(
        Color::parse("color(srgb 1 0 0)"),
        (255, 0, 0),
        0,
        "srgb red",
    );
    assert_color(
        Color::parse("color(srgb 0.5 0.5 0.5)"),
        (128, 128, 128),
        1,
        "srgb gray",
    );
    assert_color(
        Color::parse("color(srgb-linear 1 0 0)"),
        (255, 0, 0),
        1,
        "linear red",
    );
    let alpha = Color::parse("color(srgb 1 0 0 / 50%)").expect("alpha");
    assert_eq!(alpha.a, 128);
    let clamped = Color::parse("color(srgb 2 -1 0.5 / 2)").expect("clamped");
    assert_eq!(
        (clamped.r, clamped.g, clamped.b, clamped.a),
        (255, 0, 128, 255)
    );
}

#[test]
fn test_color_p3_and_rec2020() {
    assert_color(
        Color::parse("color(display-p3 1 0 0)"),
        (255, 0, 0),
        1,
        "p3 red clamps to sRGB red",
    );
    assert_color(
        Color::parse("color(rec2020 0 1 0)"),
        (0, 255, 0),
        1,
        "rec2020 green",
    );
    assert_color(
        Color::parse("color(a98-rgb 0 0 1)"),
        (0, 0, 255),
        1,
        "a98 blue",
    );
    assert_color(
        Color::parse("color(prophoto-rgb 1 1 1)"),
        (255, 255, 255),
        2,
        "prophoto white",
    );
}

#[test]
fn test_color_xyz() {
    assert_color(
        Color::parse("color(xyz 0.4124 0.2126 0.0193)"),
        (255, 0, 0),
        2,
        "xyz red",
    );
    assert_color(
        Color::parse("color(xyz-d65 0.9505 1 1.0888)"),
        (255, 255, 255),
        2,
        "xyz white",
    );
    assert_color(
        Color::parse("color(xyz-d50 0.9643 1 0.8251)"),
        (255, 255, 255),
        2,
        "xyz-d50 white",
    );
}

#[test]
fn test_lab_and_lch() {
    assert_color(
        Color::parse("lab(100 0 0)"),
        (255, 255, 255),
        2,
        "lab white",
    );
    assert_color(Color::parse("lab(0 0 0)"), (0, 0, 0), 2, "lab black");
    assert_color(
        Color::parse("lab(50 0 0)"),
        (119, 119, 119),
        2,
        "lab mid gray",
    );
    assert_color(
        Color::parse("lab(50 50 0)"),
        Color::parse("lch(50 50 0)")
            .map(|c| (c.r, c.g, c.b))
            .expect("lch"),
        2,
        "lch polar form matches lab",
    );
    let alpha = Color::parse("lab(50 0 0 / 25%)").expect("alpha lab");
    assert_eq!(alpha.a, 64);
}

#[test]
fn test_oklab_and_oklch() {
    assert_color(
        Color::parse("oklab(1 0 0)"),
        (255, 255, 255),
        2,
        "oklab white",
    );
    assert_color(Color::parse("oklab(0 0 0)"), (0, 0, 0), 2, "oklab black");
    assert_color(
        Color::parse("oklab(0.5 0 0)"),
        (99, 99, 99),
        3,
        "oklab mid gray",
    );
    let lch = Color::parse("oklch(0.5 0.2 120)").expect("oklch");
    assert!(lch.g > lch.r, "oklch hue 120 is green-ish: {lch:?}");
    assert_color(
        Color::parse("oklch(0.5 0 0)"),
        Color::parse("oklab(0.5 0 0)")
            .map(|c| (c.r, c.g, c.b))
            .expect("oklab"),
        2,
        "neutral polar form",
    );
}

#[test]
fn test_hwb() {
    assert_color(Color::parse("hwb(0 0% 0%)"), (255, 0, 0), 1, "hwb red");
    assert_color(Color::parse("hwb(120 0% 0%)"), (0, 255, 0), 2, "hwb green");
    assert_color(
        Color::parse("hwb(0 100% 0%)"),
        (255, 255, 255),
        1,
        "hwb white",
    );
    assert_color(Color::parse("hwb(0 0% 100%)"), (0, 0, 0), 1, "hwb black");
    assert_color(
        Color::parse("hwb(0 50% 50%)"),
        (128, 128, 128),
        2,
        "hwb gray",
    );
}

#[test]
fn test_color_mix_srgb() {
    assert_color(
        Color::parse("color-mix(in srgb, red, blue)"),
        (128, 0, 128),
        1,
        "srgb mix",
    );
    assert_color(
        Color::parse("color-mix(in srgb, red 25%, blue)"),
        (64, 0, 191),
        2,
        "weighted mix",
    );
    assert_color(
        Color::parse("color-mix(in srgb, red 30%, blue 30%)"),
        (128, 0, 128),
        1,
        "both percentages normalize",
    );
    let transparent =
        Color::parse("color-mix(in srgb, #ff0000, transparent)").expect("transparent mix");
    assert_eq!(transparent.a, 128, "alpha mix: {transparent:?}");
}

#[test]
fn test_color_mix_other_spaces() {
    assert_color(
        Color::parse("color-mix(in srgb-linear, red, blue)"),
        (188, 0, 188),
        3,
        "linear-light mix",
    );
    let oklab = Color::parse("color-mix(in oklab, red, blue)").expect("oklab mix");
    let srgb = Color::parse("color-mix(in srgb, red, blue)").expect("srgb mix");
    assert!(
        oklab.r.abs_diff(srgb.r) > 10 || oklab.b.abs_diff(srgb.b) > 10,
        "oklab mix differs from srgb: {oklab:?} vs {srgb:?}"
    );
    assert!(oklab.g < 130 && oklab.b > 100, "oklab purple: {oklab:?}");
    let oklch = Color::parse("color-mix(in oklch, red, blue)").expect("oklch mix");
    assert!(oklch.b > 100, "oklch purple: {oklch:?}");
}

#[test]
fn test_invalid_modern_colors() {
    for invalid in [
        "color(foo 1 0 0)",
        "color(srgb 1 0)",
        "lab(50 0)",
        "lab(a b c)",
        "oklab(1 2 3 4 5)",
        "hwb(0 0%)",
        "color-mix(in hsl, red, blue)",
        "color-mix(in srgb, red)",
        "color-mix(in srgb, red -10%, blue)",
        "color(from red srgb r g b)",
    ] {
        assert_eq!(Color::parse(invalid), None, "{invalid}");
    }
}

#[test]
fn test_legacy_colors_still_work() {
    assert_eq!(Color::parse("red"), Some(Color::RED));
    assert_eq!(Color::parse("#0f0"), Some(Color::new(0, 255, 0, 255)));
    assert_eq!(
        Color::parse("rgba(0, 0, 255, 0.5)"),
        Some(Color::new(0, 0, 255, 128))
    );
}

#[test]
fn test_modern_colors_in_properties() {
    use dom::{Attribute, Document, Namespace};
    let mut document = Document::new();
    let body = document.create_element(Namespace::Html, "body", vec![]);
    let element = document.create_element(
        Namespace::Html,
        "div",
        vec![Attribute::new(
            "style",
            "background-color: lab(50 0 0); color: color(display-p3 1 0 0); \
             border: 2px solid oklab(0.5 0 0); \
             background-image: linear-gradient(color(srgb 1 0 0), color(srgb 0 0 1)); \
             text-shadow: 1px 1px 0 color(srgb 0 1 0);",
        )],
    );
    document.append_child(document.root(), body);
    document.append_child(body, element);
    let styles = css::resolve_document_styles(&document, None);
    let style = styles.get(&element).cloned().expect("style");
    assert_color(
        Some(style.background_color),
        (119, 119, 119),
        2,
        "background lab",
    );
    assert_color(Some(style.color), (255, 0, 0), 1, "color p3");
    assert_color(
        Some(style.border_color.top),
        (99, 99, 99),
        3,
        "border oklab",
    );
    let gradient = style.background_layers[0]
        .gradient
        .as_ref()
        .expect("gradient");
    assert_color(Some(gradient.stops[0].color), (255, 0, 0), 1, "stop 1");
    assert_color(Some(gradient.stops[1].color), (0, 0, 255), 1, "stop 2");
    assert_color(
        Some(style.text_shadows[0].color),
        (0, 255, 0),
        1,
        "text shadow",
    );
}

#[test]
fn test_color_function_percentage_channels() {
    assert_color(
        Color::parse("color(srgb 100% 50% 0%)"),
        (255, 128, 0),
        1,
        "percentage channels",
    );
    assert_eq!(
        Color::parse("color(srgb 1 0 0 / 150%)")
            .expect("alpha clamp")
            .a,
        255
    );
}

#[test]
fn test_lab_and_oklab_percentage_lightness() {
    let lab_num = Color::parse("lab(50 0 0)").expect("number L");
    let lab_pct = Color::parse("lab(50% 0 0)").expect("percentage L");
    assert_eq!(
        (lab_num.r, lab_num.g, lab_num.b),
        (lab_pct.r, lab_pct.g, lab_pct.b)
    );
    let oklab_num = Color::parse("oklab(0.5 0 0)").expect("number L");
    let oklab_pct = Color::parse("oklab(50% 0 0)").expect("percentage L");
    assert!(
        oklab_num.r.abs_diff(oklab_pct.r) <= 1
            && oklab_num.g.abs_diff(oklab_pct.g) <= 1
            && oklab_num.b.abs_diff(oklab_pct.b) <= 1,
        "{oklab_num:?} vs {oklab_pct:?}"
    );
}

#[test]
fn test_color_mix_identity_and_normalization() {
    assert_color(
        Color::parse("color-mix(in oklch, red, red)"),
        (255, 0, 0),
        2,
        "identical colors stay identical",
    );
    assert_color(
        Color::parse("color-mix(in srgb, red 100%, blue)"),
        (255, 0, 0),
        1,
        "100% weight",
    );
    assert_color(
        Color::parse("color-mix(in srgb, red 200%, blue 200%)"),
        (128, 0, 128),
        1,
        "percentages normalize",
    );
    assert_eq!(
        Color::parse("color-mix(in srgb, red 0%, blue 0%)"),
        None,
        "zero weights are invalid"
    );
}

#[test]
fn test_color_mix_alpha_weights() {
    let mixed = Color::parse("color-mix(in srgb, red 50%, transparent)").expect("mix");
    assert_eq!(mixed.a, 128);
    assert_eq!(mixed.r, 255, "unpremultiplied red kept: {mixed:?}");
    let mostly_red = Color::parse("color-mix(in srgb, red 90%, transparent)").expect("mix");
    assert!(
        mostly_red.a > 200 && mostly_red.a < 245,
        "alpha weighting: {mostly_red:?}"
    );
}

#[test]
fn test_modern_colors_in_filter_and_drop_shadow() {
    use dom::{Attribute, Document, Namespace};
    let mut document = Document::new();
    let body = document.create_element(Namespace::Html, "body", vec![]);
    let element = document.create_element(
        Namespace::Html,
        "div",
        vec![Attribute::new(
            "style",
            "filter: drop-shadow(1px 1px 0 color(srgb 0 1 0));",
        )],
    );
    document.append_child(document.root(), body);
    document.append_child(body, element);
    let styles = css::resolve_document_styles(&document, None);
    let style = styles.get(&element).cloned().expect("style");
    match style.filter.first().expect("filter function") {
        css::FilterFunction::DropShadow { color, .. } => {
            assert_color(Some(*color), (0, 255, 0), 1, "drop shadow color");
        }
        other => panic!("expected drop-shadow, got {other:?}"),
    }
}

#[test]
fn test_modern_colors_in_conic_gradient_stops() {
    use dom::{Attribute, Document, Namespace};
    let mut document = Document::new();
    let body = document.create_element(Namespace::Html, "body", vec![]);
    let element = document.create_element(
        Namespace::Html,
        "div",
        vec![Attribute::new(
            "style",
            "background-image: conic-gradient(color-mix(in srgb, red, blue) 0deg, \
             color(srgb 1 1 1) 180deg);",
        )],
    );
    document.append_child(document.root(), body);
    document.append_child(body, element);
    let styles = css::resolve_document_styles(&document, None);
    let style = styles.get(&element).cloned().expect("style");
    let gradient = style.background_layers[0]
        .gradient
        .as_ref()
        .expect("gradient");
    assert_eq!(gradient.stops.len(), 2);
    assert_color(
        Some(gradient.stops[0].color),
        (128, 0, 128),
        1,
        "conic stop 1",
    );
    assert_color(
        Some(gradient.stops[1].color),
        (255, 255, 255),
        1,
        "conic stop 2",
    );
}

#[test]
fn test_modern_color_invalid_edges() {
    for invalid in [
        "color(srgb)",
        "color(srgb 0.5 0.5 0.5 /)",
        "lab(50 0 0 / 50% extra)",
        "oklch(0.5 0.1 120deg extra)",
        "color-mix(in srgb, red, blue, green)",
        "color-mix(red, blue)",
    ] {
        assert_eq!(Color::parse(invalid), None, "{invalid}");
    }
}

#[test]
fn test_hwb_alpha_and_percentages() {
    let alpha = Color::parse("hwb(120deg 10% 20% / 40%)").expect("hwb alpha");
    assert_eq!(alpha.a, 102);
    assert_color(
        Color::parse("hwb(0 0% 0% / 100%)"),
        (255, 0, 0),
        1,
        "opaque hwb",
    );
}

#[test]
fn test_color_mix_oklch_shortest_hue() {
    // Interpolating red→blue in OKLCH follows the shorter hue arc and stays purple.
    let mixed = Color::parse("color-mix(in oklch, red 50%, blue 50%)").expect("oklch mix");
    assert!(mixed.b > 90 && mixed.r > 90, "purple-ish: {mixed:?}");
    let reversed = Color::parse("color-mix(in oklch, blue 50%, red 50%)").expect("reverse");
    assert!(
        mixed.r.abs_diff(reversed.r) <= 2 && mixed.b.abs_diff(reversed.b) <= 2,
        "order symmetric: {mixed:?} vs {reversed:?}"
    );
}

#[test]
fn rejects_non_ascii_hex_digits_without_panicking() {
    // The CSS tokenizer can produce `Hash { value: "aé" }`; slicing that by byte length
    // used to panic on a non-char-boundary and abort the render.
    assert!(Color::parse("#aé").is_none());
    assert!(Color::parse("#12é4").is_none());
    assert!(Color::parse("#é").is_none());
    // ASCII hex colors keep working.
    assert_eq!(Color::parse("#abc").expect("short hex").r, 0xaa);
    assert_eq!(Color::parse("#a1b2c3").expect("long hex").g, 0xb2);
}

#[test]
fn modern_rgb_and_hsl_space_slash_and_case_insensitive() {
    let c1 = Color::parse("rgb(255 0 0 / 50%)").expect("modern rgb slash pct");
    assert_eq!((c1.r, c1.g, c1.b, c1.a), (255, 0, 0, 128));

    let c2 = Color::parse("RGBA(0 128 0 / 0.5)").expect("uppercase rgba slash num");
    assert_eq!((c2.r, c2.g, c2.b, c2.a), (0, 128, 0, 128));

    let c3 = Color::parse("RGB(100%, 0%, 50%)").expect("uppercase legacy rgb");
    assert_eq!((c3.r, c3.g, c3.b, c3.a), (255, 0, 128, 255));

    // Reject trailing garbage or invalid slash syntax.
    assert!(Color::parse("rgb(1, 2, 3, 0.5, 99)").is_none());
    assert!(Color::parse("rgb(1 2 3 / 0.5 / 0.5)").is_none());
    assert!(Color::parse("rgb(1 2 3 /)").is_none());
    assert!(Color::parse("rgb(1, 2, 3 / 0.5)").is_none());

    // hsl() and hsla() legacy and modern forms.
    let h1 = Color::parse("hsl(120, 100%, 50%)").expect("legacy hsl green");
    assert_eq!((h1.r, h1.g, h1.b, h1.a), (0, 255, 0, 255));

    let h2 = Color::parse("HSL(240deg 100% 50% / 50%)").expect("modern hsl blue");
    assert_eq!((h2.r, h2.g, h2.b, h2.a), (0, 0, 255, 128));

    let h3 = Color::parse("hsla(0, 100%, 50%, 0.25)").expect("legacy hsla red");
    assert_eq!((h3.r, h3.g, h3.b, h3.a), (255, 0, 0, 64));

    assert!(Color::parse("hsl(0, 100%, 50%, 0.5, 1)").is_none());
}

#[test]
fn shadow_list_rejects_entire_declaration_when_any_layer_is_invalid() {
    use dom::{Attribute, Document, Namespace};

    let check = |style_attr: &str| -> css::ComputedStyle {
        let mut document = Document::new();
        let body = document.create_element(Namespace::Html, "body", vec![]);
        let element = document.create_element(
            Namespace::Html,
            "div",
            vec![Attribute::new("style", style_attr)],
        );
        document.append_child(document.root(), body);
        document.append_child(body, element);
        let styles = css::resolve_document_styles(&document, None);
        styles.get(&element).cloned().expect("style")
    };

    let valid = check(
        "box-shadow: 2px 4px 6px red, inset 1px 2px blue; text-shadow: 1px 2px red, 3px 4px 5px blue;",
    );
    assert_eq!(valid.box_shadows.len(), 2);
    assert_eq!(valid.text_shadows.len(), 2);

    for invalid_box in [
        "box-shadow: 2px 4px red, none;",
        "box-shadow: 2px 4px red, 5px;",
        "box-shadow: 2px 4px red, 1px 2px -3px blue;",
        "box-shadow: 2px 4px red, 1px 2px unknowntoken;",
        "box-shadow: 2px 4px red, ;",
    ] {
        let style = check(invalid_box);
        assert!(
            style.box_shadows.is_empty(),
            "expected empty box_shadows for `{invalid_box}`, got {:?}",
            style.box_shadows
        );
    }

    for invalid_text in [
        "text-shadow: 1px 2px red, none;",
        "text-shadow: 1px 2px red, 3px;",
        "text-shadow: 1px 2px red, 1px 2px -1px blue;",
        "text-shadow: 1px 2px red, inset 1px 2px blue;",
    ] {
        let style = check(invalid_text);
        assert!(
            style.text_shadows.is_empty(),
            "expected empty text_shadows for `{invalid_text}`, got {:?}",
            style.text_shadows
        );
    }
}
