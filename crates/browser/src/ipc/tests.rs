//! IPC protocol tests (DOM patches, process frames).

use super::*;

const TEST_TOKEN: [u8; 32] = [0x11; 32];

fn write_request_frame(buffer: &mut Vec<u8>, request: &IpcRenderRequest) {
    write_framed_envelope(
        buffer,
        IPC_KIND_REQUEST,
        request.generation,
        TEST_TOKEN,
        request,
    )
    .expect("write request envelope");
}

fn read_response_frame(cursor: &mut &[u8]) -> IpcRenderResponse {
    let (header, response) = read_framed_envelope(cursor)
        .expect("read response envelope")
        .expect("response frame");
    assert_eq!(header.kind, IPC_KIND_RESPONSE);
    assert_eq!(header.site_token, TEST_TOKEN);
    response
}

fn minimal_png() -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];
    bytes.extend_from_slice(&13u32.to_be_bytes());
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&1u32.to_be_bytes());
    bytes.extend_from_slice(&1u32.to_be_bytes());
    bytes.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
    bytes
}

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
    let element = document.create_element(
        megi_webview::core::Namespace::Html,
        "p",
        vec![megi_webview::core::Attribute::new("id", "target")],
    );
    document
        .set_text_content(element, "round trip")
        .expect("text");
    assert!(document.append_child(document.root(), element));
    let html = crate::download::serialize_document(&document);
    let restored = DomPatch::restore_document(&html);
    let paragraph = restored.get_element_by_id("target").expect("restored <p>");
    let text = restored.text_content(paragraph);
    assert!(
        text.contains("round trip"),
        "restore lost the text: {text:?}"
    );
}

#[test]
fn process_frame_restores_dom_from_wire_response() {
    let response = IpcRenderResponse {
        generation: 3,
        png_bytes: minimal_png(),
        content_height: 100,
        css_width: 100,
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
        sandbox_mode: String::new(),
    };
    let frame = ProcessFrame::from_response(validate_response(response, 2).expect("valid frame"));
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
        css_width: 0,
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
        sandbox_mode: String::new(),
    };
    let frame = ProcessFrame::from_response(validate_response(response, 0).expect("valid frame"));
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

#[test]
fn process_requests_route_to_their_effective_site() {
    let request = IpcRenderRequest {
        generation: 1,
        input: "data:text/html,<p>patched</p>".to_owned(),
        css_width: 100,
        css_height: 100,
        base_override: Some("https://real.example/page".to_owned()),
        scripting: true,
        find_query: None,
        options: IpcRenderOptions::default(),
    };
    assert_eq!(
        crate::ipc::request_process_site(&request),
        crate::site::site_key("https://real.example/page")
    );
    let direct = IpcRenderRequest {
        base_override: None,
        input: "http://other.test/".to_owned(),
        ..request
    };
    assert_eq!(
        crate::ipc::request_process_site(&direct),
        crate::site::site_key("http://other.test/")
    );
}

#[test]
fn fatal_ipc_errors_trigger_process_respawn() {
    // A crashed renderer produces Truncated or Io; a malformed payload does not kill it.
    assert!(IpcError::Truncated.is_fatal());
    assert!(IpcError::Io("broken pipe".to_owned()).is_fatal());
    assert!(!IpcError::InvalidPayload("bad json".to_owned()).is_fatal());
    assert!(!IpcError::TooLarge(1).is_fatal());
    assert!(
        IpcError::Validation("bad site token".to_owned()).is_fatal(),
        "#2308: a forged response must retire the slot"
    );
}

#[test]
fn worker_stream_serves_requests_from_distinct_sites() {
    // E2E of the child-side protocol: two navigations to distinct origins produce two framed
    // responses with their own document URLs, without any real process/display.
    let mut input = Vec::new();
    for (generation, url) in [
        (1u64, "data:text/html,<title>First</title><p>one</p>"),
        (2u64, "data:text/html,<title>Second</title><p>two</p>"),
    ] {
        let request = IpcRenderRequest {
            generation,
            input: url.to_owned(),
            css_width: 320,
            css_height: 200,
            base_override: None,
            scripting: false,
            find_query: None,
            options: IpcRenderOptions::default(),
        };
        write_request_frame(&mut input, &request);
    }
    let mut output = Vec::new();
    crate::ipc::run_ipc_worker_stream(&mut &input[..], &mut output).expect("worker stream");
    let mut cursor = &output[..];
    for expected_generation in [1u64, 2] {
        let response = read_response_frame(&mut cursor);
        assert_eq!(response.generation, expected_generation);
        assert_eq!(response.css_width, 320);
        assert!(
            response.error.is_none(),
            "render failed: {:?}",
            response.error
        );
        assert!(
            !response.png_bytes.is_empty(),
            "a PNG must cross the boundary"
        );
    }
}

#[test]
fn worker_stream_detects_a_truncated_request_from_a_dead_peer() {
    // E2E crash signal: a peer that dies mid-frame yields Truncated, which the pool classifies
    // as fatal and uses to respawn the renderer process.
    let mut frame = Vec::new();
    write_request_frame(
        &mut frame,
        &IpcRenderRequest {
            generation: 1,
            input: "data:text/html,<p>cut</p>".to_owned(),
            css_width: 100,
            css_height: 100,
            base_override: None,
            scripting: false,
            find_query: None,
            options: IpcRenderOptions::default(),
        },
    );
    frame.truncate(super::IPC_ENVELOPE_HEADER_LEN + 2);
    let mut truncated = &frame[..];
    let mut output = Vec::new();
    assert_eq!(
        crate::ipc::run_ipc_worker_stream(&mut truncated, &mut output),
        Err(IpcError::Truncated)
    );
    assert!(output.is_empty(), "no response for an incomplete request");
}

#[test]
fn indexed_region_translation_maps_large_dom_linearly_with_parity() {
    let mut document = megi_webview::core::Document::new();
    let mut link_nodes = Vec::with_capacity(5_000);
    for idx in 0..5_000 {
        let el = document.create_element(
            megi_webview::core::Namespace::Html,
            "a",
            vec![megi_webview::core::Attribute::new(
                "href",
                format!("/{idx}"),
            )],
        );
        let text = document.create_text_node(&format!("L{idx}"));
        assert!(document.append_child(el, text));
        assert!(document.append_child(document.root(), el));
        link_nodes.push((el, text));
    }

    let ordinals = DocumentNodeOrdinals::build(&document);
    let nodes = DocumentOrdinalNodes::build(&document);
    for (idx, (el, text)) in link_nodes.iter().enumerate() {
        let link = crate::links::LinkRegion {
            rect: crate::links::CssRect::new(idx as f32, 0.0, 10.0, 10.0),
            href: format!("/{idx}"),
            node: Some(*el),
            is_blocker: false,
        };
        let ipc_link = IpcLinkRegion::from_region_indexed(&link, &ordinals);
        let restored_link = ipc_link.into_link_region_indexed(&nodes);
        assert_eq!(restored_link.node, Some(*el));

        let image = crate::images::ImageRegion {
            node_id: *el,
            rect: crate::links::CssRect::new(0.0, 0.0, 1.0, 1.0),
            url: format!("/{idx}.png"),
        };
        assert_eq!(
            IpcImageRegion::from_region_indexed(&image, &ordinals)
                .into_image_region_indexed(&nodes)
                .node_id,
            *el
        );

        let find = crate::find::FindMatch {
            node_id: *text,
            range: 0..1,
        };
        assert_eq!(
            IpcFindMatch::from_match_indexed(&find, &ordinals)
                .into_match_indexed(&nodes)
                .node_id,
            *text
        );

        let control = crate::forms::FormControl {
            form_id: 1,
            name: format!("f{idx}"),
            node_id: *el,
            kind: crate::forms::FormControlKind::TextInput,
            rect: crate::links::CssRect::new(0.0, 0.0, 20.0, 10.0),
            value: String::new(),
            checked: false,
            disabled: false,
            options: Vec::new(),
            multiple: false,
        };
        let restored_control = IpcFormControl::from_control_indexed(&control, &ordinals)
            .into_control_indexed(&nodes)
            .expect("valid kind");
        assert_eq!(restored_control.node_id, *el);
    }

    // Spot-check parity with single-shot helpers on deep tail nodes + invalid ordinal sentinel.
    let tail = link_nodes[4_999].0;
    let single = IpcLinkRegion::from_region(
        &crate::links::LinkRegion {
            rect: crate::links::CssRect::new(0.0, 0.0, 1.0, 1.0),
            href: "/4999".to_owned(),
            node: Some(tail),
            is_blocker: false,
        },
        &document,
    );
    assert_eq!(single.node_ordinal, ordinals.ordinal_of(tail));
    let sentinel = IpcImageRegion {
        node_ordinal: Some(usize::MAX),
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
        url: String::new(),
    };
    assert_eq!(
        sentinel.into_image_region_indexed(&nodes).node_id,
        megi_webview::core::NodeId::from_index(usize::MAX)
    );
}

#[test]
fn worker_stream_and_process_frame_preserve_many_link_and_form_nodes() {
    let mut html = String::from("<html><body>");
    for idx in 0..200 {
        html.push_str(&format!(
            "<a href=\"/{idx}\">L{idx}</a><input name=\"k{idx}\" value=\"v{idx}\">"
        ));
    }
    html.push_str("</body></html>");

    let request = IpcRenderRequest {
        generation: 1,
        input: format!("data:text/html,{html}"),
        css_width: 400,
        css_height: 300,
        base_override: None,
        scripting: false,
        find_query: None,
        options: IpcRenderOptions::default(),
    };
    let mut input = Vec::new();
    write_request_frame(&mut input, &request);
    let mut output = Vec::new();
    crate::ipc::run_ipc_worker_stream(&mut &input[..], &mut output).expect("worker stream");

    let response = read_response_frame(&mut &output[..]);
    let validated = validate_response(response, 0).expect("validated response");
    let process_frame = crate::ipc::ProcessFrame::from_response(validated);
    let render_frame = process_frame.to_render_frame(400).expect("render frame");
    assert!(
        render_frame.links.len() >= 200,
        "expected >= 200 link regions, got {}",
        render_frame.links.len()
    );
    assert_eq!(render_frame.forms.len(), 200);
    assert!(
        render_frame.links.iter().all(|link| link.node.is_some()),
        "every restored link region must resolve to a node"
    );
    assert!(
        render_frame
            .forms
            .iter()
            .all(|control| control.node_id != megi_webview::core::NodeId::from_index(usize::MAX)),
        "every restored form control must resolve to a node"
    );
}
