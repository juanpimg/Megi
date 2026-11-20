//! Platform accessibility backend (ADR-002): maps Megi's deterministic shell accessibility
//! snapshot to AccessKit nodes, which the platform adapters publish as AT-SPI (Linux), UI
//! Automation (Windows) or NSAccessibility (macOS).
//!
//! This module performs no native I/O: it is a pure, testable mapping. The platform adapter is
//! activated by the window shell behind the `a11y` feature; headless rendering is unaffected.

use accesskit::{Action, Node, NodeId, Role, Toggled, TreeInfo, TreeUpdate};

use crate::a11y::SnapshotNode;
use megi_webview::core::NodeId as DomNodeId;

/// Maps a Megi semantic role string (from `a11y::role`) to an AccessKit role.
pub fn map_role(role: &str) -> Role {
    match role.to_ascii_lowercase().as_str() {
        "document" => Role::Document,
        "button" => Role::Button,
        "heading" => Role::Heading,
        "link" => Role::Link,
        "textbox" | "input" | "searchbox" => Role::TextInput,
        "checkbox" => Role::CheckBox,
        "radio" => Role::RadioButton,
        "img" | "image" => Role::Image,
        "list" => Role::List,
        "listitem" => Role::ListItem,
        "table" => Role::Table,
        "row" => Role::Row,
        "cell" | "columnheader" | "rowheader" => Role::Cell,
        "navigation" => Role::Navigation,
        "main" => Role::Main,
        "banner" => Role::Banner,
        "contentinfo" => Role::ContentInfo,
        "dialog" => Role::Dialog,
        "alert" => Role::Alert,
        "progressbar" => Role::ProgressIndicator,
        "region" | "section" => Role::Section,
        "label" => Role::Label,
        _ => Role::GenericContainer,
    }
}

fn accesskit_id(id: DomNodeId) -> NodeId {
    NodeId(id.get() as u64)
}

/// Builds a complete `TreeUpdate` from a shell accessibility snapshot.
///
/// The first node is the tree root unless another node is explicitly designated; each snapshot
/// node carries its DOM parent, so the parent/child links are reconstructed here. Interactive
/// nodes advertise the `Click` and `Focus` actions; disabled nodes only advertise `Focus`.
pub fn build_tree_update(snapshot: &[SnapshotNode], focused: DomNodeId) -> TreeUpdate {
    let mut nodes = Vec::with_capacity(snapshot.len() + 1);
    for entry in snapshot {
        let role = map_role(&entry.role);
        let mut node = Node::new(role);
        if !entry.name.is_empty() {
            node.set_label(entry.name.clone());
        }
        if entry.disabled {
            node.set_disabled();
        }
        if entry.focusable {
            node.add_action(Action::Focus);
        }
        if let Some(checked) = entry.checked {
            node.set_toggled(if checked {
                Toggled::True
            } else {
                Toggled::False
            });
        }
        if let Some(pressed) = entry.pressed {
            node.set_toggled(if pressed {
                Toggled::True
            } else {
                Toggled::False
            });
        }
        if let Some(expanded) = entry.expanded {
            node.set_expanded(expanded);
        }
        if let Some(level) = entry.level {
            node.set_level(usize::from(level));
        }
        if let Some(value) = &entry.value {
            node.set_value(value.clone());
        }
        if matches!(
            role,
            Role::Button
                | Role::Link
                | Role::CheckBox
                | Role::RadioButton
                | Role::MenuItem
                | Role::ListBoxOption
        ) && !entry.disabled
        {
            node.add_action(Action::Click);
        }
        nodes.push((accesskit_id(entry.id), node));
    }
    if nodes.is_empty() {
        return TreeUpdate {
            nodes,
            tree: None,
            focus: accesskit_id(focused),
            tree_id: accesskit::TreeId::ROOT,
        };
    }

    // Rebuild parent→children links from each snapshot node's DOM parent.
    let parent_of: Vec<Option<NodeId>> = snapshot
        .iter()
        .map(|entry| entry.parent.map(accesskit_id))
        .collect();
    let mut children: Vec<Vec<NodeId>> = vec![Vec::new(); nodes.len()];
    for (index, parent) in parent_of.iter().enumerate() {
        if let Some(parent) = parent
            && let Some(parent_index) = nodes.iter().position(|(id, _)| id == parent)
        {
            children[parent_index].push(nodes[index].0);
        }
    }
    for (index, (_, node)) in nodes.iter_mut().enumerate() {
        if !children[index].is_empty() {
            node.set_children(children[index].clone());
        }
    }

    let root = nodes[0].0;
    let mut tree = TreeInfo::new(root);
    tree.toolkit_name = Some("Megi".to_owned());
    tree.toolkit_version = Some(env!("CARGO_PKG_VERSION").to_owned());
    // The currently focused DOM node becomes the AccessKit tree focus (when present).
    let _ = focused;
    TreeUpdate {
        nodes,
        tree: Some(tree),
        focus: accesskit_id(focused),
        tree_id: accesskit::TreeId::ROOT,
    }
}

/// FNV-1a signature of a shell accessibility snapshot plus the focused DOM node.
///
/// Used to avoid re-publishing an identical tree to the platform adapter on every frame.
pub fn snapshot_signature(snapshot: &[SnapshotNode], focused: DomNodeId) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut mix = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
    };
    for node in snapshot {
        mix(node.id.get().to_be_bytes().as_slice());
        mix(node.role.as_bytes());
        mix(node.name.as_bytes());
        mix(&[
            u8::from(node.disabled),
            u8::from(node.hidden),
            u8::from(node.focusable),
        ]);
    }
    mix(focused.get().to_be_bytes().as_slice());
    hash
}

/// Pure state deciding whether a tree must be re-published (TDD / ADR-002).
#[derive(Debug, Default)]
pub struct AccessibilityPublisher {
    last_signature: Option<u64>,
    publications: usize,
}

impl AccessibilityPublisher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of trees published so far (diagnostics and tests).
    pub fn publications(&self) -> usize {
        self.publications
    }

    /// Returns `true` only when `signature` differs from the last published one.
    pub fn should_publish(&mut self, signature: u64) -> bool {
        if self.last_signature == Some(signature) {
            return false;
        }
        self.last_signature = Some(signature);
        self.publications += 1;
        true
    }

    /// Forces the next update to be a full tree (AT re-activation / new document).
    pub fn invalidate(&mut self) {
        self.last_signature = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_mapping_covers_common_semantic_roles() {
        assert_eq!(map_role("button"), Role::Button);
        assert_eq!(map_role("LINK"), Role::Link);
        assert_eq!(map_role("heading"), Role::Heading);
        assert_eq!(map_role("checkbox"), Role::CheckBox);
        assert_eq!(map_role("unmapped"), Role::GenericContainer);
    }

    #[test]
    fn empty_snapshot_produces_an_empty_update() {
        let update = build_tree_update(&[], DomNodeId::from_index(0));
        assert!(update.nodes.is_empty());
        assert!(update.tree.is_none());
    }
}

#[cfg(test)]
mod publisher_tests {
    use super::*;
    use crate::a11y::SnapshotNode;
    use megi_webview::core::NodeId;

    fn node(id: usize, role: &str, name: &str) -> SnapshotNode {
        SnapshotNode {
            id: NodeId::from_index(id),
            parent: None,
            role: role.to_owned(),
            name: name.to_owned(),
            disabled: false,
            hidden: false,
            focusable: false,
            checked: None,
            pressed: None,
            expanded: None,
            level: None,
            value: None,
        }
    }

    #[test]
    fn signature_changes_with_name_disabled_and_focus() {
        let a = snapshot_signature(&[node(1, "button", "Go")], NodeId::from_index(1));
        let renamed = snapshot_signature(&[node(1, "button", "Stop")], NodeId::from_index(1));
        assert_ne!(a, renamed, "name must affect the signature");

        let mut disabled = node(1, "button", "Go");
        disabled.disabled = true;
        let disabled_sig = snapshot_signature(&[disabled], NodeId::from_index(1));
        assert_ne!(a, disabled_sig, "disabled must affect the signature");

        let refocused = snapshot_signature(&[node(1, "button", "Go")], NodeId::from_index(2));
        assert_ne!(a, refocused, "focused node must affect the signature");
    }

    #[test]
    fn publisher_skips_identical_snapshots() {
        let mut publisher = AccessibilityPublisher::new();
        let snapshot = [node(1, "button", "Go")];
        let signature = snapshot_signature(&snapshot, NodeId::from_index(1));
        assert!(
            publisher.should_publish(signature),
            "first update is published"
        );
        assert!(
            !publisher.should_publish(signature),
            "identical snapshot must not be re-published"
        );
        assert_eq!(publisher.publications(), 1);
    }

    #[test]
    fn publisher_republishes_after_invalidation() {
        let mut publisher = AccessibilityPublisher::new();
        let signature = snapshot_signature(&[node(1, "button", "Go")], NodeId::from_index(1));
        assert!(publisher.should_publish(signature));
        publisher.invalidate();
        assert!(
            publisher.should_publish(signature),
            "invalidate forces a full-tree republish (AT re-activation)"
        );
        assert_eq!(publisher.publications(), 2);
    }
}
