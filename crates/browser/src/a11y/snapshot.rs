use super::*;

/// A serializable accessibility node with identity and tree relationships (#1516).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotNode {
    /// DOM node id (stable within the document snapshot).
    pub id: NodeId,
    /// Parent snapshot node, if it is also exposed.
    pub parent: Option<NodeId>,
    pub role: String,
    pub name: String,
    pub disabled: bool,
    pub hidden: bool,
    /// Whether the node is reachable by keyboard Tab traversal.
    pub focusable: bool,
    /// Checkbox/radio state from `checked` or `aria-checked`.
    pub checked: Option<bool>,
    /// Toggle-button state from `aria-pressed`.
    pub pressed: Option<bool>,
    /// Disclosure state from `aria-expanded`.
    pub expanded: Option<bool>,
    /// Heading level (h1..h6) or `aria-level`.
    pub level: Option<u8>,
    /// Current control value (`value` attribute or `aria-valuetext`).
    pub value: Option<String>,
    /// ARIA live-region politeness; `None` when the node is not a live region.
    pub live: Option<LivePoliteness>,
    /// Accessible description resolved from `aria-describedby`.
    pub description: String,
}

/// Builds a complete accessibility snapshot: visible, role-bearing nodes in document order with
/// parent links and focusability. This is the consumable form of the tree a screen-reader bridge
/// (or a diagnostic tool) walks; hidden subtrees and closed dialogs are excluded (#1516).
pub fn snapshot(document: &Document, root: NodeId) -> Vec<SnapshotNode> {
    let focusable: std::collections::HashSet<NodeId> =
        focus_order(document, root).into_iter().collect();
    let scope = top_modal_dialog(document, root);
    let mut out: Vec<SnapshotNode> = Vec::new();
    // Walk the same subtree `collect()` uses (modal dialog when one is open).
    let walk_root = scope.unwrap_or(root);
    // Map from DOM id to its nearest exposed ancestor by walking the DOM parent chain.
    for node in document.descendants_inclusive(walk_root) {
        if is_hidden(document, node) || is_closed_dialog(document, node) {
            continue;
        }
        let Some(role) = role(document, node) else {
            continue;
        };
        let mut parent = None;
        let mut current = document.try_node(node).and_then(|n| n.parent);
        while let Some(id) = current {
            if out.iter().any(|exposed| exposed.id == id) {
                parent = Some(id);
                break;
            }
            current = document.try_node(id).and_then(|n| n.parent);
        }
        let live = live_politeness(document, node, &role);
        out.push(SnapshotNode {
            id: node,
            parent,
            role,
            name: name(document, node),
            disabled: is_disabled(document, node),
            hidden: false,
            focusable: focusable.contains(&node),
            checked: bool_attr(document, node, &["aria-checked", "checked"]),
            pressed: bool_attr(document, node, &["aria-pressed"]),
            expanded: bool_attr(document, node, &["aria-expanded"]),
            level: heading_level(document, node),
            value: attribute(document, node, "value")
                .or_else(|| attribute(document, node, "aria-valuetext")),
            live,
            description: description(document, node),
        });
    }
    out
}

fn bool_attr(document: &Document, node: NodeId, names: &[&str]) -> Option<bool> {
    for name in names {
        if let Some(value) = attribute(document, node, name) {
            // A bare boolean attribute (`<input checked>`) has an empty value and means true.
            return Some(value.is_empty() || value.eq_ignore_ascii_case("true"));
        }
    }
    None
}

fn heading_level(document: &Document, node: NodeId) -> Option<u8> {
    if let Some(value) = attribute(document, node, "aria-level")
        && let Ok(level) = value.parse::<u8>()
    {
        return Some(level);
    }
    let tag = document
        .try_node(node)
        .and_then(|node| node.as_element())
        .map(|element| element.local_name.as_str())?;
    match tag {
        "h1" => Some(1),
        "h2" => Some(2),
        "h3" => Some(3),
        "h4" => Some(4),
        "h5" => Some(5),
        "h6" => Some(6),
        _ => None,
    }
}

/// Serializes a snapshot to JSON (no external crate): an array of nodes with their identity,
/// parent, role, name and states. Consumable by tooling and a future AT-SPI/accesskit bridge.
pub fn serialize(document: &Document, root: NodeId) -> String {
    let nodes = snapshot(document, root);
    let mut json = String::from("[");
    for (index, node) in nodes.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str(&format!(
            "{{\"id\":{},\"parent\":{},\"role\":\"{}\",\"name\":\"{}\",\"disabled\":{},\"hidden\":{},\"focusable\":{}}}",
            node.id.get(),
            node.parent.map_or(0, |id| id.get()),
            escape_json(&node.role),
            escape_json(&node.name),
            node.disabled,
            node.hidden,
            node.focusable,
        ));
    }
    json.push(']');
    json
}

pub(crate) fn escape_json(value: &str) -> String {
    value
        .chars()
        .flat_map(|ch| match ch {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            c => vec![c],
        })
        .collect()
}

/// Deterministic Tab order: focusable, enabled, visible elements with a role, in document
/// order (`tabindex` is only used through its presence; negative values are skipped).
/// When an open modal `<dialog>` is active, focus is trapped within the topmost modal dialog.
pub fn focus_order(document: &Document, root: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let scope = top_modal_dialog(document, root).unwrap_or(root);
    for node in document.descendants(scope) {
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

/// Resolves `aria-live` with the implicit politeness of alert/status/log roles.
fn live_politeness(document: &Document, node: NodeId, role: &str) -> Option<LivePoliteness> {
    if let Some(value) = attribute(document, node, "aria-live") {
        let politeness = match value.trim().to_ascii_lowercase().as_str() {
            "off" => LivePoliteness::Off,
            "polite" => LivePoliteness::Polite,
            "assertive" => LivePoliteness::Assertive,
            _ => return None,
        };
        return Some(politeness);
    }
    match role {
        "alert" => Some(LivePoliteness::Assertive),
        "status" | "log" => Some(LivePoliteness::Polite),
        _ => None,
    }
}
