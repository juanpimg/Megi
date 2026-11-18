use crate::*;

// ---------------------------------------------------------------------------
// Cookies
// ---------------------------------------------------------------------------

#[test]
fn cookie_is_stored_and_sent_on_the_next_request() {
    let first = ok(&[("Set-Cookie", "session=abc123; Path=/")], b"one");
    let second = ok(&[], b"two");
    let (base, server) = spawn_server(vec![first, second]);
    let options = FetchOptions {
        cookie_jar: Some(jar()),
        ..options()
    };
    let first_response =
        fetch(&base_url(&base).join("/a").unwrap(), &options).expect("first fetch");
    assert_eq!(first_response.body, b"one");
    let second_response =
        fetch(&base_url(&base).join("/b").unwrap(), &options).expect("second fetch");
    assert_eq!(second_response.body, b"two");

    let requests = server.join().expect("server thread");
    assert_eq!(requests.len(), 2);
    assert!(
        !request_text(&requests[0]).contains("Cookie:"),
        "first request must not send a cookie"
    );
    assert!(
        request_text(&requests[1]).contains("Cookie: session=abc123"),
        "second request missing stored cookie: {}",
        request_text(&requests[1])
    );
}

#[test]
fn no_cookie_jar_means_no_cookie_header() {
    let first = ok(&[("Set-Cookie", "a=1")], b"one");
    let second = ok(&[], b"two");
    let (base, server) = spawn_server(vec![first, second]);
    let options = options();
    fetch(&base_url(&base).join("/").unwrap(), &options).expect("first fetch");
    fetch(&base_url(&base).join("/").unwrap(), &options).expect("second fetch");
    let requests = server.join().expect("server thread");
    assert!(!request_text(&requests[1]).contains("Cookie:"));
}

#[test]
fn secure_cookie_is_not_sent_over_http() {
    let first = ok(&[("Set-Cookie", "s=secret; Secure")], b"one");
    let second = ok(&[], b"two");
    let (base, server) = spawn_server(vec![first, second]);
    let options = FetchOptions {
        cookie_jar: Some(jar()),
        ..options()
    };
    fetch(&base_url(&base).join("/").unwrap(), &options).expect("first fetch");
    fetch(&base_url(&base).join("/").unwrap(), &options).expect("second fetch");
    let requests = server.join().expect("server thread");
    assert!(
        !request_text(&requests[1]).contains("Cookie:"),
        "Secure cookie leaked over http"
    );
}

#[test]
fn max_age_zero_deletes_cookie() {
    let set = ok(&[("Set-Cookie", "a=1")], b"one");
    let delete = ok(&[("Set-Cookie", "a=; Max-Age=0")], b"two");
    let final_response = ok(&[], b"three");
    let (base, server) = spawn_server(vec![set, delete, final_response]);
    let options = FetchOptions {
        cookie_jar: Some(jar()),
        ..options()
    };
    let url = base_url(&base);
    fetch(&url, &options).expect("set");
    fetch(&url, &options).expect("delete");
    fetch(&url, &options).expect("after delete");
    let requests = server.join().expect("server thread");
    assert!(request_text(&requests[1]).contains("Cookie: a=1"));
    assert!(
        !request_text(&requests[2]).contains("Cookie:"),
        "deleted cookie still sent: {}",
        request_text(&requests[2])
    );
}

#[test]
fn path_cookie_only_sent_for_matching_paths() {
    let set = ok(&[("Set-Cookie", "p=1; Path=/shop")], b"one");
    let matching = ok(&[], b"two");
    let other = ok(&[], b"three");
    let (base, server) = spawn_server(vec![set, matching, other]);
    let options = FetchOptions {
        cookie_jar: Some(jar()),
        ..options()
    };
    let url = base_url(&base);
    fetch(&url.join("/shop").unwrap(), &options).expect("set");
    fetch(&url.join("/shop/cart").unwrap(), &options).expect("matching");
    fetch(&url.join("/blog").unwrap(), &options).expect("other");
    let requests = server.join().expect("server thread");
    assert!(request_text(&requests[1]).contains("Cookie: p=1"));
    assert!(!request_text(&requests[2]).contains("Cookie:"));
}
