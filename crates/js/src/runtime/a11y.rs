//! Deterministic accessibility tree (F7-A11Y).
//!
//! A structured, platform-independent accessibility tree derived from the DOM. It is the
//! engine-side foundation that a platform backend (AT-SPI / UIA / NSAccessibility) consumes;
//! this module intentionally performs no native I/O and uses no `unsafe`.

use megi_core::{Document, NodeId};

/// Maximum number of accessible nodes in one snapshot (anti-DoS).
pub(crate) const MAX_A11Y_NODES: usize = 4096;

/// One node of the accessibility tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct A11yNode {
    pub role: String,
    pub name: String,
    pub description: String,
    pub level: Option<u8>,
    pub value: Option<String>,
    pub states: Vec<String>,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub dom_id: NodeId,
}

/// A flat, indexed accessibility tree with a root list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AccessibilityTree {
    pub nodes: Vec<A11yNode>,
    pub roots: Vec<usize>,
}

impl AccessibilityTree {
    /// Serializes a plain-text outline — handy for diagnostics and snapshot tests.
    #[cfg(test)]
    pub fn outline(&self) -> String {
        fn visit(tree: &AccessibilityTree, index: usize, depth: usize, out: &mut String) {
            let node = &tree.nodes[index];
            out.push_str(&"  ".repeat(depth));
            out.push_str(&node.role);
            if let Some(level) = node.level {
                out.push_str(&format!("[{level}]"));
            }
            if !node.name.is_empty() {
                out.push_str(&format!(" '{}'", node.name));
            }
            if !node.states.is_empty() {
                out.push_str(&format!(" ({})", node.states.join(",")));
            }
            out.push('\n');
            for child in &node.children {
                visit(tree, *child, depth + 1, out);
            }
        }
        let mut out = String::new();
        for root in &self.roots {
            visit(self, *root, 0, &mut out);
        }
        out
    }
}

fn attribute<'a>(element: &'a megi_core::ElementData, name: &str) -> Option<&'a str> {
    element.get_attribute(name)
}

fn is_hidden(element: &megi_core::ElementData) -> bool {
    if attribute(element, "aria-hidden").is_some_and(|v| v.eq_ignore_ascii_case("true")) {
        return true;
    }
    if attribute(element, "hidden").is_some() {
        return true;
    }
    attribute(element, "style").is_some_and(|style| {
        let style = style.to_ascii_lowercase();
        style.contains("display:none") || style.contains("visibility:hidden")
    })
}

fn implicit_role(tag: &str) -> Option<(&'static str, Option<u8>)> {
    Some(match tag {
        "a" => ("link", None),
        "button" | "summary" => ("button", None),
        "h1" => ("heading", Some(1)),
        "h2" => ("heading", Some(2)),
        "h3" => ("heading", Some(3)),
        "h4" => ("heading", Some(4)),
        "h5" => ("heading", Some(5)),
        "h6" => ("heading", Some(6)),
        "img" => ("img", None),
        "input" | "textarea" | "select" => ("textbox", None),
        "li" => ("listitem", None),
        "ul" | "ol" => ("list", None),
        "nav" => ("navigation", None),
        "main" => ("main", None),
        "header" => ("banner", None),
        "footer" => ("contentinfo", None),
        "table" => ("table", None),
        "tr" => ("row", None),
        "td" | "th" => ("cell", None),
        "form" => ("form", None),
        "label" => ("label", None),
        "progress" => ("progressbar", None),
        "dialog" => ("dialog", None),
        "details" => ("group", None),
        "option" => ("option", None),
        "section" | "article" => ("region", None),
        "body" => ("document", None),
        _ => return None,
    })
}

/// Input type refines the implicit role of `<input>` (WHATWG HTML §4.10.5).
fn input_role(element: &megi_core::ElementData) -> Option<(&'static str, Option<u8>)> {
    if !element.local_name.eq_ignore_ascii_case("input") {
        return None;
    }
    Some(
        match attribute(element, "type")
            .unwrap_or("text")
            .to_ascii_lowercase()
            .as_str()
        {
            "checkbox" => ("checkbox", None),
            "radio" => ("radio", None),
            "button" | "submit" | "reset" => ("button", None),
            _ => ("textbox", None),
        },
    )
}

/// Explicit ARIA roles that map to a supported accessibility role.
fn explicit_role(role: &str) -> Option<(&'static str, Option<u8>)> {
    Some(match role.to_ascii_lowercase().as_str() {
        "button" => ("button", None),
        "link" => ("link", None),
        "checkbox" => ("checkbox", None),
        "radio" => ("radio", None),
        "textbox" | "searchbox" => ("textbox", None),
        "img" => ("img", None),
        "heading" => ("heading", None),
        "navigation" => ("navigation", None),
        "main" => ("main", None),
        "list" => ("list", None),
        "listitem" => ("listitem", None),
        "table" => ("table", None),
        "row" => ("row", None),
        "cell" | "columnheader" | "rowheader" => ("cell", None),
        "dialog" => ("dialog", None),
        "alert" => ("alert", None),
        "progressbar" => ("progressbar", None),
        "group" => ("group", None),
        "region" => ("region", None),
        "banner" => ("banner", None),
        "contentinfo" => ("contentinfo", None),
        "form" => ("form", None),
        "option" => ("option", None),
        "menuitem" => ("menuitem", None),
        _ => return None,
    })
}

fn referenced_names(document: &Document, ids: &str) -> String {
    ids.split_ascii_whitespace()
        .filter_map(|id| document.get_element_by_id(id))
        .map(|node| document.text_content(node))
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_owned()
}

fn accessible_name(document: &Document, node: NodeId, element: &megi_core::ElementData) -> String {
    if let Some(ids) = attribute(element, "aria-labelledby") {
        let name = referenced_names(document, ids);
        if !name.is_empty() {
            return name;
        }
    }
    if let Some(label) = attribute(element, "aria-label") {
        return label.trim().to_owned();
    }
    if let Some(id) = attribute(element, "id") {
        for candidate in document.descendants(document.root()) {
            let Some(_label) = document
                .try_node(candidate)
                .and_then(|node| node.as_element())
                .filter(|element| element.local_name.eq_ignore_ascii_case("label"))
                .filter(|element| attribute(element, "for") == Some(id))
            else {
                continue;
            };
            let name = document.text_content(candidate).trim().to_owned();
            if !name.is_empty() {
                return name;
            }
        }
    }
    if matches!(element.local_name.as_str(), "img" | "input")
        && let Some(alt) = attribute(element, "alt")
    {
        return alt.trim().to_owned();
    }
    if let Some(title) = attribute(element, "title") {
        return title.trim().to_owned();
    }
    document.text_content(node).trim().to_owned()
}

fn collect_states(
    element: &megi_core::ElementData,
    focused: Option<NodeId>,
    node: NodeId,
) -> Vec<String> {
    let mut states = Vec::new();
    if focused == Some(node) {
        states.push("focused".to_owned());
    }
    if attribute(element, "disabled").is_some()
        || attribute(element, "aria-disabled").is_some_and(|v| v.eq_ignore_ascii_case("true"))
    {
        states.push("disabled".to_owned());
    }
    if attribute(element, "checked").is_some()
        || attribute(element, "aria-checked").is_some_and(|v| v.eq_ignore_ascii_case("true"))
    {
        states.push("checked".to_owned());
    }
    if attribute(element, "required").is_some()
        || attribute(element, "aria-required").is_some_and(|v| v.eq_ignore_ascii_case("true"))
    {
        states.push("required".to_owned());
    }
    if attribute(element, "aria-expanded").is_some_and(|v| v.eq_ignore_ascii_case("true"))
        || (element.local_name.eq_ignore_ascii_case("details")
            && attribute(element, "open").is_some())
    {
        states.push("expanded".to_owned());
    }
    if attribute(element, "aria-pressed").is_some_and(|v| v.eq_ignore_ascii_case("true")) {
        states.push("pressed".to_owned());
    }
    if attribute(element, "aria-invalid").is_some_and(|v| v.eq_ignore_ascii_case("true")) {
        states.push("invalid".to_owned());
    }
    states
}

/// Builds an accessibility tree from the current DOM (#1740 follow-up / F7-A11Y).
pub(crate) fn build_accessibility_tree(
    document: &Document,
    focused: Option<NodeId>,
) -> AccessibilityTree {
    let mut tree = AccessibilityTree::default();
    // An `aria-hidden`/`hidden` ancestor hides its entire subtree from the accessibility tree.
    let mut hidden: std::collections::HashSet<NodeId> = std::collections::HashSet::new();
    for node in document.descendants(document.root()) {
        if let Some(element) = document.try_node(node).and_then(|n| n.as_element())
            && is_hidden(element)
        {
            hidden.insert(node);
        }
    }
    // Stack of (DOM ancestor, accessible-node index) used to establish parent/child links.
    let mut ancestors: Vec<(NodeId, usize)> = Vec::new();
    for node in document.descendants(document.root()) {
        if hidden
            .iter()
            .any(|dom_id| is_ancestor_of(document, *dom_id, node))
        {
            continue;
        }
        let Some(element) = document.try_node(node).and_then(|n| n.as_element()) else {
            continue;
        };
        if hidden.contains(&node) {
            continue;
        }
        let (role, level) = attribute(element, "role")
            .and_then(explicit_role)
            .or_else(|| input_role(element))
            .or_else(|| implicit_role(&element.local_name))
            .unwrap_or(("generic", None));
        while ancestors
            .last()
            .is_some_and(|(dom_id, _)| !is_ancestor_of(document, *dom_id, node))
        {
            ancestors.pop();
        }
        let parent = ancestors.last().map(|(_, index)| *index);
        let mut a11y_node = A11yNode {
            role: role.to_owned(),
            name: accessible_name(document, node, element),
            description: attribute(element, "aria-describedby")
                .map(|ids| referenced_names(document, ids))
                .unwrap_or_default(),
            level,
            value: attribute(element, "value").map(str::to_owned),
            states: collect_states(element, focused, node),
            parent,
            children: Vec::new(),
            dom_id: node,
        };
        // Avoid a `document` node whose name is the entire page text.
        if a11y_node.role == "document" && a11y_node.name.len() > 256 {
            a11y_node.name.truncate(256);
        }
        if tree.nodes.len() >= MAX_A11Y_NODES {
            break;
        }
        let index = tree.nodes.len();
        if let Some(parent) = parent {
            tree.nodes[parent].children.push(index);
        } else {
            tree.roots.push(index);
        }
        tree.nodes.push(a11y_node);
        ancestors.push((node, index));
    }
    tree
}

fn is_ancestor_of(document: &Document, ancestor: NodeId, node: NodeId) -> bool {
    let mut current = Some(node);
    while let Some(id) = current {
        if id == ancestor {
            return true;
        }
        current = document.try_node(id).and_then(|n| n.parent);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(html: &str) -> megi_core::Document {
        megi_html::parse_document(html, false)
    }

    #[test]
    fn tree_reflects_dom_hierarchy_and_roles() {
        let doc = document(
            "<!doctype html><body><nav><ul><li><a href='/'>Inicio</a></li></ul></nav></body>",
        );
        let tree = build_accessibility_tree(&doc, None);
        let outline = tree.outline();
        assert!(outline.contains("navigation"));
        assert!(outline.contains("'Inicio'"));
        let roles: Vec<_> = tree.nodes.iter().map(|node| node.role.as_str()).collect();
        assert!(roles.contains(&"navigation"));
        assert!(roles.contains(&"list"));
        assert!(roles.contains(&"listitem"));
        let link = tree
            .nodes
            .iter()
            .find(|node| node.role == "link")
            .expect("link node");
        assert_eq!(link.name, "Inicio");
        let list = tree.nodes.iter().find(|node| node.role == "list").unwrap();
        let list_index = tree
            .nodes
            .iter()
            .position(|node| node.role == "list")
            .unwrap();
        assert!(
            list.children
                .iter()
                .any(|index| tree.nodes[*index].role == "listitem")
        );
        assert_eq!(
            tree.nodes[list_index].parent.is_some(),
            list.parent.is_some()
        );
    }

    #[test]
    fn aria_label_and_labelledby_override_text() {
        let doc = document(
            "<!doctype html><body><h1 id='t'>Titulo</h1>\
             <button aria-label='Cerrar'>x</button>\
             <button aria-labelledby='t'>otro</button></body>",
        );
        let tree = build_accessibility_tree(&doc, None);
        let buttons: Vec<_> = tree.nodes.iter().filter(|n| n.role == "button").collect();
        assert_eq!(buttons[0].name, "Cerrar");
        assert_eq!(buttons[1].name, "Titulo");
        let heading = tree.nodes.iter().find(|n| n.role == "heading").unwrap();
        assert_eq!(heading.level, Some(1));
    }

    #[test]
    fn hidden_subtrees_are_excluded() {
        let doc = document(
            "<!doctype html><body><div aria-hidden='true'><button>Oculto</button></div>\
             <button>Visible</button></body>",
        );
        let tree = build_accessibility_tree(&doc, None);
        assert!(!tree.nodes.iter().any(|n| n.name == "Oculto"));
        assert!(tree.nodes.iter().any(|n| n.name == "Visible"));
    }

    #[test]
    fn states_reflect_aria_and_focus() {
        let doc = document(
            "<!doctype html><body><input id='a' type='checkbox' checked required disabled>\
             <button aria-expanded='true' aria-invalid='true'>B</button></body>",
        );
        let focused = doc.get_element_by_id("a");
        let tree = build_accessibility_tree(&doc, focused);
        let checkbox = tree.nodes.iter().find(|n| n.role == "checkbox").unwrap();
        for state in ["checked", "required", "disabled", "focused"] {
            assert!(
                checkbox.states.iter().any(|s| s == state),
                "missing {state}"
            );
        }
        let button = tree.nodes.iter().find(|n| n.role == "button").unwrap();
        assert!(button.states.iter().any(|s| s == "expanded"));
        assert!(button.states.iter().any(|s| s == "invalid"));
    }
}
