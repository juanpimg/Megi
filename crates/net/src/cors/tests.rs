use super::*;

fn url(raw: &str) -> Url {
    Url::parse(raw).expect("test URL parses")
}

fn headers(pairs: &[(&str, &str)]) -> Headers {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

#[test]
fn origin_parses_and_serializes() {
    let origin = Origin::parse("https://example.com").expect("origin");
    assert_eq!(origin.as_str(), "https://example.com");
    assert_eq!(origin.port, 443);
    let with_port = Origin::parse("http://example.com:8080").expect("origin");
    assert_eq!(with_port.as_str(), "http://example.com:8080");
    assert_eq!(Origin::parse("https://example.com:443").unwrap(), origin);
    assert_eq!(Origin::parse("null"), None);
    assert_eq!(Origin::parse(""), None);
    assert_eq!(Origin::parse("ftp://example.com"), None);
    assert_eq!(Origin::parse("example.com"), None);
}

#[test]
fn origin_from_url() {
    let origin = Origin::from_url(&url("https://Example.COM/path?q")).unwrap();
    assert_eq!(origin.host, "example.com");
    assert_eq!(origin.scheme, "https");
    assert_eq!(
        Origin::from_url(&url("http://127.0.0.1:8080")),
        Origin::parse("http://127.0.0.1:8080")
    );
    assert_eq!(Origin::from_url(&url("file:///tmp/x")), None);
}

#[test]
fn same_origin_comparison() {
    let a = Origin::parse("https://example.com").unwrap();
    let b = Origin::parse("https://example.com:443").unwrap();
    let c = Origin::parse("http://example.com").unwrap();
    assert!(a.is_same_origin(&b));
    assert!(!a.is_same_origin(&c));
}

#[test]
fn simple_methods_and_headers() {
    assert!(is_simple_method("GET"));
    assert!(is_simple_method("post"));
    assert!(!is_simple_method("PUT"));
    assert!(!is_simple_method("OPTIONS"));
    assert!(is_simple_header("Accept"));
    assert!(is_simple_header("content-type"));
    assert!(!is_simple_header("x-custom"));
}

#[test]
fn preflight_classification() {
    assert!(!requires_preflight("GET", &[]));
    assert!(!requires_preflight(
        "POST",
        &[("Content-Type", "text/plain")]
    ));
    assert!(requires_preflight("PUT", &[]));
    assert!(requires_preflight("GET", &[("X-Token", "1")]));
}

#[test]
fn same_origin_response_needs_no_headers() {
    assert!(
        validate_response(
            &url("https://example.com/a"),
            "https://example.com",
            &headers(&[]),
            false
        )
        .is_ok()
    );
}

#[test]
fn cross_origin_response_with_exact_acao_passes() {
    let response = headers(&[("Access-Control-Allow-Origin", "https://app.example")]);
    assert!(
        validate_response(
            &url("https://api.example/data"),
            "https://app.example",
            &response,
            false
        )
        .is_ok()
    );
}

#[test]
fn cross_origin_response_with_wildcard_passes_without_credentials() {
    let response = headers(&[("access-control-allow-origin", "*")]);
    assert!(
        validate_response(
            &url("https://api.example/data"),
            "https://app.example",
            &response,
            false
        )
        .is_ok()
    );
}

#[test]
fn acao_is_byte_exact_not_case_or_port_normalized() {
    // Fetch §4.10 compares the ACAO value byte-for-byte with the serialized request
    // origin. `Origin::parse` lowercases and normalizes default ports, so reusing it
    // here wrongly accepts `https://App.Example` and `https://app.example:443` for a
    // request from `https://app.example` (#1405).
    let cases = [
        "https://App.example",     // host case differs
        "https://app.example:443", // explicit default port differs
        "HTTPS://app.example",     // scheme case differs
    ];
    for acao in cases {
        let response = headers(&[("Access-Control-Allow-Origin", acao)]);
        let error = validate_response(
            &url("https://api.example/data"),
            "https://app.example",
            &response,
            false,
        )
        .unwrap_err();
        assert!(
            matches!(&error, FetchError::Cors(reason) if reason.contains("not allowed")),
            "ACAO `{acao}` must be rejected byte-exactly, got {error}"
        );
    }
}

#[test]
fn missing_acao_is_rejected() {
    let error = validate_response(
        &url("https://api.example/data"),
        "https://app.example",
        &headers(&[]),
        false,
    )
    .unwrap_err();
    assert!(
        matches!(&error, FetchError::Cors(reason) if reason.contains("missing")),
        "{error}"
    );
}

#[test]
fn wrong_acao_is_rejected() {
    let response = headers(&[("Access-Control-Allow-Origin", "https://other.example")]);
    let error = validate_response(
        &url("https://api.example/data"),
        "https://app.example",
        &response,
        false,
    )
    .unwrap_err();
    assert!(
        matches!(&error, FetchError::Cors(reason) if reason.contains("not allowed")),
        "{error}"
    );
}

#[test]
fn wildcard_with_credentials_is_rejected() {
    let response = headers(&[
        ("Access-Control-Allow-Origin", "*"),
        ("Access-Control-Allow-Credentials", "true"),
    ]);
    let error = validate_response(
        &url("https://api.example/data"),
        "https://app.example",
        &response,
        true,
    )
    .unwrap_err();
    assert!(
        matches!(&error, FetchError::Cors(reason) if reason.contains("credentials")),
        "{error}"
    );
}

#[test]
fn credentials_require_allow_credentials_true() {
    let response = headers(&[("Access-Control-Allow-Origin", "https://app.example")]);
    assert!(
        validate_response(
            &url("https://api.example/data"),
            "https://app.example",
            &response,
            true
        )
        .is_err()
    );
    let response = headers(&[
        ("Access-Control-Allow-Origin", "https://app.example"),
        ("Access-Control-Allow-Credentials", "TRUE"),
    ]);
    assert!(
        validate_response(
            &url("https://api.example/data"),
            "https://app.example",
            &response,
            true
        )
        .is_ok()
    );
}

#[test]
fn invalid_origin_value_is_rejected() {
    let error = validate_response(
        &url("https://api.example/data"),
        "not-an-origin",
        &headers(&[("Access-Control-Allow-Origin", "*")]),
        false,
    )
    .unwrap_err();
    assert!(
        matches!(&error, FetchError::Cors(reason) if reason.contains("invalid Origin")),
        "{error}"
    );
}

#[test]
fn preflight_same_origin_is_a_noop() {
    assert!(
        validate_preflight_response(
            &url("https://example.com/a"),
            "https://example.com",
            "PUT",
            &[("X-Token", "1")],
            &headers(&[]),
            false,
        )
        .is_ok()
    );
}

#[test]
fn preflight_validates_method_and_headers() {
    let response = headers(&[
        ("Access-Control-Allow-Origin", "https://app.example"),
        ("Access-Control-Allow-Methods", "GET, PUT"),
        ("Access-Control-Allow-Headers", "content-type, x-token"),
    ]);
    assert!(
        validate_preflight_response(
            &url("https://api.example/data"),
            "https://app.example",
            "PUT",
            &[("X-Token", "1")],
            &response,
            false,
        )
        .is_ok()
    );
}

#[test]
fn preflight_rejects_unlisted_method() {
    let response = headers(&[
        ("Access-Control-Allow-Origin", "https://app.example"),
        ("Access-Control-Allow-Methods", "GET"),
    ]);
    let error = validate_preflight_response(
        &url("https://api.example/data"),
        "https://app.example",
        "DELETE",
        &[],
        &response,
        false,
    )
    .unwrap_err();
    assert!(
        matches!(&error, FetchError::Cors(reason) if reason.contains("DELETE")),
        "{error}"
    );
}

#[test]
fn preflight_rejects_missing_method_header() {
    let response = headers(&[("Access-Control-Allow-Origin", "https://app.example")]);
    let error = validate_preflight_response(
        &url("https://api.example/data"),
        "https://app.example",
        "PUT",
        &[],
        &response,
        false,
    )
    .unwrap_err();
    assert!(
        matches!(&error, FetchError::Cors(reason) if reason.contains("Allow-Methods")),
        "{error}"
    );
}

#[test]
fn preflight_rejects_unlisted_header() {
    let response = headers(&[
        ("Access-Control-Allow-Origin", "https://app.example"),
        ("Access-Control-Allow-Methods", "PUT"),
        ("Access-Control-Allow-Headers", "content-type"),
    ]);
    let error = validate_preflight_response(
        &url("https://api.example/data"),
        "https://app.example",
        "PUT",
        &[("X-Token", "1")],
        &response,
        false,
    )
    .unwrap_err();
    assert!(
        matches!(&error, FetchError::Cors(reason) if reason.contains("x-token") || reason.contains("X-Token")),
        "{error}"
    );
}

#[test]
fn preflight_requires_headers_list_only_for_non_simple_headers() {
    let response = headers(&[
        ("Access-Control-Allow-Origin", "https://app.example"),
        ("Access-Control-Allow-Methods", "PUT"),
    ]);
    assert!(
        validate_preflight_response(
            &url("https://api.example/data"),
            "https://app.example",
            "PUT",
            &[("Content-Type", "application/json")],
            &response,
            false,
        )
        .is_ok()
    );
}
