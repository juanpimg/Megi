//! Offline tests for `data:` URL parsing/decoding and the built-in Base64 codec.

use net::{
    FetchError, LoadError, ResourceLoader, Scheme, Url, base64_decode, base64_encode,
    decode_data_url,
};

#[test]
fn url_parser_accepts_data_scheme_and_round_trips() {
    let url = Url::parse("data:text/html,<h1>Hello</h1>").expect("data URL parses");
    assert_eq!(url.scheme(), Scheme::Data);
    assert_eq!(url.host(), "");
    assert_eq!(url.port(), None);
    assert!(url.path().contains("<h1>Hello</h1>"));
    assert_eq!(url.as_str(), "data:text/html,<h1>Hello</h1>");
    // Fragments are split off the payload and serialized back.
    let with_fragment = Url::parse("data:text/plain,abc#frag").expect("data URL with fragment");
    assert_eq!(with_fragment.fragment(), Some("frag"));
    assert_eq!(with_fragment.as_str(), "data:text/plain,abc#frag");
    // data: URLs are opaque: they cannot act as a relative base.
    assert!(url.join("/relative").is_err());
}

#[test]
fn loader_decodes_text_html_data_url() {
    let mut loader = ResourceLoader::new();
    let resource = loader
        .load_str("data:text/html,<h1>Hello</h1>")
        .expect("data: load succeeds");
    assert_eq!(resource.content_type, "text/html");
    assert_eq!(resource.text(), Some("<h1>Hello</h1>"));
    // data: URLs are not stored in the HTTP cache (immutable inline payloads).
    assert_eq!(loader.cache_len(), 0);
}

#[test]
fn loader_decodes_base64_css_data_url() {
    let payload = base64_encode(b"body { color: red }");
    let mut loader = ResourceLoader::new();
    let resource = loader
        .load_str(&format!("data:text/css;base64,{payload}"))
        .expect("base64 data URL loads");
    assert_eq!(resource.content_type, "text/css");
    assert_eq!(resource.text(), Some("body { color: red }"));
}

#[test]
fn loader_decodes_percent_encoded_payload_with_default_mediatype() {
    let mut loader = ResourceLoader::new();
    let resource = loader
        .load_str("data:,a%20b%3Cc%3E")
        .expect("percent-decoded data URL loads");
    assert_eq!(resource.content_type, "text/plain;charset=US-ASCII");
    assert_eq!(resource.bytes, b"a b<c>");
}

#[test]
fn loader_preserves_charset_parameters() {
    let mut loader = ResourceLoader::new();
    let resource = loader
        .load_str("data:text/plain;charset=utf-8,hola")
        .expect("charset parameter is preserved");
    assert_eq!(resource.content_type, "text/plain;charset=utf-8");
    assert_eq!(resource.text(), Some("hola"));
}

#[test]
fn invalid_base64_data_url_is_a_typed_error() {
    let mut loader = ResourceLoader::new();
    let error = loader
        .load_str("data:text/plain;base64,!!!!")
        .expect_err("invalid base64 must fail");
    match error {
        LoadError::Fetch(FetchError::InvalidDataUrl(reason)) => {
            assert!(!reason.is_empty(), "error needs a reason")
        }
        other => panic!("expected InvalidDataUrl, got {other:?}"),
    }
}

#[test]
fn data_url_without_comma_is_a_typed_error() {
    let mut loader = ResourceLoader::new();
    let error = loader
        .load_str("data:text/plain")
        .expect_err("missing comma must fail");
    assert!(
        matches!(error, LoadError::Fetch(FetchError::InvalidDataUrl(_))),
        "unexpected error: {error}"
    );
}

#[test]
fn decode_data_url_exposes_mime_and_bytes() {
    let url = Url::parse("data:image/png;base64,iVBORw0KGgo=").expect("data URL");
    let decoded = decode_data_url(&url).expect("decode");
    assert_eq!(decoded.mime_type, "image/png");
    assert_eq!(decoded.bytes, b"\x89PNG\r\n\x1a\n");
}

#[test]
fn base64_round_trip_for_binary_payloads() {
    let bytes: Vec<u8> = (0..=255u8).collect();
    let encoded = base64_encode(&bytes);
    let decoded = base64_decode(&encoded).expect("round trip");
    assert_eq!(decoded, bytes);
}
