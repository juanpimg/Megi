//! Regression tests for orphan-node handling in published AccessKit trees (#989).

use crate::a11y::SnapshotNode;
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
