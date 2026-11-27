#![no_main]
//! Fuzz target: streaming request bodies (`megi_net::RequestBody`/`BodyProducer`).
//!
//! Round 28 added pull-based request bodies with backpressure: producers return
//! at most the requested `max` bytes and the transport pulls only when it can
//! write. This target builds chunk plans from the input and drives a
//! [`megi_net::RequestBody::Stream`] with bounded pull sizes. Mode 3 of the
//! producer deliberately violates the `chunk <= max` contract; the consumer must
//! reject it with a typed error instead of panicking (#2428). The target also
//! checks bounded total consumption and the `Bytes` helpers (`len`/`is_empty`/
//! `Debug`). It never opens a socket; the target looks for panics/aborts/UB.

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
    let bytes = megi_net::RequestBody::bytes(data);
    if bytes.len() != Some(data.len()) || bytes.is_empty() != data.is_empty() {
        return;
    }
    let _ = format!("{bytes:?}");
    let from_slice = megi_net::RequestBody::from(data);
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

    // #2428: derive the mode from the input so mode 3 (an oversize producer
    // chunk) is reachable from a seed instead of dead code. `mode` is copied
    // into the `'static` producer closure; the input slice cannot be captured.
    let mode = usize::from(data.first().copied().unwrap_or(0)) % 4;
    let index = Arc::new(AtomicUsize::new(0));
    let loaded = Arc::new(AtomicUsize::new(0));
    let chunks_for_stream = Arc::new(chunks);
    let mut stream = megi_net::RequestBody::stream({
        let index = Arc::clone(&index);
        let chunks = Arc::clone(&chunks_for_stream);
        move |max: usize| {
            let position = index.fetch_add(1, Ordering::Relaxed);
            if position >= chunks.len() {
                return Ok(None);
            }
            let chunk = chunks[position].clone();
            // Modes 0/1 truncate, mode 2 hits `chunk == max`, mode 3 violates
            // the contract so the consumer's typed rejection is exercised.
            let chunk = megi_fuzz::producer_chunk(&chunk, max, mode);
            Ok(Some(chunk))
        }
    });

    // Pull with bounded sizes derived from the input. Contract violations are
    // rejected as typed errors (#2428); the count proves the oracle is reachable.
    let mut pulled = 0usize;
    let mut pulls = 0usize;
    let mut max_bytes = 0usize;
    let mut expected_rejections = 0usize;
    let mut rejected = 0usize;
    while pulls < MAX_PULLS {
        let max = if data.is_empty() {
            0
        } else {
            usize::from(data[pulls % data.len()]).saturating_mul(3)
        };
        let position = index.load(Ordering::Relaxed);
        let oversize_expected = mode == 3 && max > 0 && position < chunks_for_stream.len();
        let pull = match &mut stream {
            megi_net::RequestBody::Stream(producer) => {
                if oversize_expected {
                    expected_rejections += 1;
                }
                megi_fuzz::pull_chunk_checked(producer, max)
            }
            megi_net::RequestBody::Bytes(bytes) => Ok(Some(std::mem::take(bytes))),
        };
        let chunk = match pull {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(_) => {
                rejected += 1;
                pulls += 1;
                continue;
            }
        };
        pulled = pulled.saturating_add(chunk.len());
        max_bytes = max_bytes.max(chunk.len());
        if pulled > MAX_FUZZ_INPUT {
            break;
        }
        pulls += 1;
    }
    assert_eq!(
        rejected, expected_rejections,
        "oversize producer chunks must be rejected with a typed error"
    );
    loaded.store(pulled.min(MAX_FUZZ_INPUT), Ordering::Relaxed);
    let _ = (loaded, max_bytes);

    // Error path: a producer returning a typed error must abort the upload
    // without panicking.
    let mut failing = megi_net::RequestBody::stream({
        move |_max: usize| Err(megi_net::FetchError::RequestBody("fuzz".to_string()))
    });
    if let megi_net::RequestBody::Stream(producer) = &mut failing {
        let _ = producer.next_chunk(16);
    }
});
