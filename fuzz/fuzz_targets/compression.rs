#![no_main]
//! Fuzz target: gzip/deflate/identity response-body decoding (#1424).
//!
//! Drives `decode_gzip`, `decode_deflate` and `decode_body` over untrusted
//! `Content-Encoding` payloads (gzip header flags/trailer, zlib/raw DEFLATE)
//! with bounded decompression limits so malformed streams cannot OOM.

use libfuzzer_sys::fuzz_target;
use megi_net::compression::{
    CompressionLimits, ContentEncoding, decode_body, decode_deflate, decode_gzip,
};

fuzz_target!(|data: &[u8]| {
    // Bound output to avoid OOM on decompression bombs while still exercising
    // the parser/limit paths (64 KiB output, 1000:1 ratio).
    let limits = CompressionLimits::new(64 * 1024, 1000);
    let _ = decode_gzip(data, limits);
    let _ = decode_deflate(data, limits);
    let _ = decode_body(ContentEncoding::Gzip, data, limits);
    let _ = decode_body(ContentEncoding::Deflate, data, limits);
    if data.len() <= limits.max_output {
        let _ = decode_body(ContentEncoding::Identity, data, limits);
    }
});
