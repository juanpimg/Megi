//! Parent/child envelope-binding tests for the live IPC path (#2308).
//!
//! The validated envelope existed as dead code; these tests prove that a renderer reply is
//! accepted only when it answers the exact request of the exact slot, in generation order.

use super::process::decode_validated_response;
use super::{
    IPC_KIND_REQUEST, IPC_KIND_RESPONSE, IPC_VERSION, IpcError, IpcRenderRequest,
    IpcRenderResponse, WireHeader, read_framed_envelope, run_ipc_worker_stream,
    write_framed_envelope,
};

const TOKEN_A: [u8; 32] = [0xA5; 32];
const TOKEN_B: [u8; 32] = [0x5A; 32];

fn request(generation: u64) -> IpcRenderRequest {
    IpcRenderRequest {
        generation,
        input: "data:text/html,<p>envelope</p>".to_owned(),
        css_width: 160,
        css_height: 120,
        base_override: None,
        scripting: false,
        find_query: None,
        options: Default::default(),
    }
}

fn wire_response(generation: u64) -> IpcRenderResponse {
    let mut png = vec![0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];
    png.extend_from_slice(&13u32.to_be_bytes());
    png.extend_from_slice(b"IHDR");
    png.extend_from_slice(&1u32.to_be_bytes());
    png.extend_from_slice(&1u32.to_be_bytes());
    png.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
    IpcRenderResponse {
        generation,
        png_bytes: png,
        content_height: 1,
        css_width: 1,
        title: None,
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

fn header(kind: u16, request_id: u64, site_token: [u8; 32]) -> WireHeader {
    WireHeader {
        version: IPC_VERSION,
        kind,
        request_id,
        site_token,
        len: 0,
    }
}

#[test]
fn worker_stream_answers_with_request_id_and_site_token_from_the_request() {
    // E2E (in-memory child): the response envelope must carry the caller's binding fields.
    let mut input = Vec::new();
    // The parent uses the render generation as the request id, so the reply's id must
    // round-trip unchanged and stay correlated with the frame's generation.
    write_framed_envelope(&mut input, IPC_KIND_REQUEST, 1, TOKEN_A, &request(1))
        .expect("write request envelope");
    let mut output = Vec::new();
    run_ipc_worker_stream(&mut &input[..], &mut output).expect("worker stream");

    let (response_header, response): (WireHeader, IpcRenderResponse) =
        read_framed_envelope(&mut &output[..])
            .expect("read response envelope")
            .expect("response frame");
    assert_eq!(response_header.kind, IPC_KIND_RESPONSE);
    assert_eq!(response_header.request_id, 1);
    assert_eq!(response_header.site_token, TOKEN_A);
    assert_eq!(response.generation, 1, "render error: {:?}", response.error);
    assert!(!response.png_bytes.is_empty());
    assert!(
        decode_validated_response(&response_header, response, &TOKEN_A, 0).is_ok(),
        "the echoed envelope must be accepted by the parent"
    );
}

#[test]
fn worker_stream_rejects_a_response_kind_on_the_request_stream() {
    let mut input = Vec::new();
    write_framed_envelope(&mut input, IPC_KIND_RESPONSE, 1, TOKEN_A, &request(1))
        .expect("write envelope");
    let mut output = Vec::new();
    let error = run_ipc_worker_stream(&mut &input[..], &mut output).expect_err("wrong kind");
    assert!(
        matches!(&error, IpcError::InvalidPayload(reason) if reason.contains("kind")),
        "unexpected error: {error:?}"
    );
    assert!(output.is_empty());
}

#[test]
fn response_with_wrong_kind_is_rejected() {
    let bad = header(IPC_KIND_REQUEST, 1, TOKEN_A);
    let error = decode_validated_response(&bad, wire_response(1), &TOKEN_A, 0)
        .expect_err("request kind must not be accepted as a response");
    assert!(
        matches!(error, IpcError::Validation(_)),
        "unexpected error: {error:?}"
    );
}

#[test]
fn response_with_wrong_request_id_is_rejected() {
    let bad = header(IPC_KIND_RESPONSE, 99, TOKEN_A);
    let error = decode_validated_response(&bad, wire_response(1), &TOKEN_A, 0)
        .expect_err("reply to a different request must be rejected");
    assert!(
        matches!(error, IpcError::Validation(_)),
        "unexpected error: {error:?}"
    );
}

#[test]
fn response_with_wrong_site_token_is_rejected() {
    // A renderer from another site must not be able to answer this slot.
    let bad = header(IPC_KIND_RESPONSE, 1, TOKEN_B);
    let error = decode_validated_response(&bad, wire_response(1), &TOKEN_A, 0)
        .expect_err("foreign site token must be rejected");
    assert!(
        matches!(error, IpcError::Validation(_)),
        "unexpected error: {error:?}"
    );
}

#[test]
fn response_with_out_of_sequence_generation_is_rejected() {
    let good = header(IPC_KIND_RESPONSE, 5, TOKEN_A);
    let error = decode_validated_response(&good, wire_response(5), &TOKEN_A, 1)
        .expect_err("a skipped generation must be rejected");
    assert!(
        matches!(error, IpcError::Validation(_)),
        "unexpected error: {error:?}"
    );
}

#[test]
fn matching_response_is_accepted() {
    let good = header(IPC_KIND_RESPONSE, 1, TOKEN_A);
    let validated =
        decode_validated_response(&good, wire_response(1), &TOKEN_A, 0).expect("matching envelope");
    assert_eq!(validated.inner().generation, 1);
}

#[test]
fn distinct_sites_derive_distinct_tokens() {
    let site_a = crate::site::site_key("https://site-a.test/");
    let site_b = crate::site::site_key("https://site-b.test/");
    let token_a = crate::site::derive_renderer_token(&site_a, 1);
    let token_b = crate::site::derive_renderer_token(&site_b, 1);
    assert_ne!(token_a, token_b);
    assert_ne!(
        token_a,
        crate::site::derive_renderer_token(&site_a, 2),
        "a respawned renderer must get a fresh token"
    );
    assert_eq!(token_a, crate::site::derive_renderer_token(&site_a, 1));
}
