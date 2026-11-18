//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

#[cfg(test)]
pub(crate) mod background_layers_tests {
    //! `background-origin`/`clip`/`attachment`, multi-layer lists and layered
    //! `background` shorthand (R94).

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
    fn test_origin_clip_attachment_defaults() {
        let style = ComputedStyle::initial();
        assert_eq!(style.background_layers.len(), 0);
        let layer = style.background_layers.first().cloned().unwrap_or_default();
        assert_eq!(layer.origin, BackgroundOrigin::PaddingBox);
        assert_eq!(layer.clip, BackgroundClip::BorderBox);
        assert_eq!(layer.attachment, BackgroundAttachment::Scroll);
    }

    #[test]
    fn test_origin_clip_attachment_longhands() {
        let style = style_with_inline(
            "background-image: url(a.png); background-origin: content-box; \
             background-clip: content-box; background-attachment: fixed;",
        );
        assert_eq!(style.background_layers.len(), 1);
        assert_eq!(
            style.background_layers[0].origin,
            BackgroundOrigin::ContentBox
        );
        assert_eq!(style.background_layers[0].clip, BackgroundClip::ContentBox);
        assert_eq!(
            style.background_layers[0].attachment,
            BackgroundAttachment::Fixed
        );
        assert!(style.background_geometry_explicit);
    }

    #[test]
    fn test_background_clip_text_is_parsed() {
        let style = style_with_inline("background-clip: text;");
        let layer = style.background_layers.first().cloned().unwrap_or_default();
        assert_eq!(layer.clip, BackgroundClip::Text);
    }

    #[test]
    fn test_multiple_layers_parse_lists_cyclically() {
        let style = style_with_inline(
            "background-image: url(top.png), url(bottom.png); \
             background-repeat: no-repeat; \
             background-size: cover, contain; \
             background-position: center; \
             background-origin: content-box;",
        );
        assert_eq!(style.background_layers.len(), 2);
        assert_eq!(
            style.background_layers[0].image,
            Some("top.png".to_string())
        );
        assert_eq!(
            style.background_layers[1].image,
            Some("bottom.png".to_string())
        );
        // Single-valued lists repeat to every layer.
        assert_eq!(
            style.background_layers[0].repeat,
            BackgroundRepeat::NO_REPEAT
        );
        assert_eq!(
            style.background_layers[1].repeat,
            BackgroundRepeat::NO_REPEAT
        );
        assert_eq!(
            style.background_layers[0].origin,
            BackgroundOrigin::ContentBox
        );
        assert_eq!(
            style.background_layers[1].origin,
            BackgroundOrigin::ContentBox
        );
        // Multi-valued lists are consumed per layer.
        assert_eq!(style.background_layers[0].size, BackgroundSize::Cover);
        assert_eq!(style.background_layers[1].size, BackgroundSize::Contain);
        assert_eq!(style.background_layers[0].position.x, Length::Percent(50.0));
        assert!(style.background_geometry_explicit);
        // The legacy scalar view mirrors the first (top) layer.
        assert_eq!(style.background_image, Some("top.png".to_string()));
        assert_eq!(style.background_size, BackgroundSize::Cover);
    }

    #[test]
    fn test_layered_background_shorthand() {
        let style = style_with_inline(
            "background: url(top.png) no-repeat center / cover content-box, \
             url(bottom.png) repeat-x padding-box;",
        );
        assert_eq!(style.background_layers.len(), 2);
        let top = &style.background_layers[0];
        assert_eq!(top.image, Some("top.png".to_string()));
        assert_eq!(top.repeat, BackgroundRepeat::NO_REPEAT);
        assert_eq!(top.size, BackgroundSize::Cover);
        assert_eq!(top.origin, BackgroundOrigin::ContentBox);
        assert_eq!(top.clip, expected_clip_for_origin(top.origin));
        let bottom = &style.background_layers[1];
        assert_eq!(bottom.image, Some("bottom.png".to_string()));
        assert_eq!(bottom.repeat.x, BackgroundRepeatAxis::Repeat);
        assert_eq!(bottom.repeat.y, BackgroundRepeatAxis::NoRepeat);
        assert_eq!(bottom.origin, BackgroundOrigin::PaddingBox);
        assert!(style.background_geometry_explicit);
    }

    /// The shorthand maps a single box keyword to both origin and clip; both are
    /// represented by the same keyword string, so the clip enum mirrors it.
    fn expected_clip_for_origin(origin: BackgroundOrigin) -> BackgroundClip {
        match origin {
            BackgroundOrigin::ContentBox => BackgroundClip::ContentBox,
            BackgroundOrigin::PaddingBox => BackgroundClip::PaddingBox,
            BackgroundOrigin::BorderBox => BackgroundClip::BorderBox,
        }
    }

    #[test]
    fn test_background_attachment_in_shorthand() {
        let style = style_with_inline("background: url(a.png) fixed;");
        assert_eq!(
            style.background_layers[0].attachment,
            BackgroundAttachment::Fixed
        );
    }

    #[test]
    fn test_single_background_image_keeps_legacy_flag() {
        let style = style_with_inline("background-image: url(only.png);");
        assert_eq!(style.background_layers.len(), 1);
        assert!(!style.background_geometry_explicit);
    }
}

#[cfg(test)]
pub(crate) mod decoration_tests {
    //! `border-radius`, `box-shadow` and `text-shadow` (R99).

    use super::*;
    use crate::properties::MAX_BOX_SHADOWS;
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

    fn style_pair(parent: &str, child: &str) -> (ComputedStyle, ComputedStyle) {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let parent_node = doc.create_element(
            Namespace::Html,
            "div",
            vec![Attribute::new("style", parent)],
        );
        let child_node =
            doc.create_element(Namespace::Html, "p", vec![Attribute::new("style", child)]);
        doc.append_child(doc.root(), body);
        doc.append_child(body, parent_node);
        doc.append_child(parent_node, child_node);
        let styles = resolve_document_styles(&doc, None);
        (
            styles.get(&parent_node).cloned().expect("parent"),
            styles.get(&child_node).cloned().expect("child"),
        )
    }

    #[test]
    fn test_decoration_defaults() {
        let style = ComputedStyle::initial();
        assert!(style.border_radius.is_zero());
        assert!(style.box_shadows.is_empty());
        assert!(style.text_shadows.is_empty());
    }

    #[test]
    fn test_border_radius_shorthand_expansion() {
        let all = style_with_inline("border-radius: 4px;");
        assert_eq!(
            all.border_radius.top_left,
            Radius::circular(Length::Px(4.0))
        );
        assert_eq!(
            all.border_radius.bottom_right,
            Radius::circular(Length::Px(4.0))
        );

        let two = style_with_inline("border-radius: 4px 8px;");
        assert_eq!(two.border_radius.top_left.x, Length::Px(4.0));
        assert_eq!(two.border_radius.top_right.x, Length::Px(8.0));
        assert_eq!(two.border_radius.bottom_left.x, Length::Px(8.0));

        let three = style_with_inline("border-radius: 1px 2px 3px;");
        assert_eq!(three.border_radius.top_left.x, Length::Px(1.0));
        assert_eq!(three.border_radius.top_right.x, Length::Px(2.0));
        assert_eq!(three.border_radius.bottom_right.x, Length::Px(3.0));
        assert_eq!(three.border_radius.bottom_left.x, Length::Px(2.0));

        let four = style_with_inline("border-radius: 1px 2px 3px 4px;");
        assert_eq!(four.border_radius.bottom_left.x, Length::Px(4.0));
    }

    #[test]
    fn test_border_radius_elliptical_slash() {
        let style = style_with_inline("border-radius: 10px / 20px;");
        assert_eq!(style.border_radius.top_left.x, Length::Px(10.0));
        assert_eq!(style.border_radius.top_left.y, Length::Px(20.0));
        let mixed = style_with_inline("border-radius: 10px 5px / 20% 30%;");
        assert_eq!(mixed.border_radius.top_left.x, Length::Px(10.0));
        assert_eq!(mixed.border_radius.top_left.y, Length::Percent(20.0));
        assert_eq!(mixed.border_radius.top_right.y, Length::Percent(30.0));
    }

    #[test]
    fn test_border_radius_longhands_override_shorthand() {
        let style = style_with_inline("border-radius: 4px; border-top-left-radius: 12px 6px;");
        assert_eq!(style.border_radius.top_left.x, Length::Px(12.0));
        assert_eq!(style.border_radius.top_left.y, Length::Px(6.0));
        assert_eq!(style.border_radius.top_right.x, Length::Px(4.0));
    }

    #[test]
    fn test_invalid_border_radius_is_tolerated() {
        let style = style_with_inline("border-radius: -4px auto;");
        assert!(style.border_radius.is_zero());
    }

    #[test]
    fn test_box_shadow_parsing() {
        let none = style_with_inline("box-shadow: none;");
        assert!(none.box_shadows.is_empty());

        let style = style_with_inline("box-shadow: 2px 3px 4px 5px rgba(0, 0, 0, 0.5) inset;");
        assert_eq!(style.box_shadows.len(), 1);
        let shadow = style.box_shadows[0];
        assert_eq!(shadow.offset_x, 2.0);
        assert_eq!(shadow.offset_y, 3.0);
        assert_eq!(shadow.blur, 4.0);
        assert_eq!(shadow.spread, 5.0);
        assert_eq!(shadow.color.r, 0);
        assert_eq!(shadow.color.a, 128);
        assert!(shadow.inset);

        let defaults = style_with_inline("box-shadow: 1px 2px red;");
        assert_eq!(defaults.box_shadows[0].blur, 0.0);
        assert_eq!(defaults.box_shadows[0].spread, 0.0);
        assert_eq!(defaults.box_shadows[0].color, Color::RED);

        let current = style_with_inline("color: #336699; box-shadow: 1px 1px;");
        assert_eq!(
            current.box_shadows[0].color,
            Color::new(0x33, 0x66, 0x99, 255)
        );
    }

    #[test]
    fn test_multiple_box_shadows_are_capped() {
        let mut values = Vec::new();
        for index in 0..40 {
            values.push(format!("{index}px {}px red", index + 1));
        }
        let style = style_with_inline(&format!("box-shadow: {};", values.join(", ")));
        assert_eq!(style.box_shadows.len(), MAX_BOX_SHADOWS);
        assert!(style.box_shadows.len() <= 32);
    }

    #[test]
    fn test_text_shadow_and_inheritance() {
        let style = style_with_inline("text-shadow: 1px 2px 3px blue, -1px 0 red;");
        assert_eq!(style.text_shadows.len(), 2);
        assert_eq!(style.text_shadows[0].blur, 3.0);
        assert_eq!(style.text_shadows[0].color, Color::new(0, 0, 255, 255));
        assert_eq!(style.text_shadows[1].color, Color::RED);

        let (parent, child) = style_pair("text-shadow: 1px 1px 2px red;", "color: blue;");
        assert_eq!(
            child.text_shadows, parent.text_shadows,
            "text-shadow inherits"
        );
        let (shadow_parent, shadow_child) = style_pair("box-shadow: 1px 1px red;", "");
        assert!(
            shadow_child.box_shadows.is_empty(),
            "box-shadow does not inherit"
        );
        assert_eq!(shadow_parent.box_shadows.len(), 1);
    }
}
