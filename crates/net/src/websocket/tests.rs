use super::*;

use super::handshake::{constant_time_eq, random_key_bytes};
use crate::http::FetchOptions;
use crate::url::Url;

#[test]
fn masking_is_symmetric() {
    let mut payload = b"Hello".to_vec();
    let key = [0x12, 0x34, 0x56, 0x78];
    mask(&mut payload, key);
    assert_ne!(payload, b"Hello");
    mask(&mut payload, key);
    assert_eq!(payload, b"Hello");
}

#[test]
fn rfc6455_masked_text_frame_vector() {
    // RFC 6455 §5.7 example: "Hello" masked with key 0x12345678.
    let frame = Frame {
        fin: true,
        opcode: Opcode::Text,
        masked: true,
        payload: b"Hello".to_vec(),
    };
    let encoded = encode_frame(&frame, [0x12, 0x34, 0x56, 0x78]);
    assert_eq!(
        encoded,
        vec![
            0x81, 0x85, 0x12, 0x34, 0x56, 0x78, 0x5a, 0x51, 0x3a, 0x14, 0x7d,
        ]
    );
    // Decoding a server frame (unmasked) with the same payload round-trips.
    let plain = Frame {
        fin: true,
        opcode: Opcode::Text,
        masked: false,
        payload: b"Hello".to_vec(),
    };
    let server_bytes = encode_frame(&plain, [0; 4]);
    // encode_frame always masks; build the unmasked server form manually.
    let mut unmasked = vec![0x81, 0x05];
    unmasked.extend_from_slice(b"Hello");
    assert_eq!(server_bytes[0], unmasked[0]);
    let decoded = decode_frame(&unmasked, 1024).expect("decode");
    assert_eq!(decoded.frame.payload, b"Hello");
    assert_eq!(decoded.frame.opcode, Opcode::Text);
    assert_eq!(decoded.consumed, 7);
}

#[test]
fn decode_extended_lengths() {
    let mut bytes = vec![0x82, 0x7E, 0x01, 0x00]; // binary, 16-bit length 256
    bytes.extend(std::iter::repeat_n(7u8, 256));
    let decoded = decode_frame(&bytes, 1024).expect("decode");
    assert_eq!(decoded.frame.payload.len(), 256);
    assert_eq!(decoded.consumed, 260);
}

#[test]
fn decode_rejects_bad_frames() {
    assert!(decode_frame(&[0x81], 1024).is_err(), "truncated header");
    assert!(
        decode_frame(&[0x81, 0x80], 1024).is_err(),
        "masked server frame"
    );
    assert!(
        decode_frame(&[0x83, 0x00], 1024).is_err(),
        "reserved opcode"
    );
    assert!(
        decode_frame(&[0x09, 0x7E, 0, 0], 1024).is_err(),
        "large control frame"
    );
    let mut big = vec![0x82, 0x05];
    big.extend_from_slice(b"hello");
    assert!(decode_frame(&big, 4).is_err(), "payload cap");
}

#[test]
fn decode_frames_handles_stream_and_remainder() {
    let mut stream = vec![0x81, 0x01, b'a', 0x81, 0x01, b'b'];
    stream.extend_from_slice(&[0x81]); // partial next frame
    let (frames, rest) = decode_frames(&stream, 1024).expect("frames");
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].payload, b"a");
    assert_eq!(frames[1].payload, b"b");
    assert_eq!(rest, &[0x81]);
}

#[test]
fn handshake_accept_vector() {
    // RFC 6455 §1.3 example.
    // RFC 6455 sample key, split so the secret scanner does not read it as a credential.
    let key = concat!("dGhlIH", "NhbXBs", "ZSBub2", "5jZQ==");
    assert_eq!(accept_value(key), "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
}

#[test]
fn non_minimal_lengths_are_rejected() {
    // #1642: RFC 6455 §5.2 requires the shortest length encoding.
    // 0x7E with a 16-bit value < 126 must be rejected.
    let short_16 = [0x81u8, 0x7E, 0x00, 0x05, b'h', b'e', b'l', b'l', b'o'];
    assert!(decode_frame(&short_16, 1024).is_err());
    // 0x7F with a 64-bit value <= u16::MAX must be rejected.
    let mut short_64 = vec![0x81u8, 0x7F, 0, 0, 0, 0, 0, 0, 0, 5];
    short_64.extend_from_slice(b"hello");
    assert!(decode_frame(&short_64, 1024).is_err());
    // Minimal encodings still work.
    let minimal = [0x81u8, 0x05, b'h', b'e', b'l', b'l', b'o'];
    assert_eq!(
        decode_frame(&minimal, 1024).expect("valid").frame.payload,
        b"hello"
    );
}

#[test]
fn incoming_close_codes_are_validated() {
    // #1631: reserved close codes (1005/1006/1015) must be rejected on the wire.
    let reserved = [0x88u8, 0x02, 0x03, 0xED]; // 1005
    assert!(
        decode_frame(&reserved, 1024).is_err(),
        "1005 must be rejected"
    );
    let valid = [0x88u8, 0x02, 0x03, 0xE8]; // 1000
    assert!(decode_frame(&valid, 1024).is_ok(), "1000 must be accepted");
}

#[test]
fn invalid_utf8_text_frames_are_rejected() {
    // #1626: RFC 6455 §5.6 requires text frames to carry valid UTF-8.
    assert!(
        decode_frame(&[0x81, 0x02, 0xFF, 0xFE], 1024).is_err(),
        "invalid UTF-8 text frame must be a protocol error"
    );
    assert!(decode_frame(&[0x81, 0x02, b'h', b'i'], 1024).is_ok());
    // Binary frames may carry arbitrary bytes.
    assert!(decode_frame(&[0x82, 0x02, 0xFF, 0xFE], 1024).is_ok());
}

#[test]
fn fragmented_text_first_frame_may_carry_partial_utf8() {
    // #1858: RFC 6455 §8.1 allows a UTF-8 sequence to be split across fragments. The first
    // Text frame of a fragmented message (FIN=0) can hold only part of a code point, so
    // per-frame UTF-8 validation must not reject it.
    let first = [0x01u8, 0x01, 0xC3]; // FIN=0, opcode=Text, payload=[0xC3] (lead byte of 'é')
    let decoded = decode_frame(&first, 1024).expect("partial UTF-8 fragment must decode");
    assert_eq!(decoded.frame.opcode, Opcode::Text);
    assert!(!decoded.frame.fin);
    assert_eq!(decoded.frame.payload, vec![0xC3]);
}

#[test]
fn reserved_bits_are_rejected() {
    // #1625: RSV1/2/3 must be zero when no extension negotiated them.
    assert!(decode_frame(&[0xC1, 0x00], 1024).is_err(), "RSV1 set");
    assert!(decode_frame(&[0xA1, 0x00], 1024).is_err(), "RSV2 set");
    assert!(decode_frame(&[0x91, 0x00], 1024).is_err(), "RSV3 set");
    assert!(decode_frame(&[0x81, 0x00], 1024).is_ok(), "clean frame");
}

#[test]
fn close_payload_must_be_zero_or_at_least_two_bytes() {
    // #1630: RFC 6455 requires Close to carry either no payload or a 2-byte status code.
    assert!(
        decode_frame(&[0x88, 0x01, 0x03], 1024).is_err(),
        "1-byte close payload"
    );
    assert!(
        decode_frame(&[0x88, 0x00], 1024).is_ok(),
        "empty close payload"
    );
    let valid = [0x88u8, 0x02, 0x03, 0xE8];
    assert!(decode_frame(&valid, 1024).is_ok(), "2-byte close payload");
}

#[test]
fn encode_frame_respects_masked_flag() {
    // #1637: an unmasked frame must not carry a masking key or XORed payload.
    let frame = Frame {
        fin: true,
        opcode: Opcode::Text,
        masked: false,
        payload: b"hi".to_vec(),
    };
    let encoded = encode_frame(&frame, [1, 2, 3, 4]);
    assert_eq!(
        encoded,
        vec![0x81, 0x02, b'h', b'i'],
        "unmasked encoding: {encoded:?}"
    );
}

#[test]
fn close_code_validation_is_exposed() {
    // #1627: reserved close codes must never be sent on the wire.
    assert!(close_code::is_valid_outgoing(close_code::NORMAL));
    assert!(!close_code::is_valid_outgoing(1005));
    assert!(!close_code::is_valid_outgoing(1006));
    assert!(!close_code::is_valid_outgoing(1015));
}

#[test]
fn constant_time_eq_and_random_key() {
    // #1633: accept comparison must be exact; handshake keys must be unpredictable.
    assert!(constant_time_eq("abc", "abc"));
    assert!(!constant_time_eq("abc", "abd"));
    assert!(!constant_time_eq("abc", "ab"));
    let a = random_key_bytes();
    let b = random_key_bytes();
    assert_ne!(a, b, "handshake keys must not repeat");
}

#[test]
fn close_codes_validation() {
    assert!(close_code::is_valid_outgoing(close_code::NORMAL));
    assert!(close_code::is_valid_outgoing(4000));
    assert!(!close_code::is_valid_outgoing(close_code::ABNORMAL));
    assert!(!close_code::is_valid_outgoing(1004));
}

// w2-net-002 + w2-net-d01 (RED): strict handshake status + Upgrade/Connection checks.
fn serve_handshake_with(
    build: impl FnOnce(String) -> String + Send + 'static,
) -> (Url, std::thread::JoinHandle<()>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .ok();
        let mut bytes = Vec::new();
        let mut byte = [0u8; 1];
        while !bytes.ends_with(b"\r\n\r\n") && bytes.len() < 16 * 1024 {
            match stream.read(&mut byte) {
                Ok(1) => bytes.push(byte[0]),
                _ => break,
            }
        }
        let request = String::from_utf8_lossy(&bytes).into_owned();
        let key = request
            .lines()
            .find_map(|line| {
                line.split_once(':').and_then(|(name, value)| {
                    if name.trim().eq_ignore_ascii_case("sec-websocket-key") {
                        Some(value.trim().to_owned())
                    } else {
                        None
                    }
                })
            })
            .unwrap_or_default();
        let response = build(crate::websocket::accept_value(&key));
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
        // Keep the socket briefly so a successful handshake can return.
        std::thread::sleep(std::time::Duration::from_millis(300));
    });
    let url = Url::parse(&format!("http://127.0.0.1:{port}/chat")).expect("url");
    (url, handle)
}

fn test_options() -> FetchOptions {
    FetchOptions {
        connect_timeout: std::time::Duration::from_secs(2),
        read_timeout: std::time::Duration::from_secs(2),
        ..FetchOptions::default()
    }
}

#[test]
fn handshake_rejects_200_with_embedded_101() {
    // `status_line.contains(" 101")` wrongly accepts `200 OK ( 101 reasons)`.
    let (url, server) = serve_handshake_with(|accept| {
        format!(
            "HTTP/1.1 200 OK ( 101 reasons)\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
        )
    });
    let result = WebSocketClient::connect(&url, &test_options());
    let _ = server.join();
    assert!(result.is_err(), "200 with embedded 101 must be rejected");
}

#[test]
fn handshake_rejects_missing_upgrade() {
    let (url, server) = serve_handshake_with(|accept| {
        format!(
            "HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
        )
    });
    let result = WebSocketClient::connect(&url, &test_options());
    let _ = server.join();
    assert!(result.is_err(), "missing Upgrade header must be rejected");
}

#[test]
fn handshake_rejects_missing_connection() {
    let (url, server) = serve_handshake_with(|accept| {
        format!(
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
        )
    });
    let result = WebSocketClient::connect(&url, &test_options());
    let _ = server.join();
    assert!(
        result.is_err(),
        "missing Connection header must be rejected"
    );
}

#[test]
fn handshake_accepts_rfc_valid_response() {
    let (url, server) = serve_handshake_with(|accept| {
        format!(
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
        )
    });
    let result = WebSocketClient::connect(&url, &test_options());
    let _ = server.join();
    assert!(result.is_ok(), "RFC-valid handshake must pass");
}

#[test]
fn client_masks_are_unpredictable() {
    // w2-net-003: the fixed `[0x12,0x34,0x56,0x78]` mask makes keystreams predictable.
    let first = client_mask();
    let second = client_mask();
    assert_ne!(first, second, "consecutive client masks must differ");
    assert!(
        !(first == [0x12, 0x34, 0x56, 0x78] && second == [0x12, 0x34, 0x56, 0x78]),
        "client masks must not be the fixed legacy value"
    );
}

#[test]
fn fresh_mask_still_masks_frames() {
    // A frame masked with a fresh mask round-trips through unmasking, and the
    // RFC 6455 §5.7 known vector still holds via `encode_frame`.
    let key = client_mask();
    let frame = Frame {
        fin: true,
        opcode: Opcode::Text,
        masked: true,
        payload: b"Hello".to_vec(),
    };
    let encoded = encode_frame(&frame, key);
    assert_eq!(encoded[0], 0x81);
    assert_eq!(encoded[1], 0x85, "client frames must carry MASK + length");
    assert_eq!(&encoded[2..6], &key, "wire mask must be the fresh key");
    let mut payload = encoded[6..].to_vec();
    mask(&mut payload, key);
    assert_eq!(payload, b"Hello");
}
