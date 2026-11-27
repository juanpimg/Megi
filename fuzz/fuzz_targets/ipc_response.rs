#![no_main]
//! Fuzz target: untrusted IPC render response JSON decoding and semantic validation.

use libfuzzer_sys::fuzz_target;
use megi_browser::ipc::{
    IpcRenderResponse, MAX_IPC_CSS_DIMENSION, ValidationError, validate_response,
};

/// Baseline generations selected by the fuzzer-controlled `scripts_executed` field.
/// The last two entries exercise the `saturating_add` boundary in `validate_response` (#2418).
const BASELINES: [u64; 6] = [0, 1, 2, 3, u64::MAX - 1, u64::MAX];

fuzz_target!(|data: &[u8]| {
    if data.len() > 256 * 1024 {
        return;
    }
    let Ok(mut response) = serde_json::from_slice::<IpcRenderResponse>(data) else {
        return;
    };

    // A JSON object always starts with `{`, so derive the generation states from
    // mutable numeric fields instead of raw header bytes (#2418).
    let baseline = BASELINES[response.scripts_executed % BASELINES.len()];
    let step = match response.script_errors % 3 {
        0 => 1, // next generation: valid unless the baseline saturates
        1 => 0, // equal generation: stale
        _ => 2, // skipped generation: stale
    };
    let generation = baseline.saturating_add(step);
    response.generation = generation;
    let expected_valid = step == 1 && baseline != u64::MAX;

    match validate_response(response, baseline) {
        Ok(validated) => {
            assert!(
                expected_valid,
                "generation {generation} with baseline {baseline} must be rejected"
            );
            let inner = validated.into_inner();
            assert_eq!(inner.generation, generation, "validated generation changed");
            assert!(
                inner.css_width <= MAX_IPC_CSS_DIMENSION,
                "css_width {} exceeds {}",
                inner.css_width,
                MAX_IPC_CSS_DIMENSION
            );
            assert!(
                inner.content_height <= MAX_IPC_CSS_DIMENSION.saturating_mul(16),
                "content_height {} exceeds the documented bound",
                inner.content_height
            );
            assert!(
                inner.document_html.len() <= 16 * 1024 * 1024,
                "document_html exceeds the documented 16 MiB bound"
            );
            assert!(
                inner.document_url.len() <= 65_536,
                "document_url exceeds the documented 65 KiB bound"
            );
        }
        Err(error) => {
            if expected_valid {
                assert_ne!(
                    error,
                    ValidationError::StaleGeneration,
                    "valid generation {generation} with baseline {baseline} rejected as stale"
                );
            } else {
                assert_eq!(
                    error,
                    ValidationError::StaleGeneration,
                    "generation {generation} with baseline {baseline} returned {error:?}"
                );
            }
        }
    }
});
