//! R155: render wires the browser host providers (History/location/Web Storage),
//! dispatches host events (hashchange/popstate) before the final re-render and delivers
//! MutationObserver records through microtask checkpoints.

#![cfg(feature = "js")]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use dom::Document;
use render::{HistoryAction, LocationAction, RenderOptions, RenderStorage, Viewport, render_url};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render")
}

fn fixture_path(name: &str) -> String {
    format!("file://{}", fixture_dir().join(name).display())
}

fn options() -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(200, 300).expect("viewport"),
        ..RenderOptions::default()
    }
}

fn render_fixture_with(name: &str, options: &RenderOptions) -> render::RenderOutput {
    render_url(&fixture_path(name), options)
        .unwrap_or_else(|error| panic!("render {name}: {error}"))
}

fn render_fixture(name: &str) -> render::RenderOutput {
    render_fixture_with(name, &options())
}

fn element_with_id(document: &Document, id: &str) -> dom::NodeId {
    document
        .descendants(document.root())
        .find(|node_id| {
            document
                .node(*node_id)
                .as_element()
                .and_then(|element| element.get_attribute("id"))
                == Some(id)
        })
        .unwrap_or_else(|| panic!("fixture element #{id} missing"))
}

fn text_content(output: &render::RenderOutput, id: &str) -> String {
    output
        .document
        .text_content(element_with_id(&output.document, id))
}

fn count_where(pixmap: &paint::Pixmap, predicate: impl Fn([u8; 4]) -> bool) -> usize {
    pixmap
        .pixels
        .iter()
        .filter(|pixel| predicate([pixel.r, pixel.g, pixel.b, pixel.a]))
        .count()
}

#[test]
fn history_and_location_actions_are_captured_without_navigating() {
    let output = render_fixture("js_provider_actions.html");
    assert_eq!(output.stats.scripts_executed, 1, "script runs");
    assert_eq!(
        output.history_actions,
        vec![
            HistoryAction::Push {
                state: "{\"page\":1}".to_owned(),
                url: Some("file:///one".to_owned()),
            },
            HistoryAction::Replace {
                state: "{\"page\":2}".to_owned(),
                url: Some("file:///two".to_owned()),
            },
        ]
        .into_iter()
        .collect::<Vec<_>>(),
        "history actions in order"
    );
    assert!(matches!(
        output.location_actions.first(),
        Some(LocationAction::Assign(_))
    ));
    assert!(
        output
            .location_actions
            .iter()
            .any(|action| matches!(action, LocationAction::Replace(_)))
    );
    assert_eq!(
        output.location_actions.last(),
        Some(&LocationAction::Reload)
    );
    assert_eq!(output.stats.location_actions, output.location_actions.len());
    assert_eq!(output.stats.history_actions, output.history_actions.len());
    // The page itself still renders normally after the navigation requests.
    let green = count_where(&output.pixmap, |[r, g, b, _]| r < 40 && g > 200 && b < 40);
    assert!(green > 300, "page content painted, got {green} px");
}

#[test]
fn local_storage_persists_between_renders_with_shared_storage() {
    let storage = Arc::new(Mutex::new(RenderStorage::default()));
    let write_options = RenderOptions {
        storage: Some(Arc::clone(&storage)),
        ..options()
    };
    let write = render_fixture_with("js_storage_write.html", &write_options);
    assert_eq!(write.stats.scripts_executed, 1);
    assert_eq!(
        storage.lock().unwrap().local.len(),
        1,
        "localStorage flushed to host"
    );

    let read_options = RenderOptions {
        storage: Some(Arc::clone(&storage)),
        ..options()
    };
    let read = render_fixture_with("js_storage_read.html", &read_options);
    assert_eq!(text_content(&read, "local"), "local:ok");
    // sessionStorage is a fresh per-render area even with shared localStorage.
    assert_eq!(text_content(&read, "session"), "session:isolated");
}

#[test]
fn storage_is_per_render_without_the_shared_option() {
    let write = render_fixture("js_storage_write.html");
    assert_eq!(write.stats.scripts_executed, 1);
    let read = render_fixture("js_storage_read.html");
    assert_eq!(text_content(&read, "local"), "local:missing");
    assert_eq!(text_content(&read, "session"), "session:isolated");
}

#[test]
fn host_hashchange_and_popstate_events_mutate_before_the_final_render() {
    let no_events = render_fixture("js_host_events.html");
    let red =
        |pixmap: &paint::Pixmap| count_where(pixmap, |[r, g, b, _]| r > 200 && g < 60 && b < 60);
    assert!(
        red(&no_events.pixmap) > 700,
        "both boxes red without host events"
    );

    let event_options = RenderOptions {
        host_events: vec!["hashchange".to_owned(), "popstate".to_owned()],
        ..options()
    };
    let output = render_fixture_with("js_host_events.html", &event_options);
    assert_eq!(
        output.stats.script_events, 4,
        "DOMContentLoaded + load + 2 host events"
    );
    let green = count_where(&output.pixmap, |[r, g, b, _]| r < 60 && g > 200 && b < 60);
    let blue = count_where(&output.pixmap, |[r, g, b, _]| r < 60 && g < 60 && b > 200);
    assert!(
        green > 300,
        "hashchange listener mutation painted, got {green} px"
    );
    assert!(
        blue > 300,
        "popstate listener mutation painted, got {blue} px"
    );
}

#[test]
fn mutation_observer_chain_reaches_the_dom_and_png() {
    let output = render_fixture("js_observer_chain.html");
    // Observer 1 changed #observed (blue -> green) after the childList mutation of #seed.
    let observed = element_with_id(&output.document, "observed");
    let style = output
        .document
        .node(observed)
        .as_element()
        .and_then(|element| element.get_attribute("style"))
        .unwrap_or_default()
        .to_owned();
    assert!(
        style.contains("0, 255, 0") || style.contains("#00ff00"),
        "observer 1 mutation missing: {style}"
    );
    // Observer 2 saw observer 1's attribute change and mutated #second (green -> magenta).
    let second = element_with_id(&output.document, "second");
    let style = output
        .document
        .node(second)
        .as_element()
        .and_then(|element| element.get_attribute("style"))
        .unwrap_or_default()
        .to_owned();
    assert!(
        style.contains("255, 0, 255") || style.contains("#ff00ff"),
        "observer chain mutation missing: {style}"
    );

    // The chain is painted: seed red (40x40), observed green (20x20), second magenta.
    let red = count_where(&output.pixmap, |[r, g, b, _]| r > 200 && g < 60 && b < 60);
    let green = count_where(&output.pixmap, |[r, g, b, _]| r < 60 && g > 200 && b < 60);
    let magenta = count_where(&output.pixmap, |[r, g, b, _]| r > 200 && g < 60 && b > 200);
    assert!(red > 1200, "seed box painted, got {red} px");
    assert!(green > 300, "observer 1 mutation painted, got {green} px");
    assert!(
        magenta > 300,
        "observer 2 chain mutation painted, got {magenta} px"
    );
}
