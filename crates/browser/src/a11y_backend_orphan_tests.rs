//! Regression tests for orphan-node handling in published AccessKit trees (#989).

use crate::a11y::SnapshotNode;
use crate::a11y_at_bus::AtBus;
use crate::a11y_backend::build_tree_update;
use accesskit_consumer::Tree;
use megi_webview::core::NodeId as DomNodeId;

fn node(id: usize, role: &str, parent: Option<DomNodeId>) -> SnapshotNode {
    SnapshotNode {
        id: DomNodeId::from_index(id),
        parent,
        role: role.to_owned(),
        name: role.to_owned(),
        disabled: false,
        hidden: false,
        focusable: false,
        checked: None,
        pressed: None,
        expanded: None,
        level: None,
        value: None,
        live: None,
    }
}

#[test]
fn snapshot_nodes_without_an_exposed_parent_mount_under_the_root() {
    // Live AT activation (IsEnabled=true) panicked: nodes whose DOM ancestors have no
    // role were left as extra roots in the TreeUpdate (#989). Every exposed node except
    // the root must have a parent in the tree.
    let root = DomNodeId::from_index(1);
    let snapshot = [
        node(1, "document", None),
        node(2, "heading", None),
        node(3, "button", None),
    ];
    let update = build_tree_update(&snapshot, root);
    let tree = Tree::new(update, true);
    let children: Vec<_> = tree.state().root().children().collect();
    assert_eq!(children.len(), 2, "orphan nodes must become root children");
}

#[test]
fn at_bus_applies_incremental_updates_and_records_announcements() {
    // A live AT observes *incremental* TreeUpdates through the consumer, not whole-tree
    // rebuilds. This surrogate applies those updates exactly as a platform adapter does.
    use accesskit::{Action, Role, Toggled};
    let root = DomNodeId::from_index(1);
    let node =
        |id: usize, role: &str, name: &str, checked: Option<bool>, parent: Option<DomNodeId>| {
            SnapshotNode {
                id: DomNodeId::from_index(id),
                parent,
                role: role.to_owned(),
                name: name.to_owned(),
                disabled: false,
                hidden: false,
                focusable: true,
                checked,
                pressed: None,
                expanded: None,
                level: None,
                value: None,
                live: None,
            }
        };
    let mut bus = AtBus::new();
    let initial = [
        node(1, "document", "", None, None),
        node(2, "checkbox", "Remember", Some(false), Some(root)),
    ];
    bus.publish(build_tree_update(&initial, root));
    assert_eq!(
        bus.log().added.len(),
        2,
        "AT sees every root node announced"
    );
    assert_eq!(bus.focused_role(), Some(Role::Document));

    // Incremental update: same tree, checkbox becomes checked and focused.
    let changed = [
        node(1, "document", "", None, None),
        node(2, "checkbox", "Remember", Some(true), Some(root)),
    ];
    bus.publish(build_tree_update(&changed, DomNodeId::from_index(2)));
    assert!(
        bus.log()
            .updated
            .iter()
            .any(|label| label.contains("Remember")),
        "the checkbox state change must be announced: {:?}",
        bus.log().updated
    );
    assert_eq!(
        bus.log().focus_moves,
        2,
        "initial + incremental focus must be announced"
    );
    assert_eq!(bus.focused_role(), Some(Role::CheckBox));
    assert!(bus.toggled_in_focus() == Some(Toggled::True));
    assert!(bus.focus_supports_action(Action::Click));
}
