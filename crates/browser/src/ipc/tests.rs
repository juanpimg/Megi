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
        document_html: String::new(),
        error: Some("boom".to_owned()),
    };
    let frame = ProcessFrame::from_response(response);
    assert!(frame.document.get_element_by_id("anything").is_none());
    assert_eq!(frame.response.error.as_deref(), Some("boom"));
}
