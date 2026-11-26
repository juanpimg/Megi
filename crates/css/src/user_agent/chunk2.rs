//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

#[cfg(test)]
pub(crate) mod tests {
    use crate::properties::{BorderStyle, Color, ComputedStyle, Display, Length, Overflow};
    use megi_core::{Attribute, Document, Namespace, NodeId};
    use std::collections::HashMap;

    /// Builds an element under `parent` in the HTML namespace.
    fn append(
        doc: &mut Document,
        parent: NodeId,
        name: &str,
        attributes: &[(&str, &str)],
    ) -> NodeId {
        let attrs = attributes
            .iter()
            .map(|(name, value)| Attribute::new(*name, *value))
            .collect::<Vec<_>>();
        let id = doc.create_element(Namespace::Html, name, attrs);
        assert!(doc.append_child(parent, id));
        id
    }

    /// Builds `html > body > form` and returns `(doc, form, styles)`.
    fn form_document() -> (Document, NodeId, HashMap<NodeId, ComputedStyle>) {
        let mut doc = Document::new();
        let root = doc.root();
        let html = append(&mut doc, root, "html", &[]);
        let body = append(&mut doc, html, "body", &[]);
        let form = append(&mut doc, body, "form", &[]);
        let styles = crate::cascade::resolve_document_styles(&doc, None);
        (doc, form, styles)
    }

    fn style(styles: &HashMap<NodeId, ComputedStyle>, id: NodeId) -> &ComputedStyle {
        styles.get(&id).expect("element has a computed style")
    }

    fn has_visible_border(style: &ComputedStyle) -> bool {
        style.border_style.top != BorderStyle::None && style.border_width.top >= 1.0
    }

    #[test]
    fn text_controls_are_visible_boxes_with_inherited_font() {
        let (mut doc, form, _unused) = form_document();
        let input = append(&mut doc, form, "input", &[("type", "text")]);
        let textarea = append(&mut doc, form, "textarea", &[]);
        let select = append(&mut doc, form, "select", &[]);
        // CSS Fonts 4 §3.1 / CSS Cascading 4 §6.2: `font-size` and
        // `font-family` are inherited, and the UA sheet must not force a
        // widget font on text controls. An authored body font makes the
        // inheritance observable instead of merely checking every size > 0.
        let author = "body { font-size: 20px; font-family: serif; }";
        let styles = crate::cascade::resolve_document_styles(&doc, Some(author));

        for id in [input, textarea, select] {
            let control = style(&styles, id);
            assert_eq!(control.display, Display::InlineBlock, "{id:?}");
            assert!(has_visible_border(control), "{id:?} needs a border");
            assert_eq!(control.background_color, Color::WHITE, "{id:?}");
            assert_eq!(
                control.font_size, 20.0,
                "{id:?} must inherit the authored font-size"
            );
            assert_eq!(
                control.font_family, "serif",
                "{id:?} must inherit the authored font-family"
            );
        }
        let input = style(&styles, input);
        // WHATWG HTML §4.10.5.1.2: the UA sheet must not force `width:10em` on
        // text controls, so the `size` attribute can drive intrinsic width
        // (issue #2247). It keeps a `min-width: 5em` floor so the box stays
        // visible; with the authored 20px font that resolves to 100px.
        assert_eq!(input.width, Length::Auto, "no UA-forced text input width");
        assert!(
            matches!(input.min_width, Length::Px(width) if width >= 100.0),
            "input min-width: {:?}",
            input.min_width
        );
        let textarea = style(&styles, textarea);
        assert!(
            matches!(textarea.width, Length::Px(width) if width > 100.0),
            "textarea width: {:?}",
            textarea.width
        );
        assert!(matches!(textarea.height, Length::Px(height) if height > 20.0));
        // A native <select> must not clip its selected label or disclosure
        // arrow: the UA sheet intentionally keeps `overflow: visible`.
        assert_eq!(select_overflow(&styles, select), Overflow::Visible);
        // Fonts are inherited (the UA sheet does not force a widget font).
        assert!(styles.values().all(|style| style.font_size > 0.0));
    }

    fn select_overflow(styles: &HashMap<NodeId, ComputedStyle>, id: NodeId) -> Overflow {
        style(styles, id).overflow
    }

    #[test]
    fn buttons_and_submit_inputs_use_button_styling() {
        let (mut doc, form, _styles) = form_document();
        let submit = append(&mut doc, form, "input", &[("type", "submit")]);
        let button = append(&mut doc, form, "button", &[]);
        let styles = crate::cascade::resolve_document_styles(&doc, None);
        for id in [submit, button] {
            let style = style(&styles, id);
            assert_eq!(style.display, Display::InlineBlock);
            assert_eq!(style.background_color, Color::new(0xEF, 0xEF, 0xEF, 0xFF));
            assert!(
                matches!(style.padding.top, Length::Px(padding) if padding > 0.0),
                "button needs padding"
            );
            assert_eq!(style.text_align, crate::properties::TextAlign::Center);
        }
    }

    #[test]
    fn checkbox_and_radio_are_boxes_with_checked_state() {
        let (mut doc, form, _styles) = form_document();
        let unchecked = append(&mut doc, form, "input", &[("type", "checkbox")]);
        let checked = append(
            &mut doc,
            form,
            "input",
            &[("type", "checkbox"), ("checked", "")],
        );
        let radio = append(
            &mut doc,
            form,
            "input",
            &[("type", "radio"), ("checked", "")],
        );
        let styles = crate::cascade::resolve_document_styles(&doc, None);

        for id in [unchecked, checked, radio] {
            let style = style(&styles, id);
            assert!(matches!(style.width, Length::Px(width) if (width - 13.0).abs() < 0.1));
            assert!(matches!(style.height, Length::Px(height) if (height - 13.0).abs() < 0.1));
            assert!(has_visible_border(style), "checkbox/radio need a border");
        }
        assert_eq!(style(&styles, unchecked).background_color, Color::WHITE);
        assert_eq!(
            style(&styles, checked).background_color,
            Color::new(0x4D, 0x4D, 0x4D, 0xFF),
            "checked box gets the filled background"
        );
        assert!(
            matches!(
                style(&styles, radio).border_radius.top_left.x,
                Length::Percent(pct) if pct > 0.0
            ),
            "radio is circular (border-radius: 50%)"
        );
    }

    #[test]
    fn disabled_and_readonly_controls_are_greyed_out() {
        let (mut doc, form, _styles) = form_document();
        let disabled = append(
            &mut doc,
            form,
            "input",
            &[("type", "text"), ("disabled", "")],
        );
        let readonly = append(&mut doc, form, "textarea", &[("readonly", "")]);
        let styles = crate::cascade::resolve_document_styles(&doc, None);
        let disabled = style(&styles, disabled);
        assert_eq!(
            disabled.background_color,
            Color::new(0xF2, 0xF2, 0xF2, 0xFF)
        );
        assert_eq!(disabled.color, Color::new(0x80, 0x80, 0x80, 0xFF));
        assert_eq!(
            style(&styles, readonly).background_color,
            Color::new(0xF7, 0xF7, 0xF7, 0xFF)
        );
    }

    #[test]
    fn focused_attribute_draws_the_focus_border() {
        let (mut doc, form, _styles) = form_document();
        let focused = append(
            &mut doc,
            form,
            "input",
            &[("type", "text"), ("data-megi-focused", "")],
        );
        let styles = crate::cascade::resolve_document_styles(&doc, None);
        assert_eq!(
            style(&styles, focused).border_color.top,
            Color::new(0x4D, 0x90, 0xFE, 0xFF)
        );
    }

    #[test]
    fn pressed_button_gets_the_active_background() {
        let (mut doc, form, _styles) = form_document();
        let button = append(&mut doc, form, "button", &[("data-megi-active", "")]);
        let styles = crate::cascade::resolve_document_styles(&doc, None);
        assert_eq!(
            style(&styles, button).background_color,
            Color::new(0xD7, 0xD7, 0xD7, 0xFF)
        );
    }

    #[test]
    fn fieldset_legend_and_label_have_ua_styles() {
        let (mut doc, form, _styles) = form_document();
        let fieldset = append(&mut doc, form, "fieldset", &[]);
        let legend = append(&mut doc, fieldset, "legend", &[]);
        let label = append(&mut doc, fieldset, "label", &[]);
        let styles = crate::cascade::resolve_document_styles(&doc, None);
        let fieldset_style = style(&styles, fieldset);
        assert_eq!(fieldset_style.display, Display::Block);
        assert!(has_visible_border(fieldset_style));
        assert!(matches!(fieldset_style.padding.left, Length::Px(padding) if padding > 0.0));
        let legend_style = style(&styles, legend);
        assert_eq!(legend_style.display, Display::Block);
        assert_eq!(legend_style.font_weight, 700);
        assert_eq!(style(&styles, label).display, Display::Inline);
    }

    #[test]
    fn dialog_ua_styles_modal_and_backdrop() {
        let mut doc = Document::new();
        let root = doc.root();
        let html = append(&mut doc, root, "html", &[]);
        let body = append(&mut doc, html, "body", &[]);
        let closed = append(&mut doc, body, "dialog", &[]);
        let non_modal = append(&mut doc, body, "dialog", &[("open", "")]);
        let modal = append(
            &mut doc,
            body,
            "dialog",
            &[("open", ""), ("data-megi-modal", "1")],
        );

        let styles = crate::cascade::resolve_document_styles(&doc, None);
        assert_eq!(style(&styles, closed).display, Display::None);

        let non_modal_style = style(&styles, non_modal);
        assert_eq!(non_modal_style.display, Display::Block);
        assert_eq!(
            non_modal_style.position,
            crate::properties::Position::Absolute
        );
        assert!(non_modal_style.width_fit_content);
        assert!(non_modal_style.height_fit_content);
        assert_eq!(non_modal_style.background_color, Color::WHITE);
        assert_eq!(non_modal_style.backdrop_color, None);

        let modal_style = style(&styles, modal);
        assert_eq!(modal_style.display, Display::Block);
        assert_eq!(modal_style.position, crate::properties::Position::Fixed);
        assert_eq!(modal_style.overflow, Overflow::Auto);
        assert_eq!(modal_style.backdrop_color, Some(Color::new(0, 0, 0, 26)));

        let author = "dialog::backdrop { background-color: rgba(255, 0, 0, 0.5); }";
        let custom_styles = crate::cascade::resolve_document_styles(&doc, Some(author));
        assert_eq!(
            style(&custom_styles, modal).backdrop_color,
            Some(Color::new(255, 0, 0, 128))
        );
    }
}
