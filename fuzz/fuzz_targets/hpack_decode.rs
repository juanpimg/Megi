#![no_main]
//! Fuzz target: HTTP/2 HPACK decoding + frame parsing with anti-DoS caps.
//!
//! The input (≤64 KiB) is exercised through three paths:
//!
//! 1. as a complete HPACK header block fed to [`net::h2::hpack::Decoder`]
//!    (static/dynamic tables, Huffman, literals, table-size updates);
//! 2. as a raw HTTP/2 frame stream read by [`net::h2::frame::FrameReader`]
//!    (length/flag/stream-id validation, SETTINGS/WINDOW_UPDATE/PING/RST_STREAM/
//!    GOAWAY parsing, HEADERS/CONTINUATION payload decoding through HPACK);
//! 3. its first 9 bytes as an arbitrary frame header validated against
//!    [`net::h2::MAX_FRAME_SIZE`].
//!
//! The reader caps frame sizes at 16 KiB and the loop stops after 64 frames, so
//! a single iteration is bounded. Ok/Err results are intentionally discarded:
//! the target looks for panics, aborts, ASan/UBSan findings, integer overflows
//! and timeouts.

use std::io::Cursor;

use libfuzzer_sys::fuzz_target;

const MAX_FUZZ_INPUT: usize = 64 * 1024;
const MAX_FRAMES: usize = 64;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_FUZZ_INPUT {
        return;
    }

    // 1. Raw HPACK block against a fresh dynamic table.
    let mut decoder = net::h2::hpack::Decoder::new(4096);
    if let Ok(headers) = decoder.decode(data) {
        for (name, value) in headers {
            let _ = name.len() + value.len();
        }
    }

    // 2. HTTP/2 frame stream (bounded by MAX_FRAMES and the 16 KiB frame cap).
    let mut reader =
        net::h2::frame::FrameReader::new(Cursor::new(data), net::h2::MAX_FRAME_SIZE);
    for _ in 0..MAX_FRAMES {
        match reader.read_frame() {
            Ok(Some(frame)) => {
                let _ = frame.parse_settings();
                let _ = frame.window_increment();
                let _ = frame.parse_rst_stream();
                let _ = frame.parse_goaway();
                let _ = frame.data_payload();
                if let Ok(payload) = frame.headers_payload() {
                    let mut decoder = net::h2::hpack::Decoder::new(4096);
                    if let Ok(headers) = decoder.decode(payload) {
                        for (name, value) in headers {
                            let _ = name.len() + value.len();
                        }
                    }
                }
            }
            Ok(None) | Err(_) => break,
        }
    }

    // 3. Arbitrary first 9 bytes interpreted as a frame header.
    if data.len() >= 9 {
        let header = net::h2::frame::FrameHeader {
            length: u32::from_be_bytes([0, data[0], data[1], data[2]]),
            kind: data[3],
            flags: data[4],
            stream_id: u32::from_be_bytes([data[5], data[6], data[7], data[8]]) & 0x7fff_ffff,
        };
        let _ = net::h2::frame::validate_frame(&header, net::h2::MAX_FRAME_SIZE);
    }
});
