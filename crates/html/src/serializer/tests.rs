use crate::{parse_document, serialize_fragment, serialize_html, serialize_outer_html};
use megi_core::{Document, NodeId};

#[test]
fn serializer_skips_stale_template_contents_handle() {
    // Removing a template's inert fragment frees its slot but leaves the stale id in
    // ElementData::template_contents. Both serializers must skip it instead of panicking
    // on doc.node() (#32).
    let mut doc = parse_document(
        "<html><body><template><p>inside</p></template></body></html>",
        false,
    );
    let template = find_element(&doc, "template");
    let frag = doc
        .node(template)
        .as_element()
        .and_then(|el| el.template_contents)
        .expect("template contents");
    let _ = doc.remove_subtree(frag);
    assert!(!doc.owns(frag));

    let dump = crate::serialize_dom_for_test(&doc);
    assert!(dump.contains("template"), "{dump}");
    let fragment = serialize_fragment(&doc, template);
    assert!(!fragment.contains("inside"), "{fragment}");
}

fn find_element(doc: &Document, local_name: &str) -> NodeId {
    let mut stack = vec![doc.root()];
    while let Some(id) = stack.pop() {
        if let Some(el) = doc.try_node(id).and_then(|node| node.as_element())
            && el.local_name == local_name
        {
            return id;
        }
        let mut child = doc.try_node(id).and_then(|node| node.first_child);
        while let Some(c) = child {
            stack.push(c);
            child = doc.try_node(c).and_then(|node| node.next_sibling);
        }
    }
    panic!("element <{local_name}> not found");
}

#[test]
fn serializes_document_with_spec_escaping_and_void_elements() {
    let doc = parse_document(
        "<!DOCTYPE html><html><head><title>a &amp; b</title></head>\
         <body><p class=\"x\">Hi &lt;there&gt; &amp; bye<br><img src=\"a\"></p>\
         <script>if (a < b) {}</script></body></html>",
        false,
    );
    assert_eq!(
        serialize_html(&doc),
        "<!DOCTYPE html><html><head><title>a &amp; b</title></head>\
         <body><p class=\"x\">Hi &lt;there&gt; &amp; bye<br><img src=\"a\"></p>\
         <script>if (a < b) {}</script></body></html>"
    );
}

#[test]
fn serializes_template_contents_and_outer_html() {
    let doc = parse_document("<body><template><span>x</span></template></body>", false);
    let template = find_element(&doc, "template");
    assert_eq!(serialize_fragment(&doc, template), "<span>x</span>");
    assert_eq!(
        serialize_outer_html(&doc, template),
        "<template><span>x</span></template>"
    );
}

#[test]
fn escapes_attribute_values_and_non_breaking_spaces() {
    let doc = parse_document("<p title=\"a&\"b\">x\u{00A0}y</p>", false);
    // The parser drops the malformed quote; serialize the DOM as parsed.
    let body = find_element(&doc, "body");
    let html = serialize_fragment(&doc, body);
    assert!(
        html.contains("title=\"a&amp;\"") || html.contains("title=\"a&amp"),
        "attribute escaping: {html}"
    );
    assert!(html.contains("x&nbsp;y"), "text escaping: {html}");
}

#[test]
fn script_text_is_serialized_without_escaping() {
    let doc = parse_document("<body><script>a < b && c > d</script></body>", false);
    let body = find_element(&doc, "body");
    assert_eq!(
        serialize_fragment(&doc, body),
        "<script>a < b && c > d</script>"
    );
}
