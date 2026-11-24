//! Negative and round-trip tests for the F1 IPC validation boundary.
//!
//! Plan: docs/superpowers/plans/2026-11-24-security-isolation-plan.md §Task 4 (F1).

use megi_browser::ipc::{
    IPC_MAGIC, IPC_VERSION, IpcError, IpcRenderRequest, IpcRenderResponse, MAX_IPC_PAYLOAD,
    ValidationError, WireHeader, read_framed_envelope, validate_response, write_framed_envelope,
};

fn valid_png_header(width: u32, height: u32) -> Vec<u8> {
    let mut out = vec![0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];
    out.extend_from_slice(&13u32.to_be_bytes());
    out.extend_from_slice(b"IHDR");
    out.extend_from_slice(&width.to_be_bytes());
    out.extend_from_slice(&height.to_be_bytes());
    out.extend_from_slice(&[8, 6, 0, 0, 0]);
    out.extend_from_slice(&[0u8; 4]);
    out
}

fn valid_response(generation: u64) -> IpcRenderResponse {
    IpcRenderResponse {
        generation,
        png_bytes: valid_png_header(320, 200),
        content_height: 200,
        css_width: 320,
        title: Some("ok".to_owned()),
        document_url: "https://example.test/".to_owned(),
        scripts_executed: 0,
        script_timers_pending: 0,
        warnings: 0,
        script_errors: 0,
        links: Vec::new(),
        images: Vec::new(),
        forms: Vec::new(),
        find_matches: Vec::new(),
        find_rects: Vec::new(),
        document_html: "<!doctype html><html><body></body></html>".to_owned(),
        error: None,
        sandbox_mode: String::new(),
    }
}

#[test]
fn oversized_css_dimension_rejected() {
    let mut response = valid_response(1);
    response.css_width = 9000;
    assert_eq!(
        validate_response(response, 0),
        Err(ValidationError::OversizedDimension)
    );
}

#[test]
fn stale_generation_rejected() {
    assert_eq!(
        validate_response(valid_response(1), 1),
        Err(ValidationError::StaleGeneration)
    );
    assert_eq!(
        validate_response(valid_response(5), 1),
        Err(ValidationError::StaleGeneration)
    );
}

#[test]
fn fake_png_bytes_rejected() {
    let mut response = valid_response(1);
    response.png_bytes = b"not a png".to_vec();
    assert_eq!(
        validate_response(response, 0),
        Err(ValidationError::InvalidPng)
    );
}

#[test]
fn document_url_wrong_scheme_rejected() {
    let mut response = valid_response(1);
    response.document_url = "javascript:alert(1)".to_owned();
    assert_eq!(
        validate_response(response, 0),
        Err(ValidationError::DisallowedScheme)
    );
}

#[test]
fn unknown_field_rejected() {
    let req = IpcRenderRequest {
        generation: 1,
        input: "https://example.test/".to_owned(),
        css_width: 320,
        css_height: 200,
        base_override: None,
        scripting: false,
        find_query: None,
        options: Default::default(),
    };
    let mut req_json = serde_json::to_value(&req).expect("serialize req");
    req_json
        .as_object_mut()
        .expect("object")
        .insert("evil_extra".to_owned(), serde_json::json!(true));
    assert!(
        serde_json::from_value::<IpcRenderRequest>(req_json).is_err(),
        "unknown field on IpcRenderRequest must be rejected"
    );

    let resp = valid_response(1);
    let mut resp_json = serde_json::to_value(&resp).expect("serialize resp");
    resp_json
        .as_object_mut()
        .expect("object")
        .insert("cookies".to_owned(), serde_json::json!([]));
    assert!(
        serde_json::from_value::<IpcRenderResponse>(resp_json).is_err(),
        "unknown field on IpcRenderResponse must be rejected"
    );
}

#[test]
fn wire_envelope_roundtrip_and_bad_magic_rejected() {
    let token = [0xABu8; 32];
    let req = IpcRenderRequest {
        generation: 2,
        input: "https://example.test/app".to_owned(),
        css_width: 640,
        css_height: 480,
        base_override: None,
        scripting: true,
        find_query: None,
        options: Default::default(),
    };
    let mut wire = Vec::new();
    write_framed_envelope(&mut wire, 1, 99, token, &req).expect("write envelope");

    let (header, decoded): (WireHeader, IpcRenderRequest) = read_framed_envelope(&mut &wire[..])
        .expect("read envelope")
        .expect("some frame");
    assert_eq!(header.version, IPC_VERSION);
    assert_eq!(header.kind, 1);
    assert_eq!(header.request_id, 99);
    assert_eq!(header.site_token, token);
    assert_eq!(decoded, req);

    let mut bad_magic = wire.clone();
    bad_magic[..4].copy_from_slice(b"NOPE");
    assert!(matches!(
        read_framed_envelope::<_, IpcRenderRequest>(&mut &bad_magic[..]),
        Err(IpcError::InvalidPayload(_))
    ));

    let mut bad_version = wire.clone();
    bad_version[4..6].copy_from_slice(&(IPC_VERSION + 1).to_be_bytes());
    assert!(matches!(
        read_framed_envelope::<_, IpcRenderRequest>(&mut &bad_version[..]),
        Err(IpcError::InvalidPayload(_))
    ));

    let mut too_large = wire;
    too_large[..4].copy_from_slice(&IPC_MAGIC);
    too_large[48..52].copy_from_slice(&((MAX_IPC_PAYLOAD as u32) + 1).to_be_bytes());
    assert_eq!(
        read_framed_envelope::<_, IpcRenderRequest>(&mut &too_large[..]),
        Err(IpcError::TooLarge(MAX_IPC_PAYLOAD + 1))
    );
}
