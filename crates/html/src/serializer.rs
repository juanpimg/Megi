//! # Canonical `#document` Tree Serializer (`html5lib-tests` & WPT `serializeTree`)
//!
//! Serializes a [`dom::Document`] into the exact line-oriented format expected by
//! `html5lib-tests/tree-construction/*.dat` and WPT `html/syntax/parsing/resources/test.js`.
//!
//! The traversal is iterative (explicit heap work stack) so that deeply nested DOM trees
//! cannot overflow the call stack (`F6` in the core audit: a 50k-level DOM used to abort
//! the process here).

use dom::{Attribute, Document, Namespace, NodeId, NodeKind};

/// One unit of pending serialization work.
enum Work {
    /// Emit the line(s) for `node_id` at `depth`, then schedule its children.
    Node { node_id: NodeId, depth: usize },
    /// Walk the sibling chain starting at `next`, visiting every node at `depth`.
    ChildList { next: Option<NodeId>, depth: usize },
}

fn format_attr_name(attr: &Attribute) -> String {
    match attr.namespace {
        Namespace::XLink => format!("xlink {}", attr.local_name),
        Namespace::Xml => format!("xml {}", attr.local_name),
        Namespace::Xmlns => format!("xmlns {}", attr.local_name),
        _ => attr.local_name.clone(),
    }
}

fn push_child_list(stack: &mut Vec<Work>, first_child: Option<NodeId>, depth: usize) {
    if let Some(child_id) = first_child {
        stack.push(Work::ChildList {
            next: Some(child_id),
            depth,
        });
    }
}

fn serialize_node(
    doc: &Document,
    node_id: NodeId,
    depth: usize,
    out: &mut String,
    stack: &mut Vec<Work>,
) {
    let indent = "  ".repeat(depth);
    let node = doc.node(node_id);

    match &node.kind {
        NodeKind::Document | NodeKind::DocumentFragment => {
            push_child_list(stack, node.first_child, depth);
        }
        NodeKind::Doctype {
            name,
            public_id,
            system_id,
        } => {
            if public_id.is_empty() && system_id.is_empty() {
                out.push_str(&format!("| {indent}<!DOCTYPE {name}>\n"));
            } else {
                out.push_str(&format!(
                    "| {indent}<!DOCTYPE {name} \"{public_id}\" \"{system_id}\">\n"
                ));
            }
        }
        NodeKind::Text(text) => {
            out.push_str(&format!("| {indent}\"{text}\"\n"));
        }
        NodeKind::Comment(comment) => {
            out.push_str(&format!("| {indent}<!-- {comment} -->\n"));
        }
        NodeKind::ProcessingInstruction { target, data } => {
            out.push_str(&format!("| {indent}<?{target} {data}?>\n"));
        }
        NodeKind::Element(el) => {
            let tag_str = match el.namespace {
                Namespace::Svg => format!("svg {}", el.local_name),
                Namespace::MathMl => format!("math {}", el.local_name),
                _ => el.local_name.clone(),
            };
            out.push_str(&format!("| {indent}<{tag_str}>\n"));

            let attr_indent = "  ".repeat(depth + 1);
            let mut sorted_attrs: Vec<(String, &str)> = el
                .attributes
                .iter()
                .map(|a| (format_attr_name(a), a.value.as_str()))
                .collect();
            sorted_attrs.sort_by(|a, b| a.0.cmp(&b.0));

            for (attr_name, attr_val) in sorted_attrs {
                out.push_str(&format!("| {attr_indent}{attr_name}=\"{attr_val}\"\n"));
            }

            // Children and (for `<template>`) inert contents are processed in document
            // order. Because the work stack is LIFO, push the element children first so
            // that the template contents (already emitted after the `content` marker) are
            // popped before them.
            push_child_list(stack, node.first_child, depth + 1);
            if el.namespace == Namespace::Html
                && el.local_name == "template"
                && let Some(frag_id) = el.template_contents
            {
                out.push_str(&format!("| {attr_indent}content\n"));
                push_child_list(stack, doc.node(frag_id).first_child, depth + 2);
            }
        }
    }
}

/// Serializes `doc` into the canonical `#document` tree dump string (without trailing newline).
pub fn serialize_dom_for_test(doc: &Document) -> String {
    let mut out = String::new();
    let mut stack: Vec<Work> = Vec::new();
    push_child_list(&mut stack, doc.node(doc.root()).first_child, 0);

    while let Some(work) = stack.pop() {
        match work {
            Work::ChildList { next: None, .. } => {}
            Work::ChildList {
                next: Some(id),
                depth,
            } => {
                stack.push(Work::ChildList {
                    next: doc.node(id).next_sibling,
                    depth,
                });
                stack.push(Work::Node { node_id: id, depth });
            }
            Work::Node { node_id, depth } => {
                serialize_node(doc, node_id, depth, &mut out, &mut stack);
            }
        }
    }

    if out.ends_with('\n') {
        out.pop();
    }
    out
}

// ============================================================================
// WHATWG HTML §13.3 — Serializing HTML fragments (public API)
// https://html.spec.whatwg.org/multipage/parsing.html#serialising-html-fragments
// ============================================================================

/// Void elements per WHATWG HTML §13.1.2 (`#void-elements`): serialized without an end tag.
const HTML_VOID_ELEMENTS: &[&str] = &[
    "area", "base", "basefont", "bgsound", "br", "col", "embed", "frame", "hr", "img", "input",
    "keygen", "link", "meta", "param", "source", "track", "wbr",
];

/// Elements whose text children are serialized without escaping in the HTML fragment
/// serialization algorithm (WHATWG HTML §13.3, step "if current node is a style, script, xmp,
/// iframe, noembed, noframes, or plaintext element").
const HTML_RAWTEXT_ELEMENTS: &[&str] = &[
    "style",
    "script",
    "xmp",
    "iframe",
    "noembed",
    "noframes",
    "plaintext",
];

/// Options for the WHATWG HTML §13.3 fragment serializer.
///
/// The HTML fragment serialization algorithm (used by `innerHTML`/`outerHTML` getters) always
/// emits double-quoted attributes and escapes text; the only context-dependent input is the
/// scripting flag, needed to decide whether a `<noscript>` element's text is raw text (scripting
/// enabled) or normal escaped text (scripting disabled).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HtmlSerializeOptions {
    /// Whether scripting is enabled (HTML §13.3: `<noscript>` content is serialized as raw text
    /// when scripting is enabled).
    pub scripting_enabled: bool,
}

impl HtmlSerializeOptions {
    /// Options matching a page with scripting disabled (the parser default used by `parse_*`).
    pub const fn scripting_disabled() -> Self {
        Self {
            scripting_enabled: false,
        }
    }

    /// Options matching a page with scripting enabled.
    pub const fn scripting_enabled() -> Self {
        Self {
            scripting_enabled: true,
        }
    }
}

/// Appends `text` to `out`, escaping the characters the HTML fragment serialization algorithm
/// escapes in character data: `&` (as `&amp;`), U+00A0 (as `&nbsp;`), `<` (as `&lt;`) and `>`
/// (as `&gt;`).
fn escape_text(text: &str, out: &mut String) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '\u{00A0}' => out.push_str("&nbsp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
}

/// Appends an attribute value, escaping `&` (as `&amp;`), U+00A0 (as `&nbsp;`) and `"` (as
/// `&quot;`), per WHATWG HTML §13.3 attribute serialization.
fn escape_attribute(value: &str, out: &mut String) {
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '\u{00A0}' => out.push_str("&nbsp;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
}

/// Returns the tag name to serialize for an element: local name for HTML/MathML/SVG elements,
/// qualified name otherwise (WHATWG HTML §13.3, "serializing HTML fragments", element step).
fn element_tag_name(el: &dom::ElementData) -> String {
    match el.namespace {
        Namespace::Html | Namespace::Svg | Namespace::MathMl => el.local_name.clone(),
        _ => match &el.prefix {
            Some(prefix) => format!("{prefix}:{}", el.local_name),
            None => el.local_name.clone(),
        },
    }
}

/// Returns the serialized attribute name, mapping namespaces to their prefixes
/// (WHATWG HTML §13.3 attribute step; `xmlns`, `xmlns:x`, `xml:*`, `xlink:*`).
fn attribute_qualified_name(attr: &Attribute) -> String {
    match attr.namespace {
        Namespace::Xmlns => {
            if attr.local_name == "xmlns" {
                "xmlns".to_string()
            } else {
                format!("xmlns:{}", attr.local_name)
            }
        }
        Namespace::Xml => format!("xml:{}", attr.local_name),
        Namespace::XLink => format!("xlink:{}", attr.local_name),
        _ => match &attr.prefix {
            Some(prefix) => format!("{prefix}:{}", attr.local_name),
            None => attr.local_name.clone(),
        },
    }
}

fn is_void_html_element(el: &dom::ElementData) -> bool {
    el.namespace == Namespace::Html && HTML_VOID_ELEMENTS.contains(&el.local_name.as_str())
}

fn is_rawtext_element(el: &dom::ElementData, options: HtmlSerializeOptions) -> bool {
    if el.namespace != Namespace::Html {
        return false;
    }
    if HTML_RAWTEXT_ELEMENTS.contains(&el.local_name.as_str()) {
        return true;
    }
    el.local_name == "noscript" && options.scripting_enabled
}

/// Appends a single text node, escaping unless the parent is a raw-text element.
fn serialize_text_node(text: &str, rawtext_context: bool, out: &mut String) {
    if rawtext_context {
        out.push_str(text);
    } else {
        escape_text(text, out);
    }
}

/// One unit of pending DOM serialization work.
enum HtmlSerializeWork {
    /// Serialize `node_id` (element open/close, or leaf node) in the given context.
    Node {
        node_id: NodeId,
        rawtext_context: bool,
    },
    /// Serialize the siblings starting at `next` in the given context.
    ChildList {
        next: Option<NodeId>,
        rawtext_context: bool,
    },
    /// Append the element end tag (unless void); children are queued between open and close.
    CloseElement { node_id: NodeId },
}

/// Appends the serialization of the children of `parent_id` (WHATWG HTML §13.3 fragment
/// algorithm: the "children" of the context node).
fn serialize_children(
    doc: &Document,
    parent_id: NodeId,
    options: HtmlSerializeOptions,
    out: &mut String,
) {
    let child_root = doc
        .node(parent_id)
        .as_element()
        .filter(|el| el.namespace == Namespace::Html && el.local_name == "template")
        .and_then(|el| el.template_contents)
        .unwrap_or(parent_id);
    let stack = vec![HtmlSerializeWork::ChildList {
        next: doc.node(child_root).first_child,
        rawtext_context: is_rawtext_element_node(doc, parent_id, options),
    }];
    run_serialization(doc, options, stack, out);
}

/// Serializes a node's children as an HTML fragment (the `innerHTML` getter algorithm, WHATWG
/// HTML §13.3 "Serializing HTML fragments").
///
/// Escaping follows §13.3: text escapes `&`, U+00A0, `<`, `>`; attribute values are always
/// double-quoted and escape `&`, U+00A0, `"`; void elements have no end tag; raw-text elements
/// (`style`, `script`, `xmp`, `iframe`, `noembed`, `noframes`, `plaintext`) and `<noscript>` with
/// scripting enabled serialize their text unescaped; `<template>` serializes its content
/// fragment. Namespaces are serialized with the `xmlns`/`xml:*`/`xlink:*` prefixes and HTML/
/// MathML/SVG elements use their local (case-preserving) name.
pub fn serialize_fragment(doc: &Document, root: NodeId) -> String {
    serialize_fragment_with_options(doc, root, HtmlSerializeOptions::scripting_disabled())
}

/// [`serialize_fragment`] with an explicit [`HtmlSerializeOptions`].
pub fn serialize_fragment_with_options(
    doc: &Document,
    root: NodeId,
    options: HtmlSerializeOptions,
) -> String {
    if !doc.owns(root) {
        return String::new();
    }
    let mut out = String::new();
    serialize_children(doc, root, options, &mut out);
    out
}

/// Returns whether the children of `node_id` must be serialized as raw text.
fn is_rawtext_element_node(doc: &Document, node_id: NodeId, options: HtmlSerializeOptions) -> bool {
    doc.node(node_id)
        .as_element()
        .is_some_and(|el| is_rawtext_element(el, options))
}

/// Runs the explicit-work-stack traversal. The stack keeps the serializer iterative so deeply
/// nested DOM trees cannot overflow the call stack.
fn run_serialization(
    doc: &Document,
    options: HtmlSerializeOptions,
    mut stack: Vec<HtmlSerializeWork>,
    out: &mut String,
) {
    while let Some(work) = stack.pop() {
        match work {
            HtmlSerializeWork::ChildList { next: None, .. } => {}
            HtmlSerializeWork::ChildList {
                next: Some(id),
                rawtext_context,
            } => {
                stack.push(HtmlSerializeWork::ChildList {
                    next: doc.node(id).next_sibling,
                    rawtext_context,
                });
                stack.push(HtmlSerializeWork::Node {
                    node_id: id,
                    rawtext_context,
                });
            }
            HtmlSerializeWork::CloseElement { node_id } => {
                if let Some(el) = doc.node(node_id).as_element()
                    && !is_void_html_element(el)
                {
                    out.push_str("</");
                    out.push_str(&element_tag_name(el));
                    out.push('>');
                }
            }
            HtmlSerializeWork::Node {
                node_id,
                rawtext_context,
            } => serialize_node_into(doc, node_id, rawtext_context, options, out, &mut stack),
        }
    }
}

/// Serializes the full document (doctype + root element), equivalent to serializing the children
/// of the document node per WHATWG HTML §13.3.
pub fn serialize_html(doc: &Document) -> String {
    serialize_fragment(doc, doc.root())
}

/// [`serialize_html`] with an explicit [`HtmlSerializeOptions`].
pub fn serialize_html_with_options(doc: &Document, options: HtmlSerializeOptions) -> String {
    serialize_fragment_with_options(doc, doc.root(), options)
}

/// Serializes a node itself (the `outerHTML` getter algorithm, WHATWG HTML §13.3).
pub fn serialize_outer_html(doc: &Document, node_id: NodeId) -> String {
    serialize_outer_html_with_options(doc, node_id, HtmlSerializeOptions::scripting_disabled())
}

/// [`serialize_outer_html`] with an explicit [`HtmlSerializeOptions`].
pub fn serialize_outer_html_with_options(
    doc: &Document,
    node_id: NodeId,
    options: HtmlSerializeOptions,
) -> String {
    if !doc.owns(node_id) {
        return String::new();
    }
    let mut out = String::new();
    run_serialization(
        doc,
        options,
        vec![HtmlSerializeWork::Node {
            node_id,
            rawtext_context: false,
        }],
        &mut out,
    );
    out
}

/// Serializes one node into `out`, pushing follow-up work (children/close tag) onto `stack`.
fn serialize_node_into(
    doc: &Document,
    node_id: NodeId,
    rawtext_context: bool,
    options: HtmlSerializeOptions,
    out: &mut String,
    stack: &mut Vec<HtmlSerializeWork>,
) {
    let node = doc.node(node_id);
    match &node.kind {
        NodeKind::Document | NodeKind::DocumentFragment => {
            stack.push(HtmlSerializeWork::ChildList {
                next: node.first_child,
                rawtext_context,
            });
        }
        NodeKind::Doctype { name, .. } => {
            out.push_str("<!DOCTYPE ");
            out.push_str(name);
            out.push('>');
        }
        NodeKind::Text(text) => serialize_text_node(text, rawtext_context, out),
        NodeKind::Comment(data) => {
            out.push_str("<!--");
            out.push_str(data);
            out.push_str("-->");
        }
        NodeKind::ProcessingInstruction { target, data } => {
            out.push_str("<?");
            out.push_str(target);
            if !data.is_empty() {
                out.push(' ');
                out.push_str(data);
            }
            out.push('>');
        }
        NodeKind::Element(el) => {
            out.push('<');
            out.push_str(&element_tag_name(el));
            for attr in &el.attributes {
                out.push(' ');
                out.push_str(&attribute_qualified_name(attr));
                out.push_str("=\"");
                escape_attribute(&attr.value, out);
                out.push('"');
            }
            out.push('>');
            if !is_void_html_element(el) {
                stack.push(HtmlSerializeWork::CloseElement { node_id });
                let child_root = if el.namespace == Namespace::Html && el.local_name == "template" {
                    el.template_contents.unwrap_or(node_id)
                } else {
                    node_id
                };
                stack.push(HtmlSerializeWork::ChildList {
                    next: doc.node(child_root).first_child,
                    rawtext_context: is_rawtext_element(el, options),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse_document, serialize_fragment, serialize_html, serialize_outer_html};

    fn find_element(doc: &Document, local_name: &str) -> NodeId {
        let mut stack = vec![doc.root()];
        while let Some(id) = stack.pop() {
            if let Some(el) = doc.node(id).as_element()
                && el.local_name == local_name
            {
                return id;
            }
            let mut child = doc.node(id).first_child;
            while let Some(c) = child {
                stack.push(c);
                child = doc.node(c).next_sibling;
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
}
