//! Iterator, invariant, lookups, attributes and text-content tests.

use crate::*;

#[test]
fn test_deep_iterators_are_iterative_and_sized() {
    let mut doc = Document::new();
    let mut parent = doc.root();
    let mut ids = Vec::with_capacity(100_000);
    for _ in 0..100_000 {
        let element = doc.create_element(Namespace::Html, "div", vec![]);
        doc.append_child(parent, element);
        ids.push(element);
        parent = element;
    }
    assert_eq!(doc.descendants(doc.root()).count(), 100_000);
    assert_eq!(doc.descendants_inclusive(doc.root()).count(), 100_001);
    assert_eq!(doc.ancestors(ids[99_999]).count(), 100_000);
    assert!(doc.validate_invariants().is_ok());
}

#[test]
fn test_iterators_terminate_on_corrupted_cycles() {
    let mut doc = Document::new();
    let root = doc.root();
    let a = doc.create_element(Namespace::Html, "div", vec![]);
    let b = doc.create_element(Namespace::Html, "span", vec![]);
    doc.append_child(root, a);
    doc.append_child(a, b);
    // Corrupt the parent chain into a cycle b -> a -> b.
    doc.nodes[a.index()].parent = Some(b);
    assert!(doc.validate_invariants().is_err());
    assert!(doc.ancestors(b).count() <= doc.len() + 1);
    assert!(doc.ancestors(a).count() <= doc.len() + 1);
}

#[test]
fn test_validate_invariants_detects_corruption() {
    let mut doc = Document::new();
    let root = doc.root();
    let a = doc.create_element(Namespace::Html, "div", vec![]);
    let b = doc.create_element(Namespace::Html, "span", vec![]);
    doc.append_child(root, a);
    doc.append_child(a, b);
    assert!(doc.validate_invariants().is_ok());

    // Sibling asymmetry + broken last_child chain.
    doc.nodes[a.index()].next_sibling = Some(b);
    assert!(doc.validate_invariants().is_err());
    doc.nodes[a.index()].next_sibling = None;
    assert!(doc.validate_invariants().is_ok());

    // Child does not point back to its parent.
    doc.nodes[b.index()].parent = Some(root);
    assert!(doc.validate_invariants().is_err());
    doc.nodes[b.index()].parent = Some(a);
    assert!(doc.validate_invariants().is_ok());

    // A detached node (parent == None) is a valid intermediate state for the parser.
    let detached = doc.create_element(Namespace::Html, "p", vec![]);
    assert!(doc.validate_invariants().is_ok());
    doc.append_child(root, detached);
    assert!(doc.validate_invariants().is_ok());

    // Broken free-list bookkeeping: a freed slot marked occupied again.
    let victim = doc.create_element(Namespace::Html, "em", vec![]);
    doc.append_child(root, victim);
    assert!(doc.remove_node(victim));
    let victim_slot = victim.index();
    doc.occupied[victim_slot] = true;
    assert!(doc.validate_invariants().is_err());
    doc.occupied[victim_slot] = false;
    assert!(doc.validate_invariants().is_ok());
}

#[test]
fn test_validate_invariants_rejects_children_under_non_containers() {
    let mut doc = Document::new();
    let root = doc.root();
    let text = doc.create_text_node("hello");
    let child = doc.create_element(Namespace::Html, "span", vec![]);
    doc.append_child(root, text);
    assert!(doc.validate_invariants().is_ok());

    // The public APIs reject this (`CannotHaveChildren`), so build the corruption the way
    // a broken arena import would: link a child under a Text node (#385).
    doc.nodes[text.index()].first_child = Some(child);
    doc.nodes[text.index()].last_child = Some(child);
    doc.nodes[child.index()].parent = Some(text);
    let error = doc
        .validate_invariants()
        .expect_err("children under a Text node");
    assert!(
        error.contains("non-container node holds children"),
        "unexpected error: {error}"
    );
}

#[test]
fn test_stats_and_estimate_bytes() {
    let mut doc = Document::new();
    let root = doc.root();
    assert_eq!(doc.len(), 1);
    assert_eq!(doc.allocated(), 1);
    assert_eq!(doc.free_slots(), 0);
    let empty_estimate = doc.estimate_bytes();
    assert!(empty_estimate >= std::mem::size_of::<Document>());

    let mut ids = Vec::new();
    for index in 0..32 {
        let element = doc.create_element(
            Namespace::Html,
            "div",
            vec![Attribute::new("id", format!("n{index}"))],
        );
        doc.append_child(root, element);
        ids.push(element);
    }
    assert_eq!(doc.len(), 33);
    assert_eq!(doc.allocated(), 33);
    assert_eq!(doc.free_slots(), 0);
    let populated_estimate = doc.estimate_bytes();
    assert!(populated_estimate > empty_estimate);

    for id in ids.iter().take(10) {
        assert!(doc.remove_node(*id));
    }
    assert_eq!(doc.len(), 23);
    assert_eq!(doc.free_slots(), 10);
    assert_eq!(doc.allocated(), 33);
    assert!(
        doc.estimate_bytes() < populated_estimate,
        "removing payloads must lower the estimate"
    );

    for _ in 0..10 {
        let element = doc.create_element(Namespace::Html, "div", vec![]);
        doc.append_child(root, element);
    }
    assert_eq!(doc.len(), 33);
    assert_eq!(doc.allocated(), 33, "reused slots, no arena growth");
    assert!(doc.validate_invariants().is_ok());
}

#[test]
fn test_many_create_remove_cycles_keep_invariants() {
    let mut doc = Document::new();
    let root = doc.root();
    for round in 0..2_000 {
        let element = doc.create_element(Namespace::Html, "div", vec![]);
        doc.append_child(root, element);
        if round % 2 == 0 {
            assert!(doc.remove_node(element));
        }
    }
    assert!(doc.validate_invariants().is_ok());
    assert!(doc.allocated() <= 2_002);
}
#[test]
fn test_document_element_head_body_and_lookups() {
    let mut doc = Document::new();
    let root = doc.root();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let head = doc.create_element(Namespace::Html, "head", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    doc.append_child(root, html);
    doc.append_child(html, head);
    doc.append_child(html, body);
    let nav = doc.create_element(
        Namespace::Html,
        "nav",
        vec![
            Attribute::new("id", "main-nav"),
            Attribute::new("class", "top primary"),
        ],
    );
    let p = doc.create_element(Namespace::Html, "p", vec![]);
    let svg = doc.create_element(Namespace::Svg, "circle", vec![]);
    doc.append_child(body, nav);
    doc.append_child(body, p);
    doc.append_child(body, svg);

    assert_eq!(doc.document_element(), Some(html));
    assert_eq!(doc.head(), Some(head));
    assert_eq!(doc.body(), Some(body));
    assert_eq!(doc.get_element_by_id("main-nav"), Some(nav));
    assert_eq!(doc.get_element_by_id("missing"), None);
    assert_eq!(
        doc.get_elements_by_tag_name("P"),
        vec![p],
        "HTML tag lookup is case-insensitive"
    );
    assert_eq!(doc.get_elements_by_tag_name("nav"), vec![nav]);
    assert_eq!(
        doc.get_elements_by_tag_name("circle"),
        vec![svg],
        "foreign tags match exactly"
    );
    // `"*"` matches every element in any namespace, in document order (#688).
    assert_eq!(
        doc.get_elements_by_tag_name("*"),
        vec![html, head, body, nav, p, svg],
        "wildcard enumerates every element, HTML and foreign alike"
    );
    assert!(
        doc.get_elements_by_tag_name("nope*").is_empty(),
        "only the exact wildcard matches everything"
    );
    // Attribute names are ASCII case-insensitive on HTML elements for every operation
    // (#686): get/has/remove/set must agree, and set must not create duplicates.
    let nav_element = doc.node(nav).as_element().expect("nav element");
    assert_eq!(nav_element.get_attribute("ID"), Some("main-nav"));
    assert!(nav_element.has_attribute("Id"));
    assert_eq!(nav_element.get_attribute("class"), Some("top primary"));
    let mut probe = ElementData::new(Namespace::Html, "div");
    probe.attributes.push(Attribute::new("id", "lower"));
    // `set_attribute` returns true when it inserted, false when it overwrote.
    assert!(!probe.set_attribute(Attribute::new("id", "lower"), true));
    assert!(
        !probe.set_attribute(Attribute::new("ID", "upper"), true),
        "setAttribute with a different case must reuse the existing slot"
    );
    assert_eq!(
        probe.attribute_names(),
        vec!["id".to_string()],
        "setAttribute must not duplicate an attribute that differs only in case"
    );
    assert_eq!(probe.get_attribute("id"), Some("upper"));
    assert_eq!(probe.get_attribute("Id"), Some("upper"));
    assert_eq!(probe.remove_attribute("iD"), Some("upper".to_string()));
    assert_eq!(probe.get_attribute("id"), None);

    assert_eq!(doc.get_elements_by_class_name("primary"), vec![nav]);
    assert_eq!(doc.get_elements_by_class_name("top primary"), vec![nav]);
    assert!(doc.get_elements_by_class_name("top missing").is_empty());
    assert!(doc.get_elements_by_class_name("  ").is_empty());
}

#[test]
fn set_attribute_overwrite_respects_value_byte_cap() {
    // Regression test for #1106: the overwrite path used by scripted setters
    // bypassed MAX_ATTRIBUTE_VALUE_BYTES.
    let mut element = ElementData::new(Namespace::Html, "div");
    assert!(element.set_attribute(Attribute::new("id", "x"), false));
    assert!(
        !element.set_attribute(Attribute::new("id", "y".repeat(1024 * 1024)), true),
        "an oversized overwrite must be rejected"
    );
    assert_eq!(element.get_attribute("id"), Some("x"));
    // A value within the cap still overwrites normally.
    let allowed = "y".repeat(MAX_ATTRIBUTE_VALUE_BYTES);
    assert!(!element.set_attribute(Attribute::new("id", allowed.clone()), true));
    assert_eq!(element.get_attribute("id"), Some(allowed.as_str()));
}

#[test]
fn create_element_enforces_attribute_limits() {
    // #1733: embedders must not bypass the caps enforced by `set_attribute`.
    let mut doc = Document::new();
    let oversized: Vec<Attribute> = (0..MAX_ATTRIBUTES_PER_ELEMENT + 10)
        .map(|_| Attribute::new("a", "x"))
        .collect();
    let element = doc.create_element(Namespace::Html, "div", oversized);
    let attrs = &doc.node(element).as_element().unwrap().attributes;
    assert_eq!(attrs.len(), MAX_ATTRIBUTES_PER_ELEMENT);

    let values = vec![Attribute::new(
        "long",
        "é".repeat(MAX_ATTRIBUTE_VALUE_BYTES),
    )];
    let element = doc.create_element(Namespace::Html, "span", values);
    let value = &doc.node(element).as_element().unwrap().attributes[0].value;
    assert!(value.len() <= MAX_ATTRIBUTE_VALUE_BYTES);
    assert!(value.is_char_boundary(value.len()));
}

#[test]
fn test_attribute_helpers_and_tag_name() {
    let mut doc = Document::new();
    let div = doc.create_element(
        Namespace::Html,
        "div",
        vec![
            Attribute::new("id", "x"),
            Attribute::new("Class", "a"),
            Attribute::new("data-n", "1"),
        ],
    );
    let svg = doc.create_element(Namespace::Svg, "linearGradient", vec![]);

    {
        let element = doc.node(div).as_element().expect("div element");
        assert_eq!(element.tag_name(), "DIV");
        assert!(
            element.has_attribute("ID"),
            "HTML attribute names are case-insensitive"
        );
        assert!(element.has_attribute("class"));
        assert!(!element.has_attribute("missing"));
        assert_eq!(
            element.attribute_names(),
            vec!["id".to_string(), "Class".to_string(), "data-n".to_string()]
        );
        let foreign = doc.node(svg).as_element().expect("svg element");
        assert_eq!(foreign.tag_name(), "linearGradient");
        assert!(!foreign.has_attribute("lineargradient"));
    }
    let removed = doc
        .node_mut(div)
        .as_element_mut()
        .expect("div element")
        .remove_attribute("class");
    assert_eq!(removed.as_deref(), Some("a"));
    assert_eq!(
        doc.node(div)
            .as_element()
            .expect("div element")
            .attribute_names(),
        vec!["id".to_string(), "data-n".to_string()]
    );
}

#[test]
fn test_text_content_and_set_text_content() {
    let mut doc = Document::new();
    let root = doc.root();
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let div = doc.create_element(Namespace::Html, "div", vec![]);
    let text = doc.create_text_node("Hello ");
    let bold = doc.create_element(Namespace::Html, "b", vec![]);
    let bold_text = doc.create_text_node("world");
    doc.append_child(root, body);
    doc.append_child(body, div);
    doc.append_child(div, text);
    doc.append_child(div, bold);
    doc.append_child(bold, bold_text);
    let comment = doc.create_comment("ignored");
    doc.append_child(div, comment);

    assert_eq!(doc.text_content(div), "Hello world");
    assert_eq!(doc.text_content(text), "Hello ");
    assert_eq!(doc.text_content(doc.root()), "");

    doc.set_text_content(div, "replaced")
        .expect("set textContent");
    assert_eq!(doc.text_content(div), "replaced");
    assert_eq!(doc.children(div).count(), 1);
    // Removed children stay alive: DOM-style removal is detach-only.
    assert!(doc.owns(text));
    assert!(doc.owns(bold));
    doc.set_text_content(text, "updated")
        .expect("set text node data");
    assert_eq!(doc.node(text).as_text(), Some("updated"));
    // #1729: setting textContent on Document is a no-op (DOM §4.4), not an error.
    doc.set_text_content(doc.root(), "x")
        .expect("Document textContent is a no-op");
    assert!(doc.validate_invariants().is_ok());
}

#[test]
fn text_content_character_data_and_noop_setters() {
    // #1729 / DOM §4.4: textContent is data for CharacterData (Text, Comment, PI),
    // and setting it on Document/Doctype is a no-op (not an error).
    use crate::NodeKind;
    let mut doc = Document::new();
    let comment = doc.create_comment("a comment");
    let pi = doc.create_node(NodeKind::ProcessingInstruction {
        target: "xml-stylesheet".to_owned(),
        data: "href=style.xsl".to_owned(),
    });
    let doctype = doc.create_node(NodeKind::Doctype {
        name: "html".to_owned(),
        public_id: String::new(),
        system_id: String::new(),
    });
    // A Doctype must be a direct child of the Document to keep invariants valid.
    doc.append_child(doc.root(), doctype);

    assert_eq!(doc.text_content(comment), "a comment");
    assert_eq!(doc.text_content(pi), "href=style.xsl");

    doc.set_text_content(comment, "changed comment")
        .expect("comment data setter");
    assert_eq!(doc.text_content(comment), "changed comment");
    doc.set_text_content(pi, "href=new.xsl")
        .expect("PI data setter");
    assert_eq!(doc.text_content(pi), "href=new.xsl");

    // Document and Doctype setters are silent no-ops.
    doc.set_text_content(doc.root(), "ignored")
        .expect("Document textContent is a no-op");
    doc.set_text_content(doctype, "ignored")
        .expect("Doctype textContent is a no-op");
    assert_eq!(doc.text_content(doc.root()), "");
    assert!(doc.validate_invariants().is_ok());
}

#[test]
fn text_content_truncates_on_utf8_boundary() {
    // Regression test: a byte-count truncation could slice through a multi-byte
    // code point and panic (issue #1105).
    let mut doc = Document::new();
    let div = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(doc.root(), div);
    let _ = doc.append_text(div, "a"); // shift `remaining` to an odd byte offset
    let span = doc.create_element(Namespace::Html, "span", vec![]);
    doc.append_child(div, span);
    let mut big = String::new();
    while big.len() <= Document::MAX_TEXT_CONTENT_BYTES {
        big.push('é');
    }
    let _ = doc.append_text(span, &big);

    // Before the fix this panicked with "end byte index ... is not a char boundary".
    let text = doc.text_content(div);
    assert!(text.len() <= Document::MAX_TEXT_CONTENT_BYTES);
    assert_eq!(&text[..1], "a");
    assert_eq!(text.chars().last(), Some('é'));
}

fn cyclic_text_children(doc: &mut Document, parent: NodeId) {
    let a = doc.append_text(parent, "a").expect("text a");
    let b = doc.append_text(parent, "b").expect("text b");
    let (ai, bi) = (a.index(), b.index());
    doc.nodes[ai].next_sibling = Some(b);
    doc.nodes[bi].next_sibling = Some(a);
}

#[test]
fn text_content_returns_on_cyclic_sibling_chain() {
    let mut doc = Document::new();
    let div = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(doc.root(), div);
    cyclic_text_children(&mut doc, div);
    // Before the fix this hung forever on the cyclic sibling chain (#1107).
    let _ = doc.text_content(div);
}

#[test]
fn normalize_returns_on_cyclic_child_chain() {
    let mut doc = Document::new();
    let div = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(doc.root(), div);
    cyclic_text_children(&mut doc, div);
    // Before the fix this hung forever (#1108).
    let _ = doc.normalize(div);
}

#[test]
fn remove_node_returns_on_cyclic_child_chain() {
    let mut doc = Document::new();
    let div = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(doc.root(), div);
    cyclic_text_children(&mut doc, div);
    // Before the fix this hung forever (#1109).
    assert!(doc.remove_node(div));
}

#[test]
fn reparent_children_returns_on_cyclic_parent_chain() {
    let mut doc = Document::new();
    let a = doc.create_element(Namespace::Html, "div", vec![]);
    let b = doc.create_element(Namespace::Html, "span", vec![]);
    let c = doc.create_element(Namespace::Html, "p", vec![]);
    doc.append_child(doc.root(), a);
    doc.append_child(a, b);
    doc.append_child(doc.root(), c);
    let d = doc.create_text_node("d");
    doc.append_child(c, d);
    let (ai, bi) = (a.index(), b.index());
    doc.nodes[ai].parent = Some(b);
    doc.nodes[bi].parent = Some(a);
    // Before the fix is_ancestor_of(d, a) looped forever here (#1110).
    assert!(!doc.reparent_children(c, a));
}

#[test]
fn append_text_and_insert_text_before_cap_coalescing() {
    let mut doc = Document::new();
    let div = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(doc.root(), div);
    let chunk = "a".repeat(1024 * 1024);
    let mut last_id = None;
    for _ in 0..20 {
        last_id = doc.append_text(div, &chunk);
    }
    let NodeKind::Text(ref data) = doc.node(last_id.unwrap()).kind else {
        panic!("expected text")
    };
    assert!(data.len() <= Document::MAX_TEXT_CONTENT_BYTES);

    let marker = doc.create_element(Namespace::Html, "span", vec![]);
    doc.append_child(div, marker);
    let mut prev_id = None;
    for _ in 0..20 {
        prev_id = doc.insert_text_before(marker, &chunk);
    }
    let NodeKind::Text(ref data) = doc.node(prev_id.unwrap()).kind else {
        panic!("expected text")
    };
    assert!(data.len() <= Document::MAX_TEXT_CONTENT_BYTES);
}
