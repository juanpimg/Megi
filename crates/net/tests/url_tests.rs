//! WHATWG URL subset tests (parse / serialize / relative resolution / typed errors).
//!
//! Vectors adapted from the WHATWG URL Standard examples
//! (<https://url.spec.whatwg.org/>) and WPT `url/resources/urltestdata.json` cases that
//! fall inside the supported subset (http/https/file, no IDNA/legacy forms).

use net::{Scheme, Url, UrlError};

fn parse(input: &str) -> Url {
    Url::parse(input).unwrap_or_else(|error| panic!("expected `{input}` to parse: {error}"))
}

fn serialized(input: &str) -> String {
    parse(input).as_str()
}

#[test]
fn parses_absolute_http_and_https_urls() {
    let url = parse("https://example.com");
    assert_eq!(url.scheme(), Scheme::Https);
    assert_eq!(url.host(), "example.com");
    assert_eq!(url.path(), "/");
    assert_eq!(url.as_str(), "https://example.com/");

    let url = parse("HTTP://EXAMPLE.COM:80/a/./b/../c?x=1#frag");
    assert_eq!(url.scheme(), Scheme::Http);
    assert_eq!(url.host(), "example.com");
    assert_eq!(url.path(), "/a/c");
    assert_eq!(url.query(), Some("x=1"));
    assert_eq!(url.fragment(), Some("frag"));
    assert_eq!(url.as_str(), "http://example.com/a/c?x=1#frag");
    // Default port is elided during serialization.
    assert_eq!(url.port(), None);
    assert_eq!(url.port_or_default(), Some(80));
}

#[test]
fn keeps_explicit_non_default_ports_and_elides_default_ones() {
    assert_eq!(
        serialized("http://example.com:8080/"),
        "http://example.com:8080/"
    );
    assert_eq!(
        serialized("https://example.com:8443/a"),
        "https://example.com:8443/a"
    );
    assert_eq!(
        parse("https://example.com:443/").as_str(),
        "https://example.com/"
    );
    assert_eq!(parse("https://example.com:443/").port(), None);
    assert_eq!(
        parse("https://example.com:443/").port_or_default(),
        Some(443)
    );
}

#[test]
fn parses_userinfo() {
    let url = parse("https://user:pass@example.com/p");
    assert_eq!(url.username(), "user");
    assert_eq!(url.password(), "pass");
    assert_eq!(url.as_str(), "https://user:pass@example.com/p");

    let url = parse("http://user@example.com/");
    assert_eq!(url.username(), "user");
    assert_eq!(url.password(), "");
    assert_eq!(url.as_str(), "http://user@example.com/");
}

#[test]
fn parses_ipv4_and_bracketed_ipv6_hosts() {
    let url = parse("http://192.168.0.1/x");
    assert_eq!(url.host(), "192.168.0.1");

    // Leading zeros are normalized.
    let url = parse("http://192.168.000.001/x");
    assert_eq!(url.host(), "192.168.0.1");

    let url = parse("http://[::1]:8080/x");
    assert_eq!(url.host(), "[::1]");
    assert_eq!(url.host_for_dns(), "::1");
    assert_eq!(url.port(), Some(8080));
    assert_eq!(url.as_str(), "http://[::1]:8080/x");

    let url = parse("http://[2001:db8::1]/");
    assert_eq!(url.host(), "[2001:db8::1]");
    assert_eq!(url.host_for_dns(), "2001:db8::1");
}

#[test]
fn percent_encodes_path_query_and_fragment() {
    assert_eq!(
        serialized("https://example.com/a b"),
        "https://example.com/a%20b"
    );
    assert_eq!(
        serialized("https://example.com/café"),
        "https://example.com/caf%C3%A9"
    );
    assert_eq!(
        serialized("https://example.com/?a=b c"),
        "https://example.com/?a=b%20c"
    );
    assert_eq!(
        serialized("https://example.com/#a b"),
        "https://example.com/#a%20b"
    );
    // Existing valid escapes are preserved verbatim.
    assert_eq!(
        serialized("https://example.com/a%20b"),
        "https://example.com/a%20b"
    );
}

#[test]
fn normalizes_dot_segments() {
    assert_eq!(serialized("http://a/x/./y"), "http://a/x/y");
    assert_eq!(serialized("http://a/x/y/../z"), "http://a/x/z");
    assert_eq!(serialized("http://a/x/y/.."), "http://a/x/");
    assert_eq!(serialized("http://a/../../x"), "http://a/x");
    assert_eq!(serialized("http://a/x//y"), "http://a/x//y");
}

#[test]
fn parses_file_urls() {
    let url = parse("file:///tmp/x.txt");
    assert_eq!(url.scheme(), Scheme::File);
    assert!(url.is_file());
    assert_eq!(url.host(), "");
    assert_eq!(url.path(), "/tmp/x.txt");
    assert_eq!(url.as_str(), "file:///tmp/x.txt");
    assert_eq!(url.port(), None);

    // `localhost` is normalized to the empty host; single-slash form is accepted.
    assert_eq!(parse("file://localhost/tmp/x").as_str(), "file:///tmp/x");
    assert_eq!(parse("file:/tmp/x").as_str(), "file:///tmp/x");
}

#[test]
fn request_target_and_cache_key() {
    let url = parse("http://example.com/a/b?x=1#frag");
    assert_eq!(url.request_target(), "/a/b?x=1");
    assert_eq!(url.cache_key(), "http://example.com/a/b?x=1");
    assert_eq!(parse("http://example.com").request_target(), "/");
}

#[test]
fn rejects_invalid_urls_with_typed_errors() {
    assert_eq!(Url::parse(""), Err(UrlError::EmptyInput));
    assert_eq!(Url::parse("   "), Err(UrlError::EmptyInput));
    assert_eq!(Url::parse("example.com/x"), Err(UrlError::MissingScheme));
    assert_eq!(Url::parse("http://"), Err(UrlError::MissingHost));
    assert!(matches!(
        Url::parse("ftp://example.com/"),
        Err(UrlError::UnsupportedScheme(scheme)) if scheme == "ftp"
    ));
    assert!(matches!(
        Url::parse("http://exa mple.com/"),
        Err(UrlError::InvalidHost(_))
    ));
    assert!(matches!(
        Url::parse("http://café.example/"),
        Err(UrlError::InvalidHost(_))
    ));
    assert!(matches!(
        Url::parse("http://example.com:99999/"),
        Err(UrlError::InvalidPort(_))
    ));
    assert!(matches!(
        Url::parse("http://example.com:http/"),
        Err(UrlError::InvalidPort(_))
    ));
    // A colon later in the path is data, not a port.
    assert_eq!(parse("file:///tmp/x:80").path(), "/tmp/x:80");
}

#[test]
fn joins_relative_urls_against_a_http_base() {
    let base = parse("https://example.com/a/b/c?q=1#f");
    let join = |relative: &str| {
        base.join(relative)
            .unwrap_or_else(|error| panic!("join `{relative}` failed: {error}"))
            .as_str()
    };

    assert_eq!(join("d"), "https://example.com/a/b/d");
    assert_eq!(join("../d"), "https://example.com/a/d");
    assert_eq!(join("../../d"), "https://example.com/d");
    assert_eq!(join("../../../d"), "https://example.com/d");
    assert_eq!(join("/d"), "https://example.com/d");
    assert_eq!(join("//other.example/x"), "https://other.example/x");
    assert_eq!(join("?x=2"), "https://example.com/a/b/c?x=2");
    assert_eq!(join("#g"), "https://example.com/a/b/c?q=1#g");
    assert_eq!(join(""), "https://example.com/a/b/c?q=1");
    assert_eq!(join("d?y=2#h"), "https://example.com/a/b/d?y=2#h");
    assert_eq!(join("https://abs.example/z"), "https://abs.example/z");
    assert_eq!(join("http://abs.example/z"), "http://abs.example/z");
    // Backslashes behave as slashes for special schemes.
    assert_eq!(join("..\\d"), "https://example.com/a/d");
}

#[test]
fn joins_relative_urls_against_a_file_base() {
    let base = parse("file:///tmp/dir/index.html");
    assert_eq!(
        base.join("style.css").expect("join").as_str(),
        "file:///tmp/dir/style.css"
    );
    assert_eq!(
        base.join("../img/logo.png").expect("join").as_str(),
        "file:///tmp/img/logo.png"
    );
    assert_eq!(base.join("/abs").expect("join").as_str(), "file:///abs");
    assert_eq!(
        base.join("//other/x").expect("join").as_str(),
        "file://other/x"
    );
}

#[test]
fn join_rejects_unsupported_schemes() {
    let base = parse("http://example.com/");
    assert!(matches!(
        base.join("ftp://example.com/x"),
        Err(UrlError::UnsupportedScheme(scheme)) if scheme == "ftp"
    ));
}

#[test]
fn accepts_an_empty_port_as_no_port() {
    // WHATWG URL "port state": an empty buffer sets the port to null (a validation error,
    // not a failure), so browsers and Node accept `http://host:/`.
    let url = parse("http://example.com:/");
    assert_eq!(url.port(), None);
    assert_eq!(url.as_str(), "http://example.com/");
    assert_eq!(
        parse("https://user@example.com:/p").as_str(),
        "https://user@example.com/p"
    );
    assert_eq!(parse("http://[::1]:/x").as_str(), "http://[::1]/x");

    // Out-of-range ports are still failures, and `file:` URLs never accept a port.
    assert!(matches!(
        Url::parse("http://example.com:65536/"),
        Err(UrlError::InvalidPort(_))
    ));
    assert!(matches!(
        Url::parse("file://host:/x"),
        Err(UrlError::InvalidPort(_))
    ));
}

#[test]
fn backslashes_are_separators_only_in_the_path_section() {
    // `\` is a path separator for special schemes, but `?`/`#` end the path state.
    assert_eq!(serialized("http://a/b\\c#frag"), "http://a/b/c#frag");
    assert_eq!(serialized("http://a?\\x"), "http://a/?\\x");
    assert_eq!(serialized("http://a#\\x"), "http://a/#\\x");

    let base = parse("http://a/b?q=1");
    assert_eq!(
        base.join("http://a/c?x=\\y")
            .expect("absolute join")
            .as_str(),
        "http://a/c?x=\\y"
    );
    assert_eq!(
        base.join("http://a/c#z\\w")
            .expect("absolute join")
            .as_str(),
        "http://a/c#z\\w"
    );
    assert_eq!(
        base.join("..\\d").expect("relative join").as_str(),
        "http://a/d"
    );
}
