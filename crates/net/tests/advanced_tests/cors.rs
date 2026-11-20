use crate::*;

// ---------------------------------------------------------------------------
// CORS
// ---------------------------------------------------------------------------

const APP_ORIGIN: &str = "https://app.example";

#[test]
fn cross_origin_with_exact_acao_passes_and_sends_origin() {
    let first = ok(&[("Access-Control-Allow-Origin", APP_ORIGIN)], b"data");
    let (base, server) = spawn_server(vec![first]);
    let options = FetchOptions {
        origin: Some(APP_ORIGIN.to_owned()),
        ..options()
    };
    let response = fetch(&base_url(&base).join("/api").unwrap(), &options).expect("CORS allowed");
    assert_eq!(response.body, b"data");
    let requests = server.join().expect("server thread");
    assert!(
        request_text(&requests[0]).contains(&format!("Origin: {APP_ORIGIN}")),
        "missing Origin header: {}",
        request_text(&requests[0])
    );
}

#[test]
fn cross_origin_with_wildcard_passes_without_credentials() {
    let first = ok(&[("Access-Control-Allow-Origin", "*")], b"data");
    let (base, server) = spawn_server(vec![first]);
    let options = FetchOptions {
        origin: Some(APP_ORIGIN.to_owned()),
        ..options()
    };
    assert!(fetch(&base_url(&base).join("/api").unwrap(), &options).is_ok());
    let _ = server.join();
}

#[test]
fn cross_origin_without_acao_is_a_cors_error() {
    let first = ok(&[], b"data");
    let (base, server) = spawn_server(vec![first]);
    let options = FetchOptions {
        origin: Some(APP_ORIGIN.to_owned()),
        ..options()
    };
    let error = fetch(&base_url(&base).join("/api").unwrap(), &options).unwrap_err();
    assert!(
        matches!(error, FetchError::Cors(ref reason) if reason.contains("Allow-Origin")),
        "{error}"
    );
    let _ = server.join();
}

#[test]
fn cross_origin_with_wrong_acao_is_a_cors_error() {
    let first = ok(
        &[("Access-Control-Allow-Origin", "https://other.example")],
        b"data",
    );
    let (base, server) = spawn_server(vec![first]);
    let options = FetchOptions {
        origin: Some(APP_ORIGIN.to_owned()),
        ..options()
    };
    let error = fetch(&base_url(&base).join("/api").unwrap(), &options).unwrap_err();
    assert!(
        matches!(error, FetchError::Cors(ref reason) if reason.contains("not allowed")),
        "{error}"
    );
    let _ = server.join();
}

#[test]
fn credentialed_wildcard_is_a_cors_error() {
    let first = ok(
        &[
            ("Access-Control-Allow-Origin", "*"),
            ("Access-Control-Allow-Credentials", "true"),
        ],
        b"data",
    );
    let (base, server) = spawn_server(vec![first]);
    let options = FetchOptions {
        origin: Some(APP_ORIGIN.to_owned()),
        cookie_jar: Some(jar()),
        ..options()
    };
    let error = fetch(&base_url(&base).join("/api").unwrap(), &options).unwrap_err();
    assert!(
        matches!(error, FetchError::Cors(ref reason) if reason.contains("credentials")),
        "{error}"
    );
    let _ = server.join();
}

#[test]
fn credentialed_exact_origin_with_allow_credentials_passes() {
    let first = ok(
        &[
            ("Access-Control-Allow-Origin", APP_ORIGIN),
            ("Access-Control-Allow-Credentials", "true"),
            ("Set-Cookie", "session=1"),
        ],
        b"data",
    );
    let (base, server) = spawn_server(vec![first]);
    let options = FetchOptions {
        origin: Some(APP_ORIGIN.to_owned()),
        cookie_jar: Some(jar()),
        ..options()
    };
    assert!(fetch(&base_url(&base).join("/api").unwrap(), &options).is_ok());
    let _ = server.join();
}

#[test]
fn same_origin_needs_no_acao_headers() {
    let first = ok(&[], b"data");
    let (base, server) = spawn_server(vec![first]);
    let options = FetchOptions {
        origin: Some(base.clone()),
        ..options()
    };
    assert!(fetch(&base_url(&base).join("/page").unwrap(), &options).is_ok());
    let _ = server.join();
}

#[test]
fn no_origin_means_no_origin_header_and_no_cors_checks() {
    let first = ok(&[], b"data");
    let (base, server) = spawn_server(vec![first]);
    assert!(fetch(&base_url(&base).join("/api").unwrap(), &options()).is_ok());
    let requests = server.join().expect("server thread");
    assert!(!request_text(&requests[0]).contains("Origin:"));
}

#[test]
fn preflight_ok_validates_method_and_headers() {
    let first = ok(
        &[
            ("Access-Control-Allow-Origin", APP_ORIGIN),
            ("Access-Control-Allow-Methods", "PUT"),
            ("Access-Control-Allow-Headers", "x-token"),
        ],
        b"",
    );
    let (base, server) = spawn_server(vec![first]);
    let options = FetchOptions {
        origin: Some(APP_ORIGIN.to_owned()),
        ..options()
    };
    preflight(
        &base_url(&base).join("/api").unwrap(),
        &options,
        "PUT",
        &[("X-Token", "secret")],
    )
    .expect("preflight allowed");
    let requests = server.join().expect("server thread");
    let request = request_text(&requests[0]);
    assert!(
        request.starts_with("OPTIONS /api HTTP/1.1\r\n"),
        "{request}"
    );
    assert!(
        request.contains(&format!("Origin: {APP_ORIGIN}")),
        "{request}"
    );
    assert!(
        request.contains("Access-Control-Request-Method: PUT"),
        "{request}"
    );
    assert!(
        request
            .to_ascii_lowercase()
            .contains("access-control-request-headers: x-token"),
        "{request}"
    );
    assert!(
        !request.contains("Cookie:"),
        "preflights must not send cookies"
    );
}

#[test]
fn preflight_denied_method_is_a_cors_error() {
    let first = ok(
        &[
            ("Access-Control-Allow-Origin", APP_ORIGIN),
            ("Access-Control-Allow-Methods", "GET"),
        ],
        b"",
    );
    let (base, server) = spawn_server(vec![first]);
    let options = FetchOptions {
        origin: Some(APP_ORIGIN.to_owned()),
        ..options()
    };
    let error = preflight(
        &base_url(&base).join("/api").unwrap(),
        &options,
        "DELETE",
        &[],
    )
    .unwrap_err();
    assert!(
        matches!(error, FetchError::Cors(ref reason) if reason.contains("DELETE")),
        "{error}"
    );
    let _ = server.join();
}

#[test]
fn preflight_missing_acao_is_a_cors_error() {
    let first = ok(&[("Access-Control-Allow-Methods", "PUT")], b"");
    let (base, server) = spawn_server(vec![first]);
    let options = FetchOptions {
        origin: Some(APP_ORIGIN.to_owned()),
        ..options()
    };
    let error =
        preflight(&base_url(&base).join("/api").unwrap(), &options, "PUT", &[]).unwrap_err();
    assert!(
        matches!(error, FetchError::Cors(ref reason) if reason.contains("Allow-Origin")),
        "{error}"
    );
    let _ = server.join();
}

#[test]
fn preflight_requires_configured_origin() {
    let error = preflight(
        &base_url("http://127.0.0.1:9").join("/api").unwrap(),
        &options(),
        "PUT",
        &[],
    )
    .unwrap_err();
    assert!(
        matches!(error, FetchError::Cors(ref reason) if reason.contains("origin")),
        "{error}"
    );
}

#[test]
fn simple_request_needs_no_preflight() {
    let (base, server) = spawn_server(Vec::new());
    let options = FetchOptions {
        origin: Some(APP_ORIGIN.to_owned()),
        ..options()
    };
    preflight(&base_url(&base).join("/api").unwrap(), &options, "GET", &[]).expect("no preflight");
    let requests = server.join().expect("server thread");
    assert!(
        requests.is_empty(),
        "simple method issued a preflight request"
    );
}

#[test]
fn preflight_never_sends_url_basic_credentials() {
    // #1860: preflight declares credential mode "omit" (Fetch §4.8.2), so URL userinfo
    // must not be turned into an Authorization header even though `fetch_once` normally
    // does that for real requests (#310).
    let first = ok(
        &[
            ("Access-Control-Allow-Origin", APP_ORIGIN),
            ("Access-Control-Allow-Methods", "PUT"),
        ],
        b"",
    );
    let (base, server) = spawn_server(vec![first]);
    let url = Url::parse(&format!(
        "http://user:pass@{}/api",
        base.trim_start_matches("http://")
    ))
    .expect("url with userinfo");
    let options = FetchOptions {
        origin: Some(APP_ORIGIN.to_owned()),
        ..options()
    };
    preflight(&url, &options, "PUT", &[]).expect("preflight allowed");
    let requests = server.join().expect("server thread");
    let request = request_text(&requests[0]);
    assert!(
        !request.to_ascii_lowercase().contains("authorization:"),
        "preflight must not send URL credentials: {request}"
    );
    // Sanity: without userinfo the same request still carries Origin and the ACRM header.
    assert!(
        request.contains(&format!("Origin: {APP_ORIGIN}")),
        "{request}"
    );
    assert!(
        request.contains("Access-Control-Request-Method: PUT"),
        "{request}"
    );
}
