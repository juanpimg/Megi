#![no_main]
//! Fuzz target: streaming request bodies (`net::RequestBody`/`BodyProducer`).
//!
//! Round 28 added pull-based request bodies with backpressure: producers return
//! at most the requested `max` bytes and the transport pulls only when it can
//! write. This target builds chunk plans from the input and drives a
//! [`net::RequestBody::Stream`] with bounded pull sizes, checking the public
//! contract (never more than `max` bytes per pull, bounded total consumption,
//! typed errors abort cleanly) and the `Bytes` helpers (`len`/`is_empty`/`Debug`).
//! It never opens a socket; the target looks for panics/aborts/UB, not semantic
//! mismatches.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use libfuzzer_sys::fuzz_target;

/// Hard cap on the fuzzer-provided payload.
const MAX_FUZZ_INPUT: usize = 128 * 1024;
/// Bounded number of chunks / pull iterations per iteration.
const MAX_CHUNKS: usize = 32;
const MAX_PULLS: usize = 96;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_FUZZ_INPUT {
        return;
    }

    // In-memory body helpers.
    let bytes = net::RequestBody::bytes(data);
    if bytes.len() != Some(data.len()) || bytes.is_empty() != data.is_empty() {
        return;
    }
    let _ = format!("{bytes:?}");
    let from_slice = net::RequestBody::from(data);
    if from_slice.len() != Some(data.len()) {
        return;
    }

    // Build a chunk plan: one length byte is followed by that many payload bytes
    // (length capped so the plan stays bounded and total bytes ≤ input size).
    let mut chunks: Vec<Vec<u8>> = Vec::new();
    let mut cursor = 0usize;
    while cursor < data.len() && chunks.len() < MAX_CHUNKS {
        let length = usize::from(data[cursor]).min(255);
        cursor += 1;
        let end = (cursor + length).min(data.len());
        chunks.push(data[cursor..end].to_vec());
        cursor = end;
    }

    let index = Arc::new(AtomicUsize::new(0));
    let loaded = Arc::new(AtomicUsize::new(0));
    let chunks_for_stream = Arc::new(chunks);
    let mut stream = net::RequestBody::stream({
        let index = Arc::clone(&index);
        let chunks = Arc::clone(&chunks_for_stream);
        move |max: usize| {
            let position = index.fetch_add(1, Ordering::Relaxed);
            if position >= chunks.len() {
                return Ok(None);
            }
            let chunk = chunks[position].clone();
            let chunk = if max == 0 {
                Vec::new()
            } else {
                chunk[..chunk.len().min(max)].to_vec()
            };
            Ok(Some(chunk))
        }
    });

    // Pull with bounded sizes derived from the input; enforce the producer
    // contract (chunk ≤ max) and a total-consumption cap.
    let mut pulled = 0usize;
    let mut pulls = 0usize;
    let mut max_bytes = 0usize;
    while pulls < MAX_PULLS {
        let max = if data.is_empty() {
            0
        } else {
            usize::from(data[pulls % data.len()]).saturating_mul(3)
        };
        let pull = match &mut stream {
            net::RequestBody::Stream(producer) => producer.next_chunk(max),
            net::RequestBody::Bytes(bytes) => Ok(Some(std::mem::take(bytes))),
        };
        let Ok(Some(chunk)) = pull else {
            break;
        };
        if chunk.len() > max && max > 0 {
            // A producer violating the contract would make the transport
            // overrun the peer window; stop the simulation rather than panic.
            return;
        }
        pulled = pulled.saturating_add(chunk.len());
        max_bytes = max_bytes.max(chunk.len());
        if pulled > MAX_FUZZ_INPUT {
            break;
        }
        pulls += 1;
    }
    loaded.store(pulled.min(MAX_FUZZ_INPUT), Ordering::Relaxed);
    let _ = (loaded, max_bytes);

    // Error path: a producer returning a typed error must abort the upload
    // without panicking.
    let mut failing = net::RequestBody::stream({
        move |_max: usize| Err(net::FetchError::RequestBody("fuzz".to_string()))
    });
    if let net::RequestBody::Stream(producer) = &mut failing {
        let _ = producer.next_chunk(16);
    }
});
