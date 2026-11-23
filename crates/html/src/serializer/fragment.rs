//! WHATWG HTML §13.3 — serializing HTML fragments (public API).

use megi_core::{Attribute, Document, Namespace, NodeId, NodeKind};

use super::tree_dump::{MAX_SERIALIZED_BYTES, truncate_to_limit};

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
    /// Ceiling for the produced string ([`MAX_SERIALIZED_BYTES`] by default). Past it the
    /// serializer appends `<!-- output truncated -->` and stops (#647).
    pub max_output_bytes: usize,
}

impl HtmlSerializeOptions {
    /// Options matching a page with scripting disabled (the parser default used by `parse_*`).
    pub const fn scripting_disabled() -> Self {
        Self {
            scripting_enabled: false,
            max_output_bytes: MAX_SERIALIZED_BYTES,
        }
    }

    /// Returns these options with a different output ceiling.
    pub const fn with_max_output_bytes(mut self, limit: usize) -> Self {
        self.max_output_bytes = limit;
        self
    }

    /// Options matching a page with scripting enabled.
    pub const fn scripting_enabled() -> Self {
        Self {
            scripting_enabled: true,
            max_output_bytes: MAX_SERIALIZED_BYTES,
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
fn element_tag_name(el: &megi_core::ElementData) -> String {
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

fn is_void_html_element(el: &megi_core::ElementData) -> bool {
    el.namespace == Namespace::Html && HTML_VOID_ELEMENTS.contains(&el.local_name.as_str())
}

fn is_rawtext_element(el: &megi_core::ElementData, options: HtmlSerializeOptions) -> bool {
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
        next: doc.try_node(child_root).and_then(|node| node.first_child),
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
    doc.try_node(node_id)
        .and_then(|node| node.as_element())
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
    let max_steps = doc.len().saturating_mul(16).max(64);
    let mut steps = 0usize;
    while let Some(work) = stack.pop() {
        // Terminate on a corrupt sibling cycle instead of growing the stack forever (#32).
        if steps >= max_steps {
            break;
        }
        steps += 1;
        if out.len() >= options.max_output_bytes {
            truncate_to_limit(out, options.max_output_bytes);
            out.push_str("<!-- output truncated -->");
            return;
        }
        match work {
            HtmlSerializeWork::ChildList { next: None, .. } => {}
            HtmlSerializeWork::ChildList {
                next: Some(id),
                rawtext_context,
            } => {
                stack.push(HtmlSerializeWork::ChildList {
                    next: doc.try_node(id).and_then(|node| node.next_sibling),
                    rawtext_context,
                });
                stack.push(HtmlSerializeWork::Node {
                    node_id: id,
                    rawtext_context,
                });
            }
            HtmlSerializeWork::CloseElement { node_id } => {
                if let Some(el) = doc.try_node(node_id).and_then(|node| node.as_element())
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
    if out.len() >= options.max_output_bytes {
        truncate_to_limit(out, options.max_output_bytes);
        out.push_str("<!-- output truncated -->");
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
    let Some(node) = doc.try_node(node_id) else {
        return;
    };
    match &node.kind {
        NodeKind::Document | NodeKind::DocumentFragment => {
            stack.push(HtmlSerializeWork::ChildList {
                next: node.first_child,
                rawtext_context,
            });
        }
        NodeKind::Doctype {
            name,
            public_id,
            system_id,
        } => {
            out.push_str("<!DOCTYPE ");
            out.push_str(name);
            if !public_id.is_empty() {
                out.push_str(" PUBLIC \"");
                out.push_str(public_id);
                out.push('"');
                if !system_id.is_empty() {
                    out.push(' ');
                    out.push('"');
                    out.push_str(system_id);
                    out.push('"');
                }
            } else if !system_id.is_empty() {
                out.push_str(" SYSTEM \"");
                out.push_str(system_id);
                out.push('"');
            }
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
                    next: doc.try_node(child_root).and_then(|node| node.first_child),
                    rawtext_context: is_rawtext_element(el, options),
                });
            }
        }
    }
}
