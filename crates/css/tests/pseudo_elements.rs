//! CSS parser, selector matching, specificity, and cascade tests for `::before` / `::after` (#1540).

use megi_css::{
    Color, Content, ContentItem, MediaContext, PseudoElement, Specificity, parse_selectors,
    resolve_document_styles_with_pseudos, try_query_selector_all,
};
use megi_core::{Attribute, Document, Namespace};

#[test]
fn pseudo_element_selector_parsing_and_specificity() {
    let before = parse_selectors("div::before");
    assert_eq!(before.selectors.len(), 1);
    assert_eq!(
        before.selectors[0].pseudo_element(),
        Some(PseudoElement::Before)
    );
    assert_eq!(before.max_specificity(), Specificity::new(0, 0, 2));

    let standalone_after = parse_selectors("::after");
    assert_eq!(standalone_after.selectors.len(), 1);
    assert_eq!(
        standalone_after.selectors[0].pseudo_element(),
        Some(PseudoElement::After)
    );
    assert_eq!(
        standalone_after.max_specificity(),
        Specificity::new(0, 0, 1)
    );

    // Legacy single-colon syntax per CSS 2.2 / Selectors 3.
    let legacy_before = parse_selectors("li:before");
    assert_eq!(legacy_before.selectors.len(), 1);
    assert_eq!(
        legacy_before.selectors[0].pseudo_element(),
        Some(PseudoElement::Before)
    );

    let legacy_after = parse_selectors(":after");
    assert_eq!(legacy_after.selectors.len(), 1);
    assert_eq!(
        legacy_after.selectors[0].pseudo_element(),
        Some(PseudoElement::After)
    );

    // Pseudo-element in non-final compound is invalid.
    let invalid = parse_selectors("div::before span");
    assert!(invalid.selectors.is_empty());
}

#[test]
fn pseudo_element_selectors_never_match_dom_query_selector() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let div = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, div);

    let sel = parse_selectors("div::before");
    assert!(!sel.matches(&doc, div));
    assert!(sel.selectors[0].matches_for_pseudo(&doc, div, PseudoElement::Before));
    assert!(!sel.selectors[0].matches_for_pseudo(&doc, div, PseudoElement::After));

    let found = try_query_selector_all(&doc, "div::before").expect("query");
    assert!(found.is_empty());
}

#[test]
fn content_property_parses_strings_escapes_attr_counter_quotes_and_url() {
    assert_eq!(Content::parse("normal"), Some(Content::Normal));
    assert_eq!(Content::parse("none"), Some(Content::None));
    assert!(!Content::Normal.generates_box());
    assert!(!Content::None.generates_box());

    let empty = Content::parse("\"\"").expect("empty string");
    assert!(empty.generates_box());

    let bullet = Content::parse("\"\\2022 \"").expect("hex escape");
    assert_eq!(
        bullet,
        Content::Items(vec![ContentItem::String("\u{2022}".to_string())])
    );

    let combo = Content::parse("open-quote attr(data-tag) \" #\" counter(item) close-quote")
        .expect("mixed items");
    assert_eq!(
        combo,
        Content::Items(vec![
            ContentItem::Quote(true),
            ContentItem::Attr("data-tag".to_string()),
            ContentItem::String(" #".to_string()),
            ContentItem::Counter("item".to_string()),
            ContentItem::Quote(false),
        ])
    );

    let url = Content::parse("url(\"icon.png\") / \"alt\"").expect("url with alt");
    assert_eq!(url.image_url(), Some("icon.png"));
}

#[test]
fn pseudo_element_cascade_inheritance_specificity_and_layers() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let box_el = doc.create_element(
        Namespace::Html,
        "div",
        vec![
            Attribute::new("class", "card"),
            Attribute::new("data-badge", "NEW"),
        ],
    );
    let plain_span = doc.create_element(Namespace::Html, "span", vec![]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, box_el);
    doc.append_child(body, plain_span);

    let css_text = r#"
        @layer base {
            div::before { content: "base"; color: blue; }
        }
        div.card { color: red; }
        div.card::before { content: "[" attr(data-badge) "]"; }
        ::before { color: green; }
        span::after { content: none; }
    "#;

    let (styles, pseudos) =
        resolve_document_styles_with_pseudos(&doc, Some(css_text), MediaContext::default());

    let div_style = styles.get(&box_el).expect("div style");
    assert_eq!(div_style.color, Color::RED);

    // `div.card::before` beats `div::before` (in @layer) and `::before`, and inherits font from div.
    let before = pseudos
        .get(&(box_el, PseudoElement::Before))
        .expect("div::before generated");
    assert_eq!(before.color, Color::GREEN);
    assert_eq!(
        before.content,
        Content::Items(vec![
            ContentItem::String("[".to_string()),
            ContentItem::Attr("data-badge".to_string()),
            ContentItem::String("]".to_string()),
        ])
    );

    // `::before` on body has no `content` declaration -> no pseudo style stored.
    assert!(!pseudos.contains_key(&(body, PseudoElement::Before)));
    // `span::after` with `content: none` -> no pseudo style stored.
    assert!(!pseudos.contains_key(&(plain_span, PseudoElement::After)));
}
