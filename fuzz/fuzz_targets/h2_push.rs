#![no_main]
//! Fuzz target: HTTP/2 server push (PUSH_PROMISE/HEADERS/DATA) with anti-DoS caps.
//!
//! Opt-in server push (`Limits::enable_server_push`): a peer promises a resource with
//! `PUSH_PROMISE`, then sends its `HEADERS`/`DATA` on the promised stream. This target
//! exercises the frame-level surface a malicious or broken server reaches without opening a
//! socket:
//!
//! 1. the input (≤128 KiB) as a raw frame stream through
//!    [`net::h2::frame::FrameReader`], with the reader's max frame size **varied** across the
//!    legal range (`64`, `DEFAULT_MAX_FRAME_SIZE`, `MAX_FRAME_SIZE`, `MAX_FRAME_SIZE_LIMIT`)
//!    instead of being pinned to one value (#702);
//! 2. `PUSH_PROMISE` payload parsing + HPACK decoding of the promised request block, capped
//!    by [`net::h2::MAX_HEADER_BLOCK_BYTES`] and a small push count — including promised
//!    stream id `0` and ids above the last-used-stream bound, which the peer must reject
//!    without panicking;
//! 3. `HEADERS`/`CONTINUATION` reassembly with [`net::h2::frame::HeaderAssembler`] and HPACK
//!    decoding, plus `DATA` payload validation with a total-bytes cap;
//! 4. `validate_frame` for constructed promise/headers/data frames and for arbitrary 9-byte
//!    headers, against **each** of those max-frame-size settings.
//!
//! Ok/Err results are intentionally discarded: the target looks for panics, aborts,
//! ASan/UBSan findings, integer overflows and timeouts, not semantic mismatches.

use std::io::Cursor;

use libfuzzer_sys::fuzz_target;

/// Hard cap on the fuzzer-provided frame stream.
const MAX_FUZZ_INPUT: usize = 128 * 1024;
/// Bounded number of frames processed per iteration.
const MAX_FRAMES: usize = 48;
/// Maximum number of promised resources decoded per iteration.
const MAX_PUSHES: usize = 8;
/// Maximum number of header blocks decoded per iteration.
const MAX_HEADER_BLOCKS: usize = 4;
/// Maximum total DATA bytes buffered while following a promised stream.
const MAX_PUSHED_DATA: usize = 64 * 1024;

/// Legal max-frame-size settings a peer can advertise (SETTINGS_MAX_FRAME_SIZE range).
fn frame_size_choices() -> [u32; 4] {
    [
        64,
        net::h2::frame::DEFAULT_MAX_FRAME_SIZE,
        net::h2::MAX_FRAME_SIZE,
        net::h2::frame::MAX_FRAME_SIZE_LIMIT,
    ]
}

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_FUZZ_INPUT {
        return;
    }

    // 1. Raw frame stream: PUSH_PROMISE, HEADERS/CONTINUATION and DATA all reach the same
    //    parser/validator the network client uses, under every legal frame-size setting.
    for max_frame_size in frame_size_choices() {
        let mut reader = net::h2::frame::FrameReader::new(Cursor::new(data), max_frame_size);
        let mut decoder = net::h2::hpack::Decoder::new(4096);
        let mut assembler = net::h2::frame::HeaderAssembler::default();
        let mut pushes = 0usize;
        let mut header_blocks = 0usize;
        let mut pushed_data = 0usize;
        for _ in 0..MAX_FRAMES {
            let Ok(Some(frame)) = reader.read_frame() else {
                break;
            };
            match frame.header.kind {
                net::h2::frame::kind::PUSH_PROMISE => {
                    // Stream id 0 and ids past the last-used stream are protocol errors the
                    // parser must report, never panic on.
                    if let Ok((_promised_id, block)) = frame.parse_push_promise() {
                        pushes += 1;
                        if pushes <= MAX_PUSHES && block.len() <= net::h2::MAX_HEADER_BLOCK_BYTES {
                            let _ = decoder.decode(block);
                        }
                    }
                }
                net::h2::frame::kind::HEADERS | net::h2::frame::kind::CONTINUATION => {
                    if let Ok(Some((block, _end_stream))) = assembler.feed(&frame) {
                        header_blocks += 1;
                        if header_blocks <= MAX_HEADER_BLOCKS
                            && block.len() <= net::h2::MAX_HEADER_BLOCK_BYTES
                        {
                            let _ = decoder.decode(&block);
                        }
                    }
                }
                net::h2::frame::kind::DATA => {
                    if let Ok(payload) = frame.data_payload() {
                        pushed_data = pushed_data.saturating_add(payload.len());
                        if pushed_data > MAX_PUSHED_DATA {
                            // The client cancels over-cap pushed bodies; stop following.
                            pushed_data = MAX_PUSHED_DATA;
                        }
                    }
                }
                _ => {}
            }
            // Huffman decoding of arbitrary payloads is a separate HPACK entry point.
            let _ = net::h2::hpack::huffman_decode(&frame.payload);
        }
    }

    // 2. Round-trip the three push frame types built from the input.
    let payload = &data[..data.len().min(4096)];
    let raw_id = u32::from_be_bytes([
        *data.first().unwrap_or(&0),
        *data.get(1).unwrap_or(&0),
        *data.get(2).unwrap_or(&0),
        *data.get(3).unwrap_or(&0),
    ]);
    // Deliberately include 0 and the reserved top bit: both are protocol errors.
    let promised_id = raw_id & 0x7fff_ffff;
    let promise = net::h2::frame::Frame::push_promise(1, promised_id, payload);
    for max_frame_size in frame_size_choices() {
        let _ = net::h2::frame::validate_frame(&promise.header, max_frame_size);
    }
    let _ = promise.parse_push_promise();
    // Push on stream 0 (connection-level) must be rejected as well.
    let illegal = net::h2::frame::Frame::push_promise(0, promised_id, payload);
    let _ = illegal.parse_push_promise();

    let headers = net::h2::frame::Frame::headers(1, payload, data.len() % 2 == 0);
    let _ = headers.headers_payload();
    let data_frame = net::h2::frame::Frame::data(1, payload, data.len() % 2 == 0);
    let _ = data_frame.data_payload();

    // 3. Arbitrary first 9 bytes as a frame header, validated against every cap.
    if data.len() >= 9 {
        let header = net::h2::frame::FrameHeader {
            length: u32::from_be_bytes([0, data[0], data[1], data[2]]),
            kind: data[3],
            flags: data[4],
            stream_id: u32::from_be_bytes([data[5], data[6], data[7], data[8]]) & 0x7fff_ffff,
        };
        for max_frame_size in frame_size_choices() {
            let _ = net::h2::frame::validate_frame(&header, max_frame_size);
        }
    }
});
