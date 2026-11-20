//! HEADERS carrying a PRIORITY section must respect the peer's MAX_FRAME_SIZE (#1857).
//!
//! `Frame::headers_with_priority` prepends a 5-byte priority section. The request path
//! used to fragment the HPACK block to `peer_max_frame_size` and, when it fit in a single
//! frame, rebuild it with the priority section — producing a frame that was 5 bytes too
//! large (RFC 9113 §6.5.2 FRAME_SIZE_ERROR).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::time::Duration;

use megi_net::h2::client::{H2Request, Limits, Priority, request_many};
use megi_net::h2::frame::{CONNECTION_PREFACE, Frame, flags, kind, settings};
use megi_net::h2::hpack::Encoder;

const MAX_FRAME_SIZE: u32 = 16_384;

/// Picks a header value length so the HPACK block of a minimal GET plus one custom
/// header is exactly `target` bytes. The encoder adds a 3-byte length prefix for
/// values of this size, so the block length is monotonic in the value length.
fn pad_value_for_block(target: usize) -> String {
    let base: Vec<(String, String)> = vec![
        (":method".to_owned(), "GET".to_owned()),
        (":scheme".to_owned(), "http".to_owned()),
        (":authority".to_owned(), "example".to_owned()),
        (":path".to_owned(), "/".to_owned()),
    ];
    let size_with = |len: usize| {
        let mut headers = base.clone();
        headers.push(("x-pad".to_owned(), "a".repeat(len)));
        Encoder::new().encode(&headers).len()
    };
    let (mut lo, mut hi) = (0usize, target + 1);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if size_with(mid) < target {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    assert_eq!(size_with(lo), target, "target block size not reachable");
    "a".repeat(lo)
}

fn write_frame(stream: &mut TcpStream, frame: &Frame) {
    let mut bytes = Vec::new();
    frame.write_to(&mut bytes);
    stream.write_all(&bytes).expect("write frame");
    stream.flush().expect("flush");
}

/// Reads one raw frame and returns (kind, flags, length), enforcing the negotiated
/// MAX_FRAME_SIZE the same way a conforming peer does (RFC 9113 §6.5.2).
fn read_frame_header(stream: &mut TcpStream) -> (u8, u8, u32) {
    let mut header = [0u8; 9];
    stream.read_exact(&mut header).expect("frame header");
    let length = u32::from_be_bytes([0, header[0], header[1], header[2]]);
    let kind = header[3];
    let flags = header[4];
    assert!(
        length <= MAX_FRAME_SIZE,
        "frame length {length} exceeds peer MAX_FRAME_SIZE {MAX_FRAME_SIZE}"
    );
    let mut payload = vec![0u8; length as usize];
    stream.read_exact(&mut payload).expect("frame payload");
    (kind, flags, length)
}

#[test]
fn headers_with_priority_respect_peer_max_frame_size() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let authority = format!("127.0.0.1:{}", listener.local_addr().expect("addr").port());
    let (observed_tx, observed_rx) = mpsc::channel::<String>();

    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("read timeout");
        let mut preface = [0u8; CONNECTION_PREFACE.len()];
        stream.read_exact(&mut preface).expect("preface");
        assert_eq!(&preface, CONNECTION_PREFACE);

        loop {
            let (frame_kind, frame_flags, length) = read_frame_header(&mut stream);
            if frame_kind == kind::SETTINGS && frame_flags & flags::ACK == 0 {
                write_frame(
                    &mut stream,
                    &Frame::settings(&[(settings::MAX_FRAME_SIZE, MAX_FRAME_SIZE)]),
                );
                write_frame(&mut stream, &Frame::settings_ack());
                continue;
            }
            if frame_kind == kind::HEADERS {
                observed_tx
                    .send(format!("{length}"))
                    .expect("report HEADERS length");
                break;
            }
        }

        // Let the client finish cleanly.
        let block = Encoder::new().encode(&[(":status".to_owned(), "200".to_owned())]);
        write_frame(&mut stream, &Frame::headers(1, &block, true));
        std::thread::sleep(Duration::from_millis(100));
    });

    let padded = pad_value_for_block(MAX_FRAME_SIZE as usize);
    let request = H2Request::get("http", "example", "/", &[("x-pad".to_owned(), padded)])
        .with_priority(Priority {
            depends_on: 0,
            weight: 16,
            exclusive: false,
        });
    let mut stream = TcpStream::connect(&authority).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    let results = request_many(&mut stream, &[request], &Limits::default()).expect("session");
    assert_eq!(results.len(), 1);

    let reported = observed_rx
        .recv_timeout(Duration::from_secs(6))
        .expect("server report");
    let length: u32 = reported
        .parse()
        .unwrap_or_else(|_| panic!("unexpected report: {reported}"));
    assert!(
        length <= MAX_FRAME_SIZE,
        "HEADERS frame length {length} exceeds peer MAX_FRAME_SIZE {MAX_FRAME_SIZE}"
    );
}
