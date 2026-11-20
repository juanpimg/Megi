//! IPC protocol tests (DOM patches, process frames).

use super::*;

#[test]
fn dom_patch_carries_a_data_url_and_base() {
    let mut document = megi_webview::core::Document::new();
    let element = document.create_element(megi_webview::core::Namespace::Html, "p", vec![]);
    document
        .set_text_content(element, "patch me")
        .expect("text");
    assert!(document.append_child(document.root(), element));
    let patch = DomPatch::from_document(&document, "https://example.test/page");
    assert!(
        patch.input.starts_with("data:text/html"),
        "patch must carry the DOM inline: {}",
        patch.input
    );
    assert_eq!(
        patch.base_override.as_deref(),
        Some("https://example.test/page")
    );
}

#[test]
fn restore_document_recovers_the_serialized_dom() {
    let mut document = megi_webview::core::Document::new();
    let element = document.create_element(megi_webview::core::Namespace::Html, "p", vec![]);
    document
        .set_text_content(element, "round trip")
        .expect("text");
    assert!(document.append_child(document.root(), element));
    let html = crate::download::serialize_document(&document);
    let restored = DomPatch::restore_document(&html);
    let text = restored.text_content(restored.root());
    assert!(
        text.contains("round trip"),
        "restore lost the text: {text:?}"
    );
}

#[test]
fn process_frame_restores_dom_from_wire_response() {
    let response = IpcRenderResponse {
        generation: 3,
        png_bytes: vec![0x89, b'P', b'N', b'G'],
        content_height: 100,
        title: Some("probe".to_owned()),
        document_url: "https://example.test/".to_owned(),
        scripts_executed: 1,
        script_timers_pending: 0,
        warnings: 0,
        script_errors: 0,
        links: Vec::new(),
        images: Vec::new(),
        forms: Vec::new(),
        find_matches: Vec::new(),
        find_rects: Vec::new(),
        document_html: "<!doctype html><html><body><p id=from-child>patched</p></body></html>"
            .to_owned(),
        error: None,
    };
    let frame = ProcessFrame::from_response(response);
    assert_eq!(frame.response.generation, 3);
    let node = frame
        .document
        .get_element_by_id("from-child")
        .expect("restored node");
    assert_eq!(frame.document.text_content(node), "patched");
}

#[test]
fn failed_response_uses_an_empty_document() {
    let response = IpcRenderResponse {
        generation: 1,
        png_bytes: Vec::new(),
        content_height: 0,
        title: None,
        document_url: String::new(),
        scripts_executed: 0,
        script_timers_pending: 0,
        warnings: 0,
        script_errors: 0,
        links: Vec::new(),
        images: Vec::new(),
        forms: Vec::new(),
        find_matches: Vec::new(),
        find_rects: Vec::new(),
        document_html: String::new(),
        error: Some("boom".to_owned()),
    };
    let frame = ProcessFrame::from_response(response);
    assert!(frame.document.get_element_by_id("anything").is_none());
    assert_eq!(frame.response.error.as_deref(), Some("boom"));
}

#[test]
fn link_region_survives_the_process_boundary() {
    let wire = IpcLinkRegion {
        x: 10.0,
        y: 20.0,
        width: 30.0,
        height: 12.0,
        href: "/dest".to_owned(),
        node_ordinal: Some(3),
        is_blocker: false,
    };
    let json = serde_json::to_string(&wire).expect("serialize link region");
    let restored: IpcLinkRegion = serde_json::from_str(&json).expect("deserialize link region");
    assert_eq!(restored, wire);
    let region = restored.into_link_region();
    assert_eq!(
        region.rect,
        crate::links::CssRect::new(10.0, 20.0, 30.0, 12.0)
    );
    assert_eq!(region.href, "/dest");
    assert!(
        region.node.is_none(),
        "ordinal maps to a parent document node later"
    );
    assert!(!region.is_blocker);
}

#[test]
fn image_and_find_regions_round_trip_through_json() {
    let image = IpcImageRegion {
        node_ordinal: Some(5),
        x: 1.0,
        y: 2.0,
        width: 3.0,
        height: 4.0,
        url: "https://example.test/i.png".to_owned(),
    };
    let rect = IpcRect {
        x: 10.0,
        y: 20.0,
        width: 30.0,
        height: 40.0,
    };
    let find = IpcFindMatch {
        node_ordinal: Some(2),
        start: 3,
        end: 7,
    };
    let json = serde_json::to_string(&(image.clone(), rect, find.clone())).expect("serialize");
    let (back_image, back_rect, back_find): (IpcImageRegion, IpcRect, IpcFindMatch) =
        serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back_image, image);
    assert_eq!(back_rect, rect);
    assert_eq!(back_find, find);
    assert_eq!(
        back_rect.into_rect(),
        crate::links::CssRect::new(10.0, 20.0, 30.0, 40.0)
    );
}

#[test]
fn form_control_round_trip_preserves_kind_and_options() {
    let control = IpcFormControl {
        form_id: 7,
        name: "q".to_owned(),
        node_ordinal: Some(11),
        kind: "select".to_owned(),
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 20.0,
        value: "b".to_owned(),
        checked: false,
        disabled: false,
        options: vec![IpcSelectOption {
            label: "B".to_owned(),
            value: "b".to_owned(),
            selected: true,
            group: Some("letters".to_owned()),
        }],
        multiple: false,
    };
    let json = serde_json::to_string(&control).expect("serialize");
    let back: IpcFormControl = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, control);
    let document = megi_webview::core::Document::new();
    let restored = back.into_control(&document).expect("known kind");
    assert_eq!(restored.form_id, 7);
    assert_eq!(restored.kind, crate::forms::FormControlKind::Select);
    assert_eq!(restored.options[0].group.as_deref(), Some("letters"));
}
