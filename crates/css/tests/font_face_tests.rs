//! `@font-face` parsing tests (CSS Fonts 4 §4.1): descriptors, src lists, malformed rules.

use css::parse_stylesheet;

#[test]
fn parses_woff2_and_ttf_sources_with_formats() {
    let sheet = parse_stylesheet(
        r#"
        @font-face {
            font-family: "Open Sans";
            src: url("fonts/OpenSans.woff2") format("woff2"),
                 url(https://example.com/OpenSans.ttf) format("truetype");
            font-weight: 400;
            font-style: normal;
            font-display: swap;
            unicode-range: U+0000-00FF;
        }
        body { color: red; }
        "#,
    );
    assert_eq!(sheet.font_faces.len(), 1);
    let face = &sheet.font_faces[0];
    assert_eq!(face.family, "Open Sans");
    assert_eq!(face.weight, "400");
    assert_eq!(face.style, "normal");
    assert_eq!(face.sources.len(), 2);
    assert_eq!(face.sources[0].0, "fonts/OpenSans.woff2");
    assert_eq!(face.sources[0].1, "woff2");
    assert_eq!(face.sources[1].0, "https://example.com/OpenSans.ttf");
    assert_eq!(face.sources[1].1, "truetype");
    assert_eq!(face.woff2_source(), Some("fonts/OpenSans.woff2"));
    // The font-face does not leak into the cascade as a style rule.
    assert_eq!(sheet.rules.len(), 1);
}

#[test]
fn format_is_optional_and_unquoted_url_is_accepted() {
    let sheet =
        parse_stylesheet("@font-face { font-family: Unquoted Family; src: url(font.woff2); }");
    assert_eq!(sheet.font_faces.len(), 1);
    assert_eq!(sheet.font_faces[0].family, "Unquoted Family");
    assert_eq!(
        sheet.font_faces[0].sources,
        vec![("font.woff2".to_string(), String::new())]
    );
    assert_eq!(sheet.font_faces[0].weight, "400");
    assert_eq!(sheet.font_faces[0].style, "normal");
}

#[test]
fn normalizes_bold_and_italic_descriptors() {
    let sheet = parse_stylesheet(
        "@font-face { font-family: F; font-weight: bold; font-style: italic; src: url(f.woff2); }",
    );
    assert_eq!(sheet.font_faces[0].weight, "700");
    assert_eq!(sheet.font_faces[0].style, "italic");
}

#[test]
fn malformed_font_face_rules_are_ignored() {
    // Missing family.
    assert!(
        parse_stylesheet("@font-face { src: url(f.woff2); }")
            .font_faces
            .is_empty()
    );
    // Missing src.
    assert!(
        parse_stylesheet("@font-face { font-family: F; }")
            .font_faces
            .is_empty()
    );
    // Unknown descriptors only.
    assert!(
        parse_stylesheet("@font-face { font-display: swap; unicode-range: U+0000; }")
            .font_faces
            .is_empty()
    );
    // A bad rule does not discard the rest of the stylesheet.
    let sheet = parse_stylesheet(
        "@font-face { } @font-face { font-family: Good; src: url(g.woff2) } body { margin: 0 }",
    );
    assert_eq!(sheet.font_faces.len(), 1);
    assert_eq!(sheet.font_faces[0].family, "Good");
    assert_eq!(sheet.rules.len(), 1);
}
