//! WHATWG HTML §13.3 serialization rules that had no dedicated test (`#1353`).
//!
//! Each case states the rule it pins: void elements have no end tag, attribute and text
//! values are escaped with the exact character maps, raw-text elements keep their content
//! verbatim, and doctypes/comments round-trip. They are deliberately end-to-end (parse then
//! serialize) because that is how the engine uses the serializer.

use html::{parse_document, serialize_html, serialize_outer_html};

fn find_first(doc: &html::Document, local_name: &str) -> Option<html::NodeId> {
    doc.descendants(doc.root()).find(|id| {
        doc.node(*id)
            .as_element()
            .is_some_and(|e| e.local_name == local_name)
    })
}

fn roundtrip(input: &str) -> String {
    serialize_html(&parse_document(input, false))
}

#[test]
fn void_elements_have_no_end_tag_and_no_children() {
    let out = roundtrip("<!DOCTYPE html><p>a<br>b<img src=\"x.png\">");
    assert!(
        out.contains("<br>"),
        "br must serialize as a void tag: {out}"
    );
    assert!(!out.contains("</br>"), "br must not get an end tag: {out}");
    assert!(
        out.contains("<img src=\"x.png\">"),
        "img keeps its attribute: {out}"
    );
    assert!(
        !out.contains("</img>"),
        "img must not get an end tag: {out}"
    );
}

#[test]
fn text_values_escape_ampersand_lower_greater_and_nbsp() {
    let out = roundtrip("<p>a &amp; b &lt; c &gt; d\u{a0}e</p>");
    assert!(
        out.contains("a &amp; b &lt; c &gt; d&nbsp;e"),
        "text escaping: {out}"
    );
}

#[test]
fn attribute_values_escape_ampersand_quote_and_nbsp() {
    let out = roundtrip("<p title=\"a&amp;b &quot;q&quot; x\u{a0}y\">t</p>");
    assert!(
        out.contains("title=\"a&amp;b &quot;q&quot; x&nbsp;y\""),
        "attribute escaping: {out}"
    );
}

#[test]
fn raw_text_elements_keep_their_content_verbatim() {
    let out =
        roundtrip("<script>if (a < b && c > d) {}</script><style>a > b { color: red }</style>");
    assert!(
        out.contains("if (a < b && c > d) {}"),
        "script content is raw: {out}"
    );
    assert!(
        out.contains("a > b { color: red }"),
        "style content is raw: {out}"
    );
}

#[test]
fn doctype_and_comments_round_trip() {
    let out = roundtrip("<!DOCTYPE html><!--note--><p>x</p>");
    assert!(out.starts_with("<!DOCTYPE html>"), "doctype first: {out}");
    assert!(out.contains("<!--note-->"), "comment preserved: {out}");
}

#[test]
fn pre_newlines_follow_the_parse_and_serialize_rules() {
    // The parser drops the first newline after <pre> (WHATWG "ignore the next token" rule),
    // and the serializer must not invent one back.
    let stripped = roundtrip("<pre>\nline</pre>");
    assert!(
        stripped.contains("<pre>line</pre>"),
        "the parser strips the first newline and the serializer does not add it back: {stripped}"
    );
    // A text node that really starts with a newline keeps it: the serializer only mirrors the
    // character, so parse(serialize(x)) stays stable.
    let kept = roundtrip("<pre>\n\nline</pre>");
    assert!(
        kept.contains("<pre>\nline</pre>"),
        "a real leading newline survives serialization: {kept}"
    );
}

#[test]
fn boolean_attributes_serialize_with_an_empty_value() {
    let out = roundtrip("<input disabled>");
    assert!(out.contains("disabled=\"\""), "boolean attribute: {out}");
}

#[test]
fn outer_html_of_a_detached_element_serializes_that_element_only() {
    let doc = parse_document(
        "<!DOCTYPE html><div id=\"host\"><span class=\"a\">x</span></div>",
        false,
    );
    let host = find_first(&doc, "div").expect("div");
    let span = doc.node(host).first_child.expect("span");
    assert_eq!(
        serialize_outer_html(&doc, span),
        "<span class=\"a\">x</span>"
    );
    assert_eq!(serialize_outer_html(&doc, doc.root()), serialize_html(&doc));
}

#[test]
fn an_unknown_element_serializes_with_its_children() {
    let out = roundtrip("<p><my-widget>text</my-widget></p>");
    assert!(
        out.contains("<my-widget>text</my-widget>"),
        "unknown element: {out}"
    );
}
