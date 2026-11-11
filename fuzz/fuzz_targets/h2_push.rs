#![no_main]
//! Fuzz target: HTTP/2 server push (PUSH_PROMISE/HEADERS/DATA) with anti-DoS caps.
//!
//! Round 28 added opt-in server push (`Limits::enable_server_push`): a peer
//! promises a resource with `PUSH_PROMISE`, then sends its `HEADERS`/`DATA` on the
//! promised stream. This target exercises the frame-level surface a malicious or
//! broken server reaches without opening a socket:
//!
//! 1. the input (≤128 KiB) as a raw frame stream through
//!    [`net::h2::frame::FrameReader`] (16 KiB frame cap, bounded frame count);
//! 2. `PUSH_PROMISE` payload parsing + HPACK decoding of the promised request
//!    block, capped by [`net::h2::MAX_HEADER_BLOCK_BYTES`] and a small push count;
//! 3. `HEADERS`/`CONTINUATION` reassembly with [`net::h2::frame::HeaderAssembler`]
//!    and HPACK decoding, plus `DATA` payload validation with a total-bytes cap;
//! 4. round-trips of frames constructed from the input (promise/headers/data) and
//!    arbitrary 9-byte headers through [`net::h2::frame::validate_frame`].
//!
//! Ok/Err results are intentionally discarded: the target looks for panics,
//! aborts, ASan/UBSan findings and timeouts, not semantic mismatches.

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

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_FUZZ_INPUT {
        return;
    }

    // 1. Raw frame stream: PUSH_PROMISE, HEADERS/CONTINUATION and DATA all reach
    //    the same parser/validator the network client uses.
    let mut reader = net::h2::frame::FrameReader::new(Cursor::new(data), net::h2::MAX_FRAME_SIZE);
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
                let Ok((_promised_id, block)) = frame.parse_push_promise() else {
                    continue;
                };
                pushes += 1;
                if pushes <= MAX_PUSHES && block.len() <= net::h2::MAX_HEADER_BLOCK_BYTES {
                    let _ = decoder.decode(block);
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

    // 2. Round-trip the three push frame types built from the input.
    let payload = &data[..data.len().min(4096)];
    let promised_id = (u32::from_be_bytes([
        *data.first().unwrap_or(&0),
        *data.get(1).unwrap_or(&0),
        *data.get(2).unwrap_or(&0),
        *data.get(3).unwrap_or(&0),
    ]) & 0x7fff_ffff)
        .max(1);
    let promise = net::h2::frame::Frame::push_promise(1, promised_id, payload);
    let _ = net::h2::frame::validate_frame(&promise.header, net::h2::frame::DEFAULT_MAX_FRAME_SIZE);
    let _ = promise.parse_push_promise();

    let headers = net::h2::frame::Frame::headers(1, payload, data.len() % 2 == 0);
    let _ = headers.headers_payload();
    let data_frame = net::h2::frame::Frame::data(1, payload, data.len() % 2 == 0);
    let _ = data_frame.data_payload();

    // 3. Arbitrary first 9 bytes as a frame header, validated against the caps.
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
