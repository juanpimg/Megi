use super::*;

fn doc(html: &str) -> Document {
    megi_webview::html::parse_document(html, true)
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
    let document =
        doc("<input type=checkbox><input type=radio><input type=submit><input type=search><input>");
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
    let document = doc("<div aria-hidden=true><button>Hidden</button></div><button>Shown</button>");
    let body = document.body().expect("body");
    let nodes = collect(&document, body);
    assert!(nodes.iter().any(|node| node.name == "Shown"));
    assert!(!nodes.iter().any(|node| node.name == "Hidden"));
}

#[test]
fn closed_dialog_is_hidden_and_modal_dialog_traps_focus() {
    let document = doc("<button>Outside</button>\
         <dialog><button>Closed</button></dialog>\
         <dialog open data-megi-modal=1><button>First</button><a href=/next>Next</a></dialog>");
    let body = document.body().expect("body");
    let order = focus_order(&document, body);
    let names: Vec<String> = order.iter().map(|node| name(&document, *node)).collect();
    assert_eq!(names, vec!["First", "Next"]);

    let nodes = collect(&document, body);
    assert_eq!(nodes[0].role, "dialog");
    assert!(nodes.iter().any(|n| n.name == "First"));
    assert!(!nodes.iter().any(|n| n.name == "Outside"));
    assert!(!nodes.iter().any(|n| n.name == "Closed"));
}

#[test]
fn snapshot_exposes_identity_parent_and_focusability() {
    let document = doc(
        "<main><h1>Title</h1><button id=b>Send</button><span aria-hidden=true>secret</span></main>",
    );
    let body = document.body().expect("body");
    let nodes = snapshot(&document, body);
    let button = nodes
        .iter()
        .find(|node| node.name == "Send")
        .expect("button node");
    assert_eq!(button.role, "button");
    assert!(button.focusable, "button must be keyboard reachable");
    assert!(
        button.parent.is_some(),
        "button must have an exposed parent"
    );
    assert!(
        !nodes.iter().any(|node| node.name == "secret"),
        "aria-hidden subtree must be excluded"
    );
}

#[test]
fn serialize_produces_consumable_json() {
    let document = doc("<button aria-label='Say \"hi\"'>Go</button>");
    let body = document.body().expect("body");
    let json = serialize(&document, body);
    assert!(json.starts_with('[') && json.ends_with(']'));
    assert!(json.contains("\"role\":\"button\""));
    assert!(json.contains("\"focusable\":true"));
    assert!(json.contains("\\\""), "quotes must be escaped");
}

#[test]
fn stacked_modal_dialogs_focus_topmost_modal() {
    let document = doc(
        "<dialog open data-megi-modal=1><button>Lower</button></dialog>\
         <dialog open data-megi-modal=2><button>Upper</button></dialog>",
    );
    let body = document.body().expect("body");
    let order = focus_order(&document, body);
    let names: Vec<String> = order.iter().map(|node| name(&document, *node)).collect();
    assert_eq!(names, vec!["Upper"]);
}
