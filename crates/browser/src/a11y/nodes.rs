use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessNode {
    /// Explicit `role` or the implicit role for the tag.
    pub role: String,
    /// Accessible name (may be empty for unnamed controls).
    pub name: String,
    /// `aria-disabled`/`disabled`.
    pub disabled: bool,
    /// `aria-hidden`/`hidden`/`inert` (hidden nodes are excluded from focus order).
    pub hidden: bool,
}

pub(crate) fn attribute(document: &Document, node: NodeId, name: &str) -> Option<String> {
    document
        .try_node(node)
        .and_then(|node| node.as_element())
        .and_then(|element| element.get_attribute(name))
        .map(str::to_owned)
}

/// Implicit ARIA role of a tag (subset relevant to the shell's controls and content).
pub fn implicit_role(tag: &str) -> Option<&'static str> {
    Some(match tag {
        "a" | "area" => "link",
        "button" => "button",
        "dialog" => "dialog",
        "input" => "textbox",
        "select" => "combobox",
        "textarea" => "textbox",
        "img" => "img",
        "nav" => "navigation",
        "main" => "main",
        "article" => "article",
        "aside" => "complementary",
        "footer" => "contentinfo",
        "header" => "banner",
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => "heading",
        "ul" | "ol" => "list",
        "li" => "listitem",
        "table" => "table",
        "form" => "form",
        _ => return None,
    })
}

/// Accessible role of one element (explicit `role` wins).
pub fn role(document: &Document, node: NodeId) -> Option<String> {
    let tag = document
        .try_node(node)
        .and_then(|node| node.as_element())
        .map(|element| element.local_name.clone())?;
    let kind = attribute(document, node, "type").unwrap_or_default();
    if tag == "input" {
        return Some(
            match kind.as_str() {
                "checkbox" => "checkbox",
                "radio" => "radio",
                "button" | "submit" | "reset" => "button",
                "search" => "searchbox",
                _ => implicit_role("input").unwrap_or("textbox"),
            }
            .to_string(),
        );
    }
    if tag == "select" {
        return Some(
            if attribute(document, node, "multiple").is_some() {
                "listbox"
            } else {
                "combobox"
            }
            .to_string(),
        );
    }
    attribute(document, node, "role")
        .filter(|role| !role.trim().is_empty())
        .or_else(|| implicit_role(&tag).map(str::to_owned))
}

/// Accessible name: `aria-label`, then the first `aria-labelledby` target's text, then the
/// element's text content or `title`/`alt`/`value` fallbacks.
pub fn name(document: &Document, node: NodeId) -> String {
    if let Some(label) =
        attribute(document, node, "aria-label").filter(|value| !value.trim().is_empty())
    {
        return label;
    }
    if let Some(ids) = attribute(document, node, "aria-labelledby") {
        let mut out = String::new();
        for id in ids.split_whitespace() {
            if let Some(target) = document.get_element_by_id(id) {
                out.push_str(document.text_content(target).trim());
                out.push(' ');
            }
        }
        let out = out.trim();
        if !out.is_empty() {
            return out.to_string();
        }
    }
    for attr in ["title", "alt", "value", "placeholder"] {
        if let Some(value) =
            attribute(document, node, attr).filter(|value| !value.trim().is_empty())
        {
            return value;
        }
    }
    document
        .text_content(node)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Accessible description (`aria-describedby`): text of all referenced elements, joined in
/// document order with single spaces. Unresolved or empty references are skipped.
pub fn description(document: &Document, node: NodeId) -> String {
    let Some(ids) = attribute(document, node, "aria-describedby") else {
        return String::new();
    };
    ids.split_whitespace()
        .filter_map(|id| document.get_element_by_id(id))
        .map(|target| document.text_content(target))
        .flat_map(|text| {
            text.split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn is_closed_dialog(document: &Document, node: NodeId) -> bool {
    document
        .try_node(node)
        .and_then(|n| n.as_element())
        .is_some_and(|el| {
            el.namespace == megi_webview::core::Namespace::Html
                && el.local_name.eq_ignore_ascii_case("dialog")
                && el.get_attribute("open").is_none()
        })
}

pub(crate) fn modal_dialog_order(document: &Document, node: NodeId) -> Option<u32> {
    let el = document.try_node(node)?.as_element()?;
    if el.namespace == megi_webview::core::Namespace::Html
        && el.local_name.eq_ignore_ascii_case("dialog")
        && el.get_attribute("open").is_some()
        && let Some(order) = el.get_attribute("data-megi-modal")
    {
        return Some(order.trim().parse::<u32>().unwrap_or(0));
    }
    None
}

/// Returns the topmost visible modal `<dialog>` under `root`, if one is currently active.
pub fn top_modal_dialog(document: &Document, root: NodeId) -> Option<NodeId> {
    let mut best: Option<(u32, NodeId)> = None;
    for node in document.descendants_inclusive(root) {
        let Some(order) = modal_dialog_order(document, node) else {
            continue;
        };
        if is_hidden(document, node) {
            continue;
        }
        match best {
            Some((best_order, _)) if order < best_order => {}
            _ => best = Some((order, node)),
        }
    }
    best.map(|(_, node)| node)
}

/// Whether the node or one of its ancestors is hidden from assistive tech
/// (`aria-hidden`, `hidden`, `inert`, or a closed `<dialog>`), so hidden subtrees are excluded as a whole.
pub fn is_hidden(document: &Document, node: NodeId) -> bool {
    let direct = ["aria-hidden", "hidden", "inert"].iter().any(|attr| {
        attribute(document, node, attr).is_some_and(|value| !value.eq_ignore_ascii_case("false"))
    });
    if direct || is_closed_dialog(document, node) {
        return true;
    }
    let mut parent = document.try_node(node).and_then(|node| node.parent);
    while let Some(ancestor) = parent {
        if ["aria-hidden", "hidden", "inert"].iter().any(|attr| {
            attribute(document, ancestor, attr)
                .is_some_and(|value| !value.eq_ignore_ascii_case("false"))
        }) || is_closed_dialog(document, ancestor)
        {
            return true;
        }
        parent = document.try_node(ancestor).and_then(|node| node.parent);
    }
    false
}

/// Disabled state (`disabled` or `aria-disabled="true"`).
pub fn is_disabled(document: &Document, node: NodeId) -> bool {
    attribute(document, node, "disabled").is_some()
        || attribute(document, node, "aria-disabled")
            .is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

/// Snapshot of every element under `root` with a role (excluding `aria-hidden` subtrees
/// and background content behind an active modal `<dialog>`).
pub fn collect(document: &Document, root: NodeId) -> Vec<AccessNode> {
    let mut out = Vec::new();
    let nodes: Vec<NodeId> = match top_modal_dialog(document, root) {
        Some(modal) => document.descendants_inclusive(modal).collect(),
        None => document.descendants(root).collect(),
    };
    for node in nodes {
        if is_hidden(document, node) {
            continue;
        }
        let Some(role) = role(document, node) else {
            continue;
        };
        out.push(AccessNode {
            role,
            name: name(document, node),
            disabled: is_disabled(document, node),
            hidden: false,
        });
    }
    out
}
