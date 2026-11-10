//! # WHATWG HTML §15 Default User-Agent Stylesheet
//!
//! Provides the normative default UA styles for HTML elements per
//! <https://html.spec.whatwg.org/multipage/rendering.html>, plus an **approximate form
//! controls** section (R102) for the desktop shell. This engine paints no native widgets:
//! inputs/textarea/select/button are ordinary CSS boxes with borders/padding/backgrounds;
//! checkbox/radio are squares/circles whose `checked` state comes from the DOM `checked`
//! attribute (`:checked`/`:disabled`/`:focus` are dynamic pseudo-classes this engine maps to
//! `PseudoClass::Never`, so attribute selectors are used instead). The shell additionally
//! sets `data-megi-focused`/`data-megi-active` attributes while editing so the focus ring and
//! pressed state can be styled here. This is deliberately Chrome-approximate, not pixel-
//! exact.

use crate::cascade::{Origin, StyleCollection};
use crate::rule::Stylesheet;

/// The normative default HTML5 User-Agent stylesheet string.
pub const USER_AGENT_STYLESHEET: &str = r#"
/* WHATWG HTML §15.3.1 Display types */
html, body, div, p, blockquote, figure, figcaption, header, footer,
nav, section, article, aside, main, h1, h2, h3, h4, h5, h6,
ul, ol, dl, dt, dd, pre, address, hr, fieldset, form {
    display: block;
}

li {
    display: list-item;
}

/* Hidden elements (§15.3.2) */
head, link, meta, script, style, template, title {
    display: none;
}

/* Page margin (§15.3.3) */
body {
    margin: 8px;
}

/* Headings margin and font-size (§15.3.6) */
h1 {
    font-size: 2em;
    margin-block: 0.67em;
    font-weight: bold;
}
h2 {
    font-size: 1.5em;
    margin-block: 0.83em;
    font-weight: bold;
}
h3 {
    font-size: 1.17em;
    margin-block: 1em;
    font-weight: bold;
}
h4 {
    font-size: 1em;
    margin-block: 1.33em;
    font-weight: bold;
}
h5 {
    font-size: 0.83em;
    margin-block: 1.67em;
    font-weight: bold;
}
h6 {
    font-size: 0.67em;
    margin-block: 2.33em;
    font-weight: bold;
}

/* Paragraphs, preformatted text & quotes */
p {
    margin-block: 1em;
}
pre {
    white-space: pre;
    font-family: monospace;
    margin-block: 1em;
}
blockquote {
    margin-block: 1em;
    margin-inline: 40px;
}
ul, ol {
    margin-block: 1em;
    padding-inline: 40px;
}
ul {
    list-style-type: disc;
}
ol {
    list-style-type: decimal;
}

/* Text-level semantics (§15.3.7) */
b, strong {
    font-weight: bold;
}
i, em, cite, var, dfn {
    font-style: italic;
}
code, kbd, samp {
    font-family: monospace;
}
small {
    font-size: smaller;
}
hr {
    margin-block: 0.5em;
    border-style: solid;
    border-width: 1px;
}

/* Tables (§15.3.8) */
table {
    display: table;
    border-collapse: separate;
    border-spacing: 2px;
    border-color: gray;
    box-sizing: border-box;
}
caption {
    display: table-caption;
    text-align: center;
}
thead {
    display: table-header-group;
}
tbody {
    display: table-row-group;
}
tfoot {
    display: table-footer-group;
}
tr {
    display: table-row;
}
colgroup {
    display: table-column-group;
}
col {
    display: table-column;
}
td, th {
    display: table-cell;
    padding: 1px;
}
th {
    font-weight: bold;
    text-align: center;
}

/* ---------------------------------------------------------------------------------------
 * Form controls (approximate desktop UA styling; no native widgets).
 * The shell marks the focused control with `data-megi-focused` and a pressed button with
 * `data-megi-active`; `checked`/`disabled`/`readonly` are plain DOM attributes, while
 * `:checked`/`:disabled`/`:focus` are dynamic pseudo-classes this engine does not track.
 * ------------------------------------------------------------------------------------- */
form {
    display: block;
}
fieldset {
    display: block;
    margin-block: 0 0.75em;
    margin-inline: 2px;
    padding-block: 0.35em 0.625em;
    padding-inline: 0.75em;
    border: 1px solid #c0c0c0;
}
legend {
    display: block;
    padding-inline: 2px;
    font-weight: bold;
}
label {
    display: inline;
}

input, textarea, select, button {
    display: inline-block;
    box-sizing: border-box;
    vertical-align: middle;
    padding: 1px 2px;
    border: 1px solid #767676;
    background-color: #ffffff;
    color: #000000;
    max-width: 100%;
}
input[type=text], input[type=search], input[type=email], input[type=password],
input[type=number], input[type=file], input[type=date] {
    width: 10em;
    min-width: 5em;
    height: 1.6em;
}
textarea {
    display: inline-block;
    width: 20em;
    min-width: 8em;
    height: 3.4em;
    padding: 2px;
    vertical-align: top;
    overflow: hidden;
}
select {
    display: inline-block;
    width: 8em;
    height: 1.6em;
    padding: 0 2px;
    overflow: hidden;
}
option {
    display: block;
    padding: 0 2px;
}
input[type=submit], input[type=button], input[type=reset], button {
    display: inline-block;
    width: auto;
    min-width: 5em;
    height: 1.7em;
    padding: 1px 6px;
    background-color: #efefef;
    text-align: center;
}
input[type=checkbox], input[type=radio] {
    display: inline-block;
    width: 13px;
    height: 13px;
    min-width: 0;
    padding: 0;
    margin: 3px 3px 3px 4px;
    background-color: #ffffff;
    vertical-align: middle;
}
input[type=radio] {
    border-radius: 50%;
}
input[type=checkbox][checked] {
    background-color: #4d4d4d;
}
input[type=radio][checked] {
    background-color: #4d4d4d;
}
input[disabled], textarea[disabled], select[disabled], button[disabled],
input[disabled][checked] {
    background-color: #f2f2f2;
    color: #808080;
    border-color: #c0c0c0;
}
input[readonly], textarea[readonly] {
    background-color: #f7f7f7;
}
input[data-megi-focused], textarea[data-megi-focused], select[data-megi-focused],
button[data-megi-focused] {
    border-color: #4d90fe;
}
input[type=submit][data-megi-active], input[type=button][data-megi-active],
input[type=reset][data-megi-active], button[data-megi-active] {
    background-color: #d7d7d7;
    border-color: #4d90fe;
}
"#;

/// Parses the default User-Agent stylesheet into a [`Stylesheet`].
pub fn parse_user_agent_stylesheet() -> Stylesheet {
    crate::parser::parse_stylesheet(USER_AGENT_STYLESHEET)
}

/// Creates a [`StyleCollection`] pre-loaded with the default HTML5 User-Agent stylesheet.
pub fn create_user_agent_style_collection() -> StyleCollection {
    let mut collection = StyleCollection::new();
    let ua_sheet = parse_user_agent_stylesheet();
    collection.add_stylesheet(&ua_sheet, Origin::UserAgent);
    collection
}

#[cfg(test)]
mod tests {
    use crate::properties::{BorderStyle, Color, ComputedStyle, Display, Length, Overflow};
    use dom::{Attribute, Document, Namespace, NodeId};
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
        let styles = crate::cascade::resolve_document_styles(&doc, None);

        for id in [input, textarea, select] {
            let control = style(&styles, id);
            assert_eq!(control.display, Display::InlineBlock, "{id:?}");
            assert!(has_visible_border(control), "{id:?} needs a border");
            assert_eq!(control.background_color, Color::WHITE, "{id:?}");
        }
        let input = style(&styles, input);
        assert!(matches!(input.width, Length::Px(width) if width > 100.0));
        let textarea = style(&styles, textarea);
        assert!(matches!(textarea.width, Length::Px(width) if width > 100.0));
        assert!(matches!(textarea.height, Length::Px(height) if height > 20.0));
        assert_eq!(select_overflow(&styles, select), Overflow::Hidden);
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
}
