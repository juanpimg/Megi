use crate::*;

// ---------------------------------------------------------------------------
// Compression
// ---------------------------------------------------------------------------

#[test]
fn gzip_is_negotiated_decoded_and_header_preserved() {
    let html = b"<!DOCTYPE html><html><head><title>gzip</title></head><body>hello</body></html>";
    let first = ok(&[("Content-Encoding", "gzip")], &gzip_stored(html));
    let (base, server) = spawn_server(vec![first]);
    let response = fetch(&base_url(&base).join("/page").unwrap(), &options()).expect("fetch");
    assert_eq!(response.body, html);
    assert_eq!(response.header("content-encoding"), Some("gzip"));
    let requests = server.join().expect("server thread");
    assert!(
        request_text(&requests[0]).contains("Accept-Encoding: gzip"),
        "missing Accept-Encoding: {}",
        request_text(&requests[0])
    );
}

#[test]
fn deflate_zlib_body_is_decoded() {
    let css = b"body { color: red; } /* deflate */";
    let first = ok(&[("Content-Encoding", "deflate")], &zlib_stored(css));
    let (base, server) = spawn_server(vec![first]);
    let response = fetch(&base_url(&base).join("/style.css").unwrap(), &options()).expect("fetch");
    assert_eq!(response.body, css);
    let _ = server.join();
}

#[test]
fn deflate_raw_body_is_decoded() {
    let data = b"raw deflate payload";
    let first = ok(&[("Content-Encoding", "deflate")], &deflate_stored(data));
    let (base, server) = spawn_server(vec![first]);
    let response = fetch(&base_url(&base).join("/").unwrap(), &options()).expect("fetch");
    assert_eq!(response.body, data);
    let _ = server.join();
}

#[test]
fn identity_mode_sends_no_accept_encoding_and_leaves_body_encoded() {
    let encoded = gzip_stored(b"compressed body");
    let first = ok(&[("Content-Encoding", "gzip")], &encoded);
    let (base, server) = spawn_server(vec![first]);
    let options = FetchOptions {
        accept_encoding: AcceptEncoding::Identity,
        ..options()
    };
    let response = fetch(&base_url(&base).join("/").unwrap(), &options).expect("fetch");
    assert_eq!(response.body, encoded, "identity mode must not decode");
    let requests = server.join().expect("server thread");
    assert!(
        !request_text(&requests[0]).contains("Accept-Encoding:"),
        "identity mode must not advertise encodings: {}",
        request_text(&requests[0])
    );
}

#[test]
fn corrupt_gzip_crc_is_a_typed_decompression_error() {
    let mut encoded = gzip_stored(b"payload");
    let crc_offset = encoded.len() - 8;
    encoded[crc_offset] ^= 0xFF;
    let first = ok(&[("Content-Encoding", "gzip")], &encoded);
    let (base, server) = spawn_server(vec![first]);
    let error = fetch(&base_url(&base).join("/").unwrap(), &options()).unwrap_err();
    assert!(
        matches!(error, FetchError::Decompression(ref reason) if reason.contains("CRC-32")),
        "{error}"
    );
    let _ = server.join();
}

#[test]
fn corrupt_gzip_isize_is_a_typed_decompression_error() {
    let mut encoded = gzip_stored(b"payload");
    let isize_offset = encoded.len() - 2;
    encoded[isize_offset] ^= 0x01;
    let first = ok(&[("Content-Encoding", "gzip")], &encoded);
    let (base, server) = spawn_server(vec![first]);
    let error = fetch(&base_url(&base).join("/").unwrap(), &options()).unwrap_err();
    assert!(
        matches!(error, FetchError::Decompression(ref reason) if reason.contains("ISIZE")),
        "{error}"
    );
    let _ = server.join();
}

#[test]
fn unsupported_content_encoding_is_rejected() {
    // #1918: `br` is now decoded (ADR-003); use a genuinely unsupported coding here.
    let first = ok(&[("Content-Encoding", "zstd")], b"zstd bytes");
    let (base, server) = spawn_server(vec![first]);
    let error = fetch(&base_url(&base).join("/").unwrap(), &options()).unwrap_err();
    assert!(
        matches!(error, FetchError::ContentEncoding(ref reason) if reason.contains("zstd")),
        "{error}"
    );
    let _ = server.join();
}

#[test]
fn compressed_body_larger_than_max_response_bytes_after_decode_is_too_large() {
    let html = vec![b'x'; 4096];
    let first = ok(&[("Content-Encoding", "gzip")], &gzip_stored(&html));
    let (base, server) = spawn_server(vec![first]);
    let options = FetchOptions {
        max_response_bytes: 1024,
        ..options()
    };
    let error = fetch(&base_url(&base).join("/").unwrap(), &options).unwrap_err();
    assert!(matches!(error, FetchError::ResponseTooLarge), "{error}");
    let _ = server.join();
}

#[test]
fn decompression_bomb_hits_ratio_cap_with_typed_error() {
    let first = ok(&[("Content-Encoding", "gzip")], &bomb_gzip());
    let (base, server) = spawn_server(vec![first]);
    let options = FetchOptions {
        compression_limits: CompressionLimits::new(64 * 1024 * 1024, 100),
        ..options()
    };
    let error = fetch(&base_url(&base).join("/").unwrap(), &options).unwrap_err();
    assert!(
        matches!(error, FetchError::DecompressionLimit { max_ratio: 100, .. }),
        "{error}"
    );
    let _ = server.join();
}

#[test]
fn decompression_bomb_hits_absolute_cap_with_typed_error() {
    let first = ok(&[("Content-Encoding", "gzip")], &bomb_gzip());
    let (base, server) = spawn_server(vec![first]);
    let options = FetchOptions {
        compression_limits: CompressionLimits::new(64 * 1024, 1000),
        ..options()
    };
    let error = fetch(&base_url(&base).join("/").unwrap(), &options).unwrap_err();
    assert!(
        matches!(
            error,
            FetchError::DecompressionLimit {
                max_output: 65536,
                ..
            }
        ),
        "{error}"
    );
    let _ = server.join();
}
