use super::*;

#[test]
fn percent_decode_roundtrip_for_ascii_and_utf8() {
    assert_eq!(percent_decode("/tmp/a%20b"), "/tmp/a b");
    assert_eq!(percent_decode("/caf%C3%A9"), "/café");
    assert_eq!(percent_decode("/bad%2"), "/bad%2");
    assert_eq!(percent_decode("/bad%zz"), "/bad%zz");
}

#[test]
fn oversized_hosts_are_rejected_quickly() {
    // 10k labels used to make the PSL/cookie/referrer walks take ~100 s (#1284).
    let host = "a.".repeat(10_000);
    let input = format!("http://{host}test/");
    let start = std::time::Instant::now();
    assert!(
        Url::parse(&input).is_err(),
        "oversized host must be rejected"
    );
    assert!(start.elapsed() < std::time::Duration::from_millis(100));
}

#[test]
fn normalize_path_resolves_dot_segments() {
    assert_eq!(normalize_path("/a/./b/../c"), "/a/c");
    assert_eq!(normalize_path("/a/b/.."), "/a/");
    assert_eq!(normalize_path("/a/b/."), "/a/b/");
    assert_eq!(normalize_path("/a//b"), "/a//b");
    assert_eq!(normalize_path("/../x"), "/x");
    assert_eq!(normalize_path("/"), "/");
}

#[test]
fn only_2e_escapes_are_dot_segments() {
    // #1404: `%2E`/`%2e` are traversal, every other `%?E` must stay literal.
    assert_eq!(normalize_path("/a/%2e%2e/b"), "/b");
    assert_eq!(normalize_path("/a/%2E/b"), "/a/b");
    // The real `%2E` still decodes, but the resulting `%3E.` is not a dot segment.
    assert_eq!(normalize_path("/secret/%3E%2E/x"), "/secret/%3E./x");
    assert_eq!(normalize_path("/%5E/x"), "/%5E/x");
    assert_eq!(normalize_path("/%7Ealice/"), "/%7Ealice/");
    assert_eq!(normalize_path("/%ce/"), "/%ce/");
}
