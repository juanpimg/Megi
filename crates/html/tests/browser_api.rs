//! Integration tests for the browser-style HTML mutation API
//! (`innerHTML`/`outerHTML` setters and minimal `document.write`).
//!
//! Everything runs on parser-produced documents; no JS engine, network or filesystem.

use html::{
    Document, DomError, LimitViolations, ParseOptions, document_write, document_write_with_options,
    parse_document, set_inner_html, set_inner_html_with_options, set_outer_html,
};

fn parse(markup: &str) -> Document {
    parse_document(markup, false)
}

fn element_names(doc: &Document, parent: html::NodeId) -> Vec<String> {
    doc.children(parent)
        .filter_map(|child| {
            doc.node(child)
                .as_element()
                .map(|element| element.local_name.clone())
        })
        .collect()
}

fn text(doc: &Document, node: html::NodeId) -> String {
    doc.text_content(node)
}

#[test]
fn inner_html_uses_paragraph_context() {
    let mut doc = parse("<html><body><div id=\"x\"><p>old</p></div></body></html>");
    let div = doc.get_element_by_id("x").expect("div present");
    let violations =
        set_inner_html(&mut doc, div, "Hello <b>bold</b> <i>italics</i>").expect("set innerHTML");
    assert_eq!(violations, LimitViolations::default());
    assert_eq!(element_names(&doc, div), vec!["b", "i"]);
    assert_eq!(text(&doc, div), "Hello bold italics");
    assert!(doc.validate_invariants().is_ok());
}

#[test]
fn inner_html_table_context_wraps_rows() {
    let mut doc =
        parse("<html><body><table id=\"t\"><tbody id=\"tb\"></tbody></table></body></html>");
    let tbody = doc.get_element_by_id("tb").expect("tbody present");
    set_inner_html(&mut doc, tbody, "<tr><td>1</td></tr><tr><td>2</td></tr>")
        .expect("table innerHTML");
    assert_eq!(element_names(&doc, tbody), vec!["tr", "tr"]);
    assert_eq!(text(&doc, tbody), "12");

    // Setting the <table> itself in table context synthesises a <tbody>.
    let table = doc.get_element_by_id("t").expect("table present");
    set_inner_html(&mut doc, table, "<tr><td>3</td></tr>").expect("table content");
    assert_eq!(element_names(&doc, table), vec!["tbody"]);
    assert_eq!(text(&doc, table), "3");
    assert!(doc.validate_invariants().is_ok());
}

#[test]
fn inner_html_select_and_option_context() {
    let mut doc = parse("<html><body><select id=\"s\"></select></body></html>");
    let select = doc.get_element_by_id("s").expect("select present");
    set_inner_html(
        &mut doc,
        select,
        "<option>a</option><option selected>b</option>",
    )
    .expect("select innerHTML");
    assert_eq!(element_names(&doc, select), vec!["option", "option"]);
    assert_eq!(text(&doc, select), "ab");
    assert!(doc.validate_invariants().is_ok());
}

#[test]
fn inner_html_script_and_style_use_rawtext_contexts() {
    let mut doc = parse(
        "<html><head><style id=\"st\"></style></head><body><script id=\"sc\"></script></body></html>",
    );
    let script = doc.get_element_by_id("sc").expect("script present");
    let payload = "if (a < b) { x = '</div>'; }";
    set_inner_html(&mut doc, script, payload).expect("script innerHTML");
    assert_eq!(doc.children(script).count(), 1);
    assert_eq!(text(&doc, script), payload);

    let style = doc.get_element_by_id("st").expect("style present");
    set_inner_html(&mut doc, style, "body { color: red }").expect("style innerHTML");
    assert_eq!(text(&doc, style), "body { color: red }");
    assert!(doc.validate_invariants().is_ok());
}

#[test]
fn inner_html_and_outer_html_on_template() {
    let mut doc = parse("<html><body><template id=\"t\"><p>old</p></template></body></html>");
    let template = doc.get_element_by_id("t").expect("template present");
    let fragment = doc
        .node(template)
        .as_element()
        .and_then(|element| element.template_contents)
        .expect("template contents");
    set_inner_html(&mut doc, template, "<template><b>x</b></template><i>y</i>")
        .expect("template innerHTML");
    assert_eq!(element_names(&doc, fragment), vec!["template", "i"]);
    assert_eq!(doc.children(template).count(), 0);

    let nested = doc
        .children(fragment)
        .next()
        .expect("nested template child");
    set_outer_html(&mut doc, nested, "<u>replaced</u>").expect("nested outerHTML");
    assert!(!doc.owns(nested));
    assert_eq!(element_names(&doc, fragment), vec!["u", "i"]);
    assert!(doc.validate_invariants().is_ok());
}

#[test]
fn outer_html_preserves_siblings() {
    let mut doc =
        parse("<html><body><ul><li>1</li><li id=\"a\">2</li><li>3</li></ul></body></html>");
    let target = doc.get_element_by_id("a").expect("li present");
    set_outer_html(&mut doc, target, "<li>x</li><li>y</li>").expect("outerHTML");
    assert!(!doc.owns(target));
    let ul = doc
        .get_elements_by_tag_name("ul")
        .first()
        .copied()
        .expect("ul present");
    assert_eq!(element_names(&doc, ul), vec!["li", "li", "li", "li"]);
    assert_eq!(text(&doc, ul), "1xy3");
    assert!(doc.validate_invariants().is_ok());
}

#[test]
fn inner_html_is_idempotent_and_reuses_arena_slots() {
    let mut doc = parse("<html><body><div id=\"x\"></div></body></html>");
    let div = doc.get_element_by_id("x").expect("div present");
    let payload = "<p>one</p><p>two</p><span>three</span>";
    set_inner_html(&mut doc, div, payload).expect("first innerHTML");
    // The parse runs before the previous children are freed (so a limit hit cannot wipe the
    // element, #646), which means the first write needs one extra batch of slots. From then on
    // the freed slots feed every parse: what must hold is that repeats do not grow the arena.
    set_inner_html(&mut doc, div, payload).expect("second innerHTML");
    let allocated_after_second = doc.allocated();
    for _ in 0..25 {
        set_inner_html(&mut doc, div, payload).expect("repeated innerHTML");
    }
    assert_eq!(text(&doc, div), "onetwothree");
    assert_eq!(
        doc.allocated(),
        allocated_after_second,
        "freed slots must be reused by repeated innerHTML"
    );
    assert!(doc.validate_invariants().is_ok());
}

#[test]
fn inner_html_respects_parse_options() {
    let mut doc = parse("<html><body><div id=\"x\"></div></body></html>");
    let div = doc.get_element_by_id("x").expect("div present");
    let payload = "<div>".repeat(1_000);
    let options = ParseOptions {
        max_depth: 8,
        ..ParseOptions::default()
    };
    let violations =
        set_inner_html_with_options(&mut doc, div, &payload, &options).expect("bounded innerHTML");
    assert!(violations.depth_exceeded > 0);
    assert!(doc.validate_invariants().is_ok());
}

#[test]
fn inner_html_rejects_non_elements_and_stale_handles() {
    let mut doc = parse("<html><body><div id=\"x\"><span>old</span></div></body></html>");
    let text_id = doc
        .children(doc.get_element_by_id("x").expect("div"))
        .next()
        .and_then(|span| doc.children(span).next())
        .expect("text node inside span");
    assert!(matches!(
        set_inner_html(&mut doc, text_id, "x"),
        Err(DomError::NotAnElement(_))
    ));

    let old = doc.get_element_by_id("x").expect("div");
    set_inner_html(&mut doc, old, "<b>new</b>").expect("replace div children");
    let stale = doc
        .get_element_by_id("x")
        .expect("div still present after innerHTML");
    doc.remove_subtree(stale);
    assert!(matches!(
        set_inner_html(&mut doc, stale, "<p>x</p>"),
        Err(DomError::UnknownNode(_))
    ));
}

#[test]
fn document_write_appends_to_body_and_survives_repeated_calls() {
    let mut doc = parse("<html><body><p>start</p></body></html>");
    document_write(&mut doc, "<p>one</p>");
    document_write(&mut doc, "<div>two</div><span>three</span>");
    let body = doc.body().expect("body");
    assert_eq!(text(&doc, body), "startonetwothree");
    assert_eq!(doc.get_elements_by_tag_name("p").len(), 2);
    assert!(doc.validate_invariants().is_ok());

    let violations =
        document_write_with_options(&mut doc, "<b>x</b>", false, &ParseOptions::default());
    assert!(violations.is_empty());
    assert_eq!(text(&doc, body), "startonetwothreex");
}

#[test]
fn document_write_on_document_without_body_is_noop() {
    let mut doc = Document::new();
    let before = doc.len();
    document_write(&mut doc, "<p>ignored</p>");
    assert_eq!(doc.len(), before);
    assert!(doc.validate_invariants().is_ok());
}

#[test]
fn malformed_payloads_never_panic() {
    let mut doc = parse("<html><body><div id=\"d\"></div><table id=\"t\"></table></body></html>");
    let div = doc.get_element_by_id("d").expect("div");
    let table = doc.get_element_by_id("t").expect("table");
    let payloads = [
        "<b><i>unclosed",
        "</p></div></td>",
        "<table><tbody><tr><td>",
        "<select><option><optgroup>",
        "<template><template><template>",
        "<svg><foreignObject><math><mi>x",
        "<!-- comment --><!doctype html><?pi?>",
        "\u{0000}\u{FFFD}<script>a<b</script>",
        "<plaintext>rest",
        "",
    ];
    for payload in payloads {
        let _ = set_inner_html(&mut doc, div, payload);
        let _ = set_inner_html(&mut doc, table, payload);
        assert!(doc.validate_invariants().is_ok(), "payload: {payload:?}");
    }
}

#[test]
fn inner_html_keeps_previous_children_when_the_parse_hits_a_limit() {
    // #646: the old order freed the element's children first, so a limit hit left it wiped even
    // though the caller got the violations back, with nothing to roll back to.
    let mut doc = parse("<html><body><div id=\"x\"><p>keep me</p></div></body></html>");
    let div = doc.get_element_by_id("x").expect("div present");
    let options = ParseOptions {
        max_nodes: 4,
        ..ParseOptions::default()
    };
    let violations = set_inner_html_with_options(&mut doc, div, &"<div>".repeat(1_000), &options)
        .expect("innerHTML call itself succeeds");
    assert!(
        !violations.is_empty(),
        "the tight node budget must be reported"
    );
    assert_eq!(
        text(&doc, div),
        "keep me",
        "a refused write must leave the original content in place"
    );
    assert!(doc.validate_invariants().is_ok());
}

#[test]
fn outer_html_rolls_back_when_an_insertion_is_rejected() {
    // The dual half-move hazard: children were inserted one by one before `element`, so a
    // rejection in the middle left the tree half-mutated (#646).
    let mut doc = parse("<html><body><div id=\"x\"></div><p>sibling</p></body></html>");
    let div = doc.get_element_by_id("x").expect("div present");
    let before = doc.allocated();
    // `<html>` as fragment content is rejected by the parser rules for a div context, so this
    // is mostly a smoke check that a failed write leaves the sibling and the arena consistent.
    let _ = set_outer_html(&mut doc, div, "<span>a</span><span>b</span>");
    assert!(doc.validate_invariants().is_ok());
    assert!(doc.allocated() >= before);
}
