//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;


#[cfg(test)]
pub(crate) mod tests_part5 {
    #[allow(unused_imports)]
    use super::*;
    #[allow(unused_imports)]
    use megi_core::{Attribute, Document, Namespace};
    #[allow(unused_imports)]
    use crate::lib_tests_helpers::*;
    #[test]
    fn test_cascade_media_queries_full() {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let p = doc.create_element(Namespace::Html, "p", vec![]);
        doc.append_child(doc.root(), body);
        doc.append_child(body, p);

        // `and` + orientation with a known viewport.
        let css = "@media (min-width: 600px) and (orientation: landscape) { p { color: red; } }";
        let wide = resolve_document_styles_with_viewport(&doc, Some(css), 800.0, 600.0);
        assert_eq!(wide.get(&p).expect("style").color, Color::RED);
        let portrait = resolve_document_styles_with_viewport(&doc, Some(css), 600.0, 800.0);
        assert_eq!(portrait.get(&p).expect("style").color, Color::BLACK);

        // `not` condition.
        let css = "@media not (min-width: 900px) { p { color: red; } }";
        let styles = resolve_document_styles_with_viewport(&doc, Some(css), 800.0, 600.0);
        assert_eq!(styles.get(&p).expect("style").color, Color::RED);

        // Query list with `em` and `print`: either query may apply.
        let css = "@media (max-width: 40em), print { p { color: red; } }";
        let narrow = resolve_document_styles_with_viewport(&doc, Some(css), 400.0, 800.0);
        assert_eq!(narrow.get(&p).expect("style").color, Color::RED);
        let print_context = MediaContext {
            viewport: None,
            screen: false,
            color_scheme: ColorScheme::Light,
            reduced_motion: false,
        };
        let print = resolve_document_styles_with_media(&doc, Some(css), print_context);
        assert_eq!(print.get(&p).expect("style").color, Color::RED);
        let screen_wide = resolve_document_styles_with_viewport(&doc, Some(css), 1000.0, 600.0);
        assert_eq!(screen_wide.get(&p).expect("style").color, Color::BLACK);
    }

    // ---------------------------------------------------------------------------------------

    // Round 7: query API is covered in `query::tests`; image parsing below.

    // ---------------------------------------------------------------------------------------

    #[test]
    fn test_background_image_and_list_style_image_parsing() {
        // Plain URLs: quoted, single-quoted and unquoted; outer quotes are removed.
        for value in [
            "background-image: url(bg.png);",
            "background-image: url(\"bg.png\");",
            "background-image: url('bg.png');",
        ] {
            let style = style_with_inline("div", value);
            assert_eq!(
                style.background_image,
                Some("bg.png".to_string()),
                "{value}"
            );
        }
        // URLs may contain spaces/slashes when quoted.
        assert_eq!(
            style_with_inline("div", "background-image: url(\"img/hero 1.png\");").background_image,
            Some("img/hero 1.png".to_string())
        );

        // `none` and unsupported image types (gradients) compute to None.
        assert_eq!(
            style_with_inline("div", "background-image: none;").background_image,
            None
        );
        for value in [
            "background-image: linear-gradient(red, blue);",
            "background-image: radial-gradient(circle, #fff, #000);",
            "background-image: repeating-linear-gradient(red 0, blue 10px);",
        ] {
            assert_eq!(
                style_with_inline("div", value).background_image,
                None,
                "{value}"
            );
        }
        // `image-set()` selects its 1x URL (DPR 1 is the only supported ratio).
        assert_eq!(
            style_with_inline(
                "div",
                "background-image: image-set(url(a.png) 1x, url(b.png) 2x);"
            )
            .background_image,
            Some("a.png".to_string())
        );
        // Invalid values fall back to the initial None.
        assert_eq!(
            style_with_inline("div", "background-image: 42px;").background_image,
            None
        );

        // Multi-layer values use the first plain URL layer.
        assert_eq!(
            style_with_inline(
                "div",
                "background-image: url(first.png), linear-gradient(red, blue);"
            )
            .background_image,
            Some("first.png".to_string())
        );

        // The `background` shorthand populates color and image at the same time.
        let shorthand = style_with_inline("div", "background: #eee url(bg.png) no-repeat center;");
        assert_eq!(shorthand.background_color, Color::new(238, 238, 238, 255));
        assert_eq!(shorthand.background_image, Some("bg.png".to_string()));

        let color_only = style_with_inline("div", "background: red;");
        assert_eq!(color_only.background_color, Color::RED);
        assert_eq!(color_only.background_image, None);

        let image_only = style_with_inline("div", "background: url(bg.png);");
        assert_eq!(image_only.background_color, Color::TRANSPARENT);
        assert_eq!(image_only.background_image, Some("bg.png".to_string()));

        // Gradients in the shorthand are preserved as images but compute to None.
        assert_eq!(
            style_with_inline("div", "background: url(a.png) linear-gradient(red, blue);")
                .background_image,
            Some("a.png".to_string())
        );
        assert_eq!(
            style_with_inline("div", "background: linear-gradient(red, blue);").background_image,
            None
        );

        // `list-style-image` longhand and `list-style` shorthand.
        assert_eq!(
            style_with_inline("ul", "list-style-image: url(bullet.png);").list_style_image,
            Some("bullet.png".to_string())
        );
        let list_shorthand = style_with_inline("ul", "list-style: url(bullet.png) square;");
        assert_eq!(list_shorthand.list_style_type, ListStyleType::Square);
        assert_eq!(
            list_shorthand.list_style_image,
            Some("bullet.png".to_string())
        );
        let list_none = style_with_inline("ul", "list-style: none;");
        assert_eq!(list_none.list_style_type, ListStyleType::None);
        assert_eq!(list_none.list_style_image, None);

        // Images are not inherited.
        let (parent, child) = style_pair(
            "background-image: url(parent.png); list-style-image: url(parent-bullet.png);",
            "",
        );
        assert_eq!(parent.background_image, Some("parent.png".to_string()));
        assert_eq!(
            parent.list_style_image,
            Some("parent-bullet.png".to_string())
        );
        assert_eq!(child.background_image, None);
        assert_eq!(child.list_style_image, None);

        // Initial values.
        let initial = ComputedStyle::initial();
        assert_eq!(initial.background_image, None);
        assert_eq!(initial.list_style_image, None);
    }
}



#[cfg(test)]
pub(crate) mod background_geometry_tests {
    //! `background-repeat`/`-size`/`-position` + `background` shorthand (R89).

    use super::*;
    use megi_core::{Attribute, Document, Namespace};

    fn style_with_inline(inline: &str) -> ComputedStyle {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let element = doc.create_element(
            Namespace::Html,
            "div",
            vec![Attribute::new("style", inline)],
        );
        doc.append_child(doc.root(), body);
        doc.append_child(body, element);
        let styles = resolve_document_styles(&doc, None);
        styles.get(&element).cloned().expect("computed style")
    }

    #[test]
    fn test_background_geometry_initial_values() {
        let style = ComputedStyle::initial();
        assert_eq!(style.background_repeat, BackgroundRepeat::REPEAT);
        assert_eq!(style.background_size, BackgroundSize::Auto);
        assert_eq!(style.background_position, BackgroundPosition::ZERO);
        assert!(!style.background_geometry_explicit);
    }

    #[test]
    fn test_background_image_only_keeps_legacy_compatibility_flag() {
        let style = style_with_inline("background-image: url(bg.png);");
        assert_eq!(style.background_image, Some("bg.png".to_string()));
        assert!(
            !style.background_geometry_explicit,
            "image-only declarations keep the legacy paint behaviour"
        );
    }

    #[test]
    fn test_background_repeat_parsing() {
        let repeat_x = style_with_inline("background-repeat: repeat-x;");
        assert_eq!(repeat_x.background_repeat.x, BackgroundRepeatAxis::Repeat);
        assert_eq!(repeat_x.background_repeat.y, BackgroundRepeatAxis::NoRepeat);
        let repeat_y = style_with_inline("background-repeat: repeat-y;");
        assert_eq!(repeat_y.background_repeat.x, BackgroundRepeatAxis::NoRepeat);
        assert_eq!(repeat_y.background_repeat.y, BackgroundRepeatAxis::Repeat);
        let no_repeat = style_with_inline("background-repeat: no-repeat;");
        assert_eq!(no_repeat.background_repeat, BackgroundRepeat::NO_REPEAT);
        let pair = style_with_inline("background-repeat: space round;");
        assert_eq!(pair.background_repeat.x, BackgroundRepeatAxis::Space);
        assert_eq!(pair.background_repeat.y, BackgroundRepeatAxis::Round);
        assert!(pair.background_geometry_explicit);
        let invalid = style_with_inline("background-repeat: banana;");
        assert_eq!(invalid.background_repeat, BackgroundRepeat::REPEAT);
    }

    #[test]
    fn test_background_size_parsing() {
        assert_eq!(
            style_with_inline("background-size: cover;").background_size,
            BackgroundSize::Cover
        );
        assert_eq!(
            style_with_inline("background-size: contain;").background_size,
            BackgroundSize::Contain
        );
        assert_eq!(
            style_with_inline("background-size: 100px 50%;").background_size,
            BackgroundSize::Explicit(Length::Px(100.0), Length::Percent(50.0))
        );
        assert_eq!(
            style_with_inline("background-size: 50% auto;").background_size,
            BackgroundSize::Explicit(Length::Percent(50.0), Length::Auto)
        );
        let shorthand_width = style_with_inline("background-size: 40px;");
        assert_eq!(
            shorthand_width.background_size,
            BackgroundSize::Explicit(Length::Px(40.0), Length::Px(40.0))
        );
    }

    #[test]
    fn test_background_position_parsing() {
        let center = style_with_inline("background-position: center;");
        assert_eq!(center.background_position.x, Length::Percent(50.0));
        assert_eq!(center.background_position.y, Length::Percent(50.0));
        let right_bottom = style_with_inline("background-position: right bottom;");
        assert_eq!(right_bottom.background_position.x, Length::Percent(100.0));
        assert_eq!(right_bottom.background_position.y, Length::Percent(100.0));
        let top_left = style_with_inline("background-position: top left;");
        assert_eq!(top_left.background_position.x, Length::Percent(0.0));
        assert_eq!(top_left.background_position.y, Length::Percent(0.0));
        let mixed = style_with_inline("background-position: 10px 75%;");
        assert_eq!(mixed.background_position.x, Length::Px(10.0));
        assert_eq!(mixed.background_position.y, Length::Percent(75.0));
        let one_value = style_with_inline("background-position: 25px;");
        assert_eq!(one_value.background_position.x, Length::Px(25.0));
        assert_eq!(one_value.background_position.y, Length::Percent(50.0));
    }

    #[test]
    fn test_background_shorthand_with_geometry() {
        let style = style_with_inline("background: red url(bg.png) no-repeat center / cover;");
        assert_eq!(style.background_color, Color::RED);
        assert_eq!(style.background_image, Some("bg.png".to_string()));
        assert_eq!(style.background_repeat, BackgroundRepeat::NO_REPEAT);
        assert_eq!(style.background_size, BackgroundSize::Cover);
        assert_eq!(style.background_position.x, Length::Percent(50.0));
        assert_eq!(style.background_position.y, Length::Percent(50.0));
        assert!(style.background_geometry_explicit);
    }

    #[test]
    fn test_background_shorthand_resets_omitted_components() {
        let style = style_with_inline("background: url(bg.png);");
        assert_eq!(style.background_image, Some("bg.png".to_string()));
        assert_eq!(style.background_repeat, BackgroundRepeat::REPEAT);
        assert_eq!(style.background_size, BackgroundSize::Auto);
        assert_eq!(style.background_position, BackgroundPosition::ZERO);
        assert!(style.background_geometry_explicit);
    }

    #[test]
    fn test_background_longhand_overrides_shorthand() {
        let style = style_with_inline(
            "background: url(bg.png) no-repeat center / cover; \
             background-repeat: repeat-y; background-position: 5px 6px;",
        );
        assert_eq!(style.background_repeat.x, BackgroundRepeatAxis::NoRepeat);
        assert_eq!(style.background_repeat.y, BackgroundRepeatAxis::Repeat);
        assert_eq!(style.background_position.x, Length::Px(5.0));
        assert_eq!(style.background_position.y, Length::Px(6.0));
        assert_eq!(style.background_size, BackgroundSize::Cover);
    }
}
