//! Regression tests for ARIA live-region announcements (alpha limitation: no live-region
//! announcements). RED first: the snapshot model and AT bus have no concept of `aria-live`.

use crate::a11y::{LivePoliteness, snapshot};
use crate::a11y_at_bus::AtBus;
use crate::a11y_backend::build_tree_update;
use accesskit_consumer::Tree;
use megi_webview::core::{Attribute, Document, Namespace};

fn live_page() -> (Document, megi_webview::core::NodeId) {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let polite = doc.create_element(
        Namespace::Html,
        "div",
        vec![
            Attribute::new("role", "status"),
            Attribute::new("aria-live", "polite"),
        ],
    );
    assert!(doc.append_child(doc.root(), html));
    assert!(doc.append_child(html, body));
    assert!(doc.append_child(body, polite));
    (doc, polite)
}

#[test]
fn aria_live_is_captured_in_the_accessibility_snapshot() {
    let (doc, polite) = live_page();
    let nodes = snapshot(&doc, doc.root());
    let entry = nodes
        .iter()
        .find(|node| node.id == polite)
        .expect("live node");
    assert_eq!(entry.live, Some(LivePoliteness::Polite));
}

#[test]
fn published_accesskit_node_carries_live_politeness() {
    let (doc, polite) = live_page();
    let nodes = snapshot(&doc, doc.root());
    let root = nodes[0].id;
    let update = build_tree_update(&nodes, root);
    let tree = Tree::new(update, true);
    fn has_live(node: &accesskit_consumer::NodeRef<'_>) -> bool {
        node.live() == accesskit::Live::Polite || node.children().any(|child| has_live(&child))
    }
    assert!(
        has_live(&tree.state().root()),
        "AT must see the live region politeness"
    );
    let _ = polite;
}

#[test]
fn at_bus_announces_live_region_text_changes() {
    let (doc, polite) = live_page();
    let nodes = snapshot(&doc, doc.root());
    let root = nodes[0].id;
    let mut bus = AtBus::new();
    bus.publish(build_tree_update(&nodes, root));
    let announced_before = bus.log().announcements.len();

    // The page updates the live region's text; the next snapshot exposes a new label.
    let mut changed = nodes.clone();
    let entry = changed.iter_mut().find(|node| node.id == polite).unwrap();
    entry.name = "Saved".to_owned();
    bus.publish(build_tree_update(&changed, root));

    assert_eq!(bus.log().announcements.len(), announced_before + 1);
    let announcement = bus.log().announcements.last().expect("announcement");
    assert_eq!(announcement.politeness, LivePoliteness::Polite);
    assert!(
        announcement.text.contains("Saved"),
        "announcement must carry the new text: {announcement:?}"
    );
}
