use crate::*;

// ---------------------------------------------------------------------------
// Range / 206
// ---------------------------------------------------------------------------

#[test]
fn byte_range_parsing_and_errors() {
    assert_eq!(
        ByteRange::parse("bytes=0-499"),
        Ok(ByteRange::FromTo { start: 0, end: 499 })
    );
    assert_eq!(
        ByteRange::parse("bytes=500-"),
        Ok(ByteRange::From { start: 500 })
    );
    assert_eq!(
        ByteRange::parse("bytes=-200"),
        Ok(ByteRange::Suffix { length: 200 })
    );
    assert_eq!(
        ByteRange::parse(" bytes = 1 - 2 "),
        Err(RangeError::UnsupportedUnit("bytes = 1 - 2".to_owned()))
    );
    assert_eq!(
        ByteRange::parse("bytes=5-1"),
        Err(RangeError::Invalid("5-1".to_owned()))
    );
    assert_eq!(
        ByteRange::parse("bytes=0-1,3-4"),
        Err(RangeError::MultiRange)
    );
    assert_eq!(
        ByteRange::parse("bytes=-0"),
        Err(RangeError::Invalid("-0".to_owned()))
    );
    assert_eq!(
        ByteRange::parse("items=0-1"),
        Err(RangeError::UnsupportedUnit("items=0-1".to_owned()))
    );
    assert_eq!(
        ByteRange::FromTo { start: 2, end: 4 }.header_value(),
        "bytes=2-4"
    );
}

#[test]
fn byte_range_resolution() {
    assert_eq!(
        ByteRange::FromTo { start: 0, end: 999 }.resolve(10),
        Some((0, 9))
    );
    assert_eq!(
        ByteRange::FromTo { start: 2, end: 4 }.resolve(10),
        Some((2, 4))
    );
    assert_eq!(ByteRange::From { start: 5 }.resolve(10), Some((5, 9)));
    assert_eq!(ByteRange::Suffix { length: 3 }.resolve(10), Some((7, 9)));
    assert_eq!(ByteRange::Suffix { length: 30 }.resolve(10), Some((0, 9)));
    assert_eq!(ByteRange::From { start: 10 }.resolve(10), None);
    assert_eq!(ByteRange::FromTo { start: 0, end: 1 }.resolve(0), None);
}

#[test]
fn content_range_parsing() {
    assert_eq!(
        ContentRange::parse("BYTES 0-2/10").map(|range| (range.start, range.end)),
        Some((0, 2)),
        "range units are case-insensitive"
    );
    assert!(ContentRange::parse("Bytes */10").is_some());
    let satisfied = ContentRange::parse("bytes 0-499/1234").expect("valid");
    assert_eq!(satisfied.start, 0);
    assert_eq!(satisfied.end, 499);
    assert_eq!(satisfied.total, Some(1234));
    assert_eq!(satisfied.length(), 500);
    assert!(!satisfied.unsatisfied);
    let unsatisfied = ContentRange::parse("bytes */1234").expect("valid");
    assert!(unsatisfied.unsatisfied);
    assert_eq!(unsatisfied.total, Some(1234));
    assert_eq!(unsatisfied.to_string(), "bytes */1234");
    assert_eq!(ContentRange::parse("bytes 5-1/10"), None);
    assert_eq!(ContentRange::parse("bytes 0-20/10"), None);
    assert_eq!(ContentRange::parse("pages 0-1/10"), None);
}

#[test]
fn loopback_206_response_exposes_content_range() {
    let body = b"hello world";
    let responses = vec![response(
        "206 Partial Content",
        &[
            ("Content-Range", "bytes 6-10/11"),
            ("Accept-Ranges", "bytes"),
            ("Content-Type", "text/plain"),
        ],
        &body[6..],
    )];
    let (base, handle) = spawn_server(responses);
    let mut opts = options();
    opts.range = Some(ByteRange::FromTo { start: 6, end: 10 });
    let response = fetch(&url(&format!("{base}/range.txt")), &opts).expect("range fetch");
    assert_eq!(response.status, 206);
    assert!(response.is_partial());
    assert_eq!(response.body, b"world");
    assert!(response.accept_ranges);
    let range = response.content_range.expect("Content-Range parsed");
    assert_eq!((range.start, range.end, range.total), (6, 10, Some(11)));
    let requests = handle.join().expect("server thread");
    assert_eq!(
        header_value(&request_text(&requests[0]), "range"),
        Some("bytes=6-10")
    );
}

#[test]
fn loopback_200_ignored_range_has_no_content_range() {
    let responses = vec![response("200 OK", &[], b"full body")];
    let (base, handle) = spawn_server(responses);
    let mut opts = options();
    opts.range = Some(ByteRange::From { start: 2 });
    let response = fetch(&url(&format!("{base}/full")), &opts).expect("fetch");
    assert_eq!(response.status, 200);
    assert!(!response.is_partial());
    assert_eq!(response.body, b"full body");
    assert!(response.content_range.is_none());
    let requests = handle.join().expect("server thread");
    assert_eq!(
        header_value(&request_text(&requests[0]), "range"),
        Some("bytes=2-")
    );
}

#[test]
fn loader_load_range_http_slices_body() {
    let responses = vec![response(
        "206 Partial Content",
        &[("Content-Range", "bytes 2-4/11")],
        b"cde",
    )];
    let (base, handle) = spawn_server(responses);
    let mut loader = ResourceLoader::with_options(options());
    let resource = loader
        .load_range(
            &url(&format!("{base}/abc")),
            ByteRange::FromTo { start: 2, end: 4 },
        )
        .expect("load_range");
    assert_eq!(resource.bytes, b"cde");
    let requests = handle.join().expect("server thread");
    assert_eq!(
        header_value(&request_text(&requests[0]), "range"),
        Some("bytes=2-4")
    );
}

#[test]
fn loader_load_range_slices_files_and_rejects_unsatisfiable() {
    let path = std::env::temp_dir().join(format!("megi-net-range-{}.txt", std::process::id()));
    std::fs::write(&path, b"0123456789").expect("write fixture");
    let file_url = url(&format!("file://{}", path.display()));
    let mut loader = ResourceLoader::with_options(options());
    let middle = loader
        .load_range(&file_url, ByteRange::FromTo { start: 2, end: 5 })
        .expect("middle range");
    assert_eq!(middle.bytes, b"2345");
    let suffix = loader
        .load_range(&file_url, ByteRange::Suffix { length: 3 })
        .expect("suffix range");
    assert_eq!(suffix.bytes, b"789");
    let error = loader
        .load_range(&file_url, ByteRange::From { start: 20 })
        .expect_err("unsatisfiable range");
    assert!(matches!(
        error,
        megi_net::LoadError::Fetch(megi_net::FetchError::InvalidRange(_))
    ));
    let _ = std::fs::remove_file(path);
}
