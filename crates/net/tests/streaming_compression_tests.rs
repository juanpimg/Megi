//! R218: incremental gzip/deflate decompression inside `fetch_streaming`.
//!
//! The loopback servers emit DEFLATE stored blocks as separate flushed writes, so a
//! buffered decoder would produce one callback at EOF; the incremental inflater must
//! deliver decompressed data as each block arrives, with typed errors and caps.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread::JoinHandle;
use std::time::Duration;

use net::{FetchError, FetchOptions, Url, fetch, fetch_streaming};

const GZIP_HEADER: [u8; 10] = [0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 255];

fn spawn_server(handler: impl FnOnce(TcpStream) + Send + 'static) -> (u16, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let port = listener.local_addr().expect("address").port();
    let handle = std::thread::spawn(move || {
        if let Ok((tcp, _)) = listener.accept() {
            tcp.set_read_timeout(Some(Duration::from_secs(5))).ok();
            tcp.set_write_timeout(Some(Duration::from_secs(5))).ok();
            handler(tcp);
        }
    });
    (port, handle)
}

fn read_request_head(tcp: &mut TcpStream) {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while head.len() < 64 * 1024 {
        match tcp.read(&mut byte) {
            Ok(0) | Err(_) => break,
            Ok(1) => {
                head.push(byte[0]);
                if head.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            Ok(_) => unreachable!(),
        }
    }
}

fn options() -> FetchOptions {
    FetchOptions {
        connect_timeout: Duration::from_secs(2),
        read_timeout: Duration::from_secs(2),
        ..FetchOptions::default()
    }
}

fn target(port: u16) -> Url {
    Url::parse(&format!("http://127.0.0.1:{port}/stream")).expect("target URL")
}

/// Builds a gzip member whose DEFLATE payload is a sequence of stored blocks (BFINAL
/// only on the last), so each part can be flushed independently.
fn gzip_stored(parts: &[Vec<u8>]) -> Vec<u8> {
    let mut out = GZIP_HEADER.to_vec();
    let mut all = Vec::new();
    for (index, part) in parts.iter().enumerate() {
        let final_block = index + 1 == parts.len();
        out.push(u8::from(final_block)); // BFINAL + BTYPE=00
        let len = part.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(part);
        all.extend_from_slice(part);
    }
    out.extend_from_slice(&net::compression::crc32_ieee(&all).to_le_bytes());
    out.extend_from_slice(&(all.len() as u32).to_le_bytes());
    out
}

/// Builds a zlib stream using stored DEFLATE blocks.
fn zlib_stored(parts: &[Vec<u8>]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    let mut all = Vec::new();
    for (index, part) in parts.iter().enumerate() {
        let final_block = index + 1 == parts.len();
        out.push(u8::from(final_block));
        let len = part.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(part);
        all.extend_from_slice(part);
    }
    out.extend_from_slice(&image::adler32(&all).to_be_bytes());
    out
}

fn pieces() -> Vec<Vec<u8>> {
    vec![vec![b'a'; 20_000], vec![b'b'; 20_000], vec![b'c'; 20_000]]
}

#[test]
fn gzip_is_delivered_incrementally_per_block() {
    let parts = pieces();
    let compressed = gzip_stored(&parts);
    let expected: Vec<u8> = parts.concat();
    let (port, server) = spawn_server(move |mut tcp| {
        read_request_head(&mut tcp);
        let _ = tcp.write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\n\r\n",
                compressed.len()
            )
            .as_bytes(),
        );
        let _ = tcp.flush();
        // Send the compressed stream one stored block at a time: header+block.
        let per_block = 10 + 5 + 20_000;
        let _ = tcp.write_all(&compressed[..per_block]);
        let _ = tcp.flush();
        std::thread::sleep(Duration::from_millis(40));
        let _ = tcp.write_all(&compressed[per_block..2 * per_block]);
        let _ = tcp.flush();
        std::thread::sleep(Duration::from_millis(40));
        let _ = tcp.write_all(&compressed[2 * per_block..]);
        let _ = tcp.flush();
    });
    let mut body = Vec::new();
    let mut calls = 0usize;
    let response = fetch_streaming(&target(port), &options(), &mut |chunk| {
        calls += 1;
        body.extend_from_slice(chunk);
        Ok(())
    })
    .expect("gzip streamed fetch");
    server.join().expect("server");
    assert_eq!(response.status, 200);
    assert_eq!(body, expected);
    assert!(
        calls >= 3,
        "expected >=3 decompressed callbacks, saw {calls}"
    );
}

#[test]
fn deflate_zlib_is_delivered_incrementally() {
    let parts = pieces();
    let compressed = zlib_stored(&parts);
    let expected: Vec<u8> = parts.concat();
    let (port, server) = spawn_server(move |tcp| {
        let mut tcp = tcp;
        read_request_head(&mut tcp);
        let _ = tcp.write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Encoding: deflate\r\nContent-Length: {}\r\n\r\n",
                compressed.len()
            )
            .as_bytes(),
        );
        let per_block = 2 + 5 + 20_000;
        for block in compressed.chunks(per_block) {
            let _ = tcp.write_all(block);
            let _ = tcp.flush();
            std::thread::sleep(Duration::from_millis(30));
        }
    });
    let mut body = Vec::new();
    let mut calls = 0usize;
    let response = fetch_streaming(&target(port), &options(), &mut |chunk| {
        calls += 1;
        body.extend_from_slice(chunk);
        Ok(())
    })
    .expect("deflate streamed fetch");
    server.join().expect("server");
    assert_eq!(response.status, 200);
    assert_eq!(body, expected);
    assert!(
        calls >= 3,
        "expected >=3 decompressed callbacks, saw {calls}"
    );
}

#[test]
fn raw_deflate_is_auto_detected() {
    let parts = [vec![b'x'; 4096], vec![b'y'; 4096]];
    let expected: Vec<u8> = parts.concat();
    let mut raw = Vec::new();
    for (index, part) in parts.iter().enumerate() {
        raw.push(u8::from(index + 1 == parts.len()));
        let len = part.len() as u16;
        raw.extend_from_slice(&len.to_le_bytes());
        raw.extend_from_slice(&(!len).to_le_bytes());
        raw.extend_from_slice(part);
    }
    let (port, server) = spawn_server(move |tcp| {
        let mut tcp = tcp;
        read_request_head(&mut tcp);
        let _ = tcp.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Encoding: deflate\r\nTransfer-Encoding: chunked\r\n\r\n",
        );
        for piece in raw.chunks(3000) {
            let _ = tcp.write_all(format!("{:x}\r\n", piece.len()).as_bytes());
            let _ = tcp.write_all(piece);
            let _ = tcp.write_all(b"\r\n");
            let _ = tcp.flush();
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = tcp.write_all(b"0\r\n\r\n");
        let _ = tcp.flush();
    });
    let mut body = Vec::new();
    let response = fetch_streaming(&target(port), &options(), &mut |chunk| {
        body.extend_from_slice(chunk);
        Ok(())
    })
    .expect("raw deflate streamed fetch");
    server.join().expect("server");
    assert_eq!(response.status, 200);
    assert_eq!(body, expected);
}

#[test]
fn gzip_crc_error_is_typed_and_aborts() {
    let parts = pieces();
    let mut compressed = gzip_stored(&parts);
    let crc_offset = compressed.len() - 8;
    compressed[crc_offset] ^= 0xFF;
    let (port, server) = spawn_server(move |mut tcp| {
        read_request_head(&mut tcp);
        let _ = tcp.write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\n\r\n",
                compressed.len()
            )
            .as_bytes(),
        );
        let _ = tcp.write_all(&compressed);
        let _ = tcp.flush();
    });
    let error = fetch_streaming(&target(port), &options(), &mut |_chunk| Ok(()))
        .expect_err("corrupt gzip must fail");
    server.join().expect("server");
    assert!(matches!(error, FetchError::Decompression(_)), "{error:?}");
}

#[test]
fn streaming_respects_the_decompressed_size_cap() {
    let parts = pieces();
    let compressed = gzip_stored(&parts);
    let (port, server) = spawn_server(move |mut tcp| {
        read_request_head(&mut tcp);
        let _ = tcp.write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\n\r\n",
                compressed.len()
            )
            .as_bytes(),
        );
        let _ = tcp.write_all(&compressed);
        let _ = tcp.flush();
    });
    let mut options = options();
    options.max_response_bytes = 1024;
    let error =
        fetch_streaming(&target(port), &options, &mut |_chunk| Ok(())).expect_err("cap must fail");
    server.join().expect("server");
    assert!(
        matches!(
            error,
            FetchError::ResponseTooLarge | FetchError::DecompressionLimit { .. }
        ),
        "{error:?}"
    );
}

#[test]
fn normal_fetch_still_returns_the_decoded_body() {
    let parts = pieces();
    let compressed = gzip_stored(&parts);
    let expected: Vec<u8> = parts.concat();
    let (port, server) = spawn_server(move |mut tcp| {
        read_request_head(&mut tcp);
        let _ = tcp.write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\n\r\n",
                compressed.len()
            )
            .as_bytes(),
        );
        let _ = tcp.write_all(&compressed);
        let _ = tcp.flush();
    });
    let response = fetch(&target(port), &options()).expect("normal fetch");
    server.join().expect("server");
    assert_eq!(response.body, expected);
}

#[test]
fn unsupported_content_encoding_is_a_typed_streaming_error() {
    for declared in ["br", "gzip, br", "nonsense"] {
        let (port, handle) = spawn_server(move |mut tcp| {
            read_request_head(&mut tcp);
            let body = b"still-compressed".to_vec();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Encoding: {declared}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = tcp.write_all(response.as_bytes());
            let _ = tcp.write_all(&body);
        });
        let mut delivered: Vec<Vec<u8>> = Vec::new();
        let error = fetch_streaming(&target(port), &options(), &mut |data: &[u8]| {
            delivered.push(data.to_vec());
            Ok(())
        })
        .expect_err("unsupported Content-Encoding must be a typed error");
        handle.join().ok();
        assert!(
            matches!(&error, FetchError::ContentEncoding(reason) if !reason.is_empty()),
            "unexpected error for {declared:?}: {error:?}"
        );
        assert!(
            delivered.is_empty(),
            "no still-compressed bytes may be delivered for {declared:?}"
        );
    }
}
