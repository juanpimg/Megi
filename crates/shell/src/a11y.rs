//! Minimal accessibility model for the shell: implicit/explicit ARIA roles, names,
//! hidden/disabled state and a deterministic Tab focus order.
//!
//! The DOM already stores every `aria-*` attribute; this module exposes a small, testable
//! snapshot (role/name/disabled/hidden) and the focus traversal used by `Tab`/`Shift+Tab`.
//! Limits: no full ARIA spec (only the common roles/attributes), no screen-reader bridge,
//! no live-region announcements and no computed accessible-name algorithm beyond
//! `aria-label` → `aria-labelledby` → text/attribute fallback.

use dom::{Document, NodeId};

/// One accessibility node exposed by the shell.
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

fn attribute(document: &Document, node: NodeId, name: &str) -> Option<String> {
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

/// Whether the node or one of its ancestors is hidden from assistive tech
/// (`aria-hidden`, `hidden` or `inert`), so hidden subtrees are excluded as a whole.
pub fn is_hidden(document: &Document, node: NodeId) -> bool {
    let direct = ["aria-hidden", "hidden", "inert"].iter().any(|attr| {
        attribute(document, node, attr).is_some_and(|value| !value.eq_ignore_ascii_case("false"))
    });
    if direct {
        return true;
    }
    let mut parent = document.try_node(node).and_then(|node| node.parent);
    while let Some(ancestor) = parent {
        if ["aria-hidden", "hidden", "inert"].iter().any(|attr| {
            attribute(document, ancestor, attr)
                .is_some_and(|value| !value.eq_ignore_ascii_case("false"))
        }) {
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

/// Snapshot of every element under `root` with a role (excluding `aria-hidden` subtrees).
pub fn collect(document: &Document, root: NodeId) -> Vec<AccessNode> {
    let mut out = Vec::new();
    for node in document.descendants(root) {
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

/// Deterministic Tab order: focusable, enabled, visible elements with a role, in document
/// order (`tabindex` is only used through its presence; negative values are skipped).
pub fn focus_order(document: &Document, root: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    for node in document.descendants(root) {
        if is_hidden(document, node) || is_disabled(document, node) {
            continue;
        }
        let Some(role) = role(document, node) else {
            continue;
        };
        let focusable = matches!(
            role.as_str(),
            "link"
                | "button"
                | "textbox"
                | "searchbox"
                | "checkbox"
                | "radio"
                | "combobox"
                | "listbox"
        );
        if !focusable {
            continue;
        }
        if attribute(document, node, "tabindex").is_some_and(|value| value.trim().starts_with('-'))
        {
            continue;
        }
        out.push(node);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(html: &str) -> Document {
        html::parse_document(html, true)
    }

    #[test]
    fn roles_cover_explicit_and_implicit_tags() {
        let document = doc(
            "<button>Go</button><input type=checkbox><a href=/x>link</a><div role=alert>w</div><select multiple><option>a</option></select>",
        );
        let body = document.body().expect("body");
        let roles: Vec<String> = document
            .descendants(body)
            .filter_map(|node| role(&document, node))
            .collect();
        assert!(roles.iter().any(|r| r == "button"), "{roles:?}");
        assert!(roles.iter().any(|r| r == "checkbox"));
        assert!(roles.iter().any(|r| r == "link"));
        assert!(roles.iter().any(|r| r == "alert"));
        assert!(roles.iter().any(|r| r == "listbox"));
    }

    #[test]
    fn names_hidden_and_disabled_states() {
        let document = doc(
            "<button aria-label=Save>X</button><button aria-disabled=true>D</button><p aria-hidden=true>hidden</p><span id=l>Label</span><button aria-labelledby=l>Y</button>",
        );
        let body = document.body().expect("body");
        let buttons: Vec<_> = document
            .descendants(body)
            .filter(|node| role(&document, *node).as_deref() == Some("button"))
            .collect();
        assert_eq!(name(&document, buttons[0]), "Save");
        assert!(is_disabled(&document, buttons[1]));
        assert_eq!(name(&document, buttons[2]), "Label");
        let hidden = document
            .descendants(body)
            .find(|node| {
                role(&document, *node).is_none()
                    && document
                        .try_node(*node)
                        .and_then(|n| n.as_element())
                        .is_some_and(|e| e.local_name == "p")
            })
            .expect("p");
        assert!(is_hidden(&document, hidden));
    }

    #[test]
    fn focus_order_skips_hidden_and_disabled_controls() {
        let document = doc(
            "<button>One</button><button disabled>Two</button><button aria-hidden=true>Three</button><a href=/x>Four</a><button aria-disabled=true>Five</button>",
        );
        let body = document.body().expect("body");
        let order = focus_order(&document, body);
        let names: Vec<String> = order.iter().map(|node| name(&document, *node)).collect();
        assert_eq!(names, vec!["One", "Four"]);
    }

    #[test]
    fn access_snapshot_exposes_aria_attributes() {
        let document = doc(
            "<main><h1>T</h1><form><input aria-label=Email><button aria-disabled=true>Send</button></form></main>",
        );
        let body = document.body().expect("body");
        let nodes = collect(&document, body);
        assert!(nodes.iter().any(|node| node.role == "main"));
        assert!(
            nodes
                .iter()
                .any(|node| node.role == "heading" && node.name == "T")
        );
        assert!(
            nodes
                .iter()
                .any(|node| node.role == "textbox" && node.name == "Email")
        );
        assert!(
            nodes
                .iter()
                .any(|node| node.role == "button" && node.disabled)
        );
    }

    #[test]
    fn input_types_get_specific_roles() {
        let document = doc(
            "<input type=checkbox><input type=radio><input type=submit><input type=search><input>",
        );
        let body = document.body().expect("body");
        let roles: Vec<String> = document
            .descendants(body)
            .filter_map(|node| role(&document, node))
            .collect();
        assert_eq!(
            roles,
            vec!["checkbox", "radio", "button", "searchbox", "textbox"]
        );
    }

    #[test]
    fn hidden_aria_subtree_is_excluded() {
        let document =
            doc("<div aria-hidden=true><button>Hidden</button></div><button>Shown</button>");
        let body = document.body().expect("body");
        let nodes = collect(&document, body);
        assert!(nodes.iter().any(|node| node.name == "Shown"));
        assert!(!nodes.iter().any(|node| node.name == "Hidden"));
    }
}
