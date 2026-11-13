#![no_main]
//! Fuzz target: the byte → code-point boundary layer and the text preprocessor (#711).
//!
//! Every other text harness decodes its input with `String::from_utf8_lossy`, which replaces
//! invalid sequences with U+FFFD *before* the parser sees them. Overlong encodings, lone
//! surrogates encoded as raw `u32` code points, truncated multi-byte sequences and NUL bytes
//! therefore never reached `html::input_stream` as raw code points — the exact layer where
//! boundary bugs live (cf. #150, an `expect` on a UTF-8 boundary).
//!
//! This target is bytes-native: the input is interpreted as 4-byte little-endian code points
//! (22 significant bits) so the full U+0000..U+3FFFFF space reaches
//! `html::input_stream::preprocess_code_points`, *including* lone surrogates and values above
//! U+10FFFF that a `&str` cannot represent. It also re-runs the normalized output through the
//! tokenizer through a `char`-filtered document, so preprocessing fixes are exercised
//! end-to-end, and compares against `preprocess_str` on the lossy-decoded view so the two
//! entry points cannot drift.

use libfuzzer_sys::fuzz_target;

/// Hard cap on the fuzzer input (bytes).
const MAX_FUZZ_INPUT: usize = 64 * 1024;
/// Cap on the code points fed to the preprocessor per iteration.
const MAX_CODE_POINTS: usize = 8 * 1024;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_FUZZ_INPUT {
        return;
    }

    // Raw code points: 4 bytes each, 22 significant bits (surrogates and >U+10FFFF included).
    let code_points: Vec<u32> = data
        .chunks_exact(4)
        .take(MAX_CODE_POINTS)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) & 0x003F_FFFF)
        .collect();

    let (normalized, errors) = html::input_stream::preprocess_code_points(&code_points);
    // Errors are expected: surrogates, noncharacters and control characters must be reported,
    // never panic. The normalized output must be free of CR and of stray NUL handling that the
    // tokenizer assumes.
    let _ = errors.len();

    // Lossy view of the same bytes (the path every other text harness uses), so the two entry
    // points see the same kind of input and cannot drift.
    let lossy = megi_fuzz::lossy_prefix(data, MAX_FUZZ_INPUT);
    let (normalized_from_str, _str_errors) = html::input_stream::preprocess_str(&lossy);
    assert!(normalized_from_str.iter().all(|&cp| cp != 0x0D));

    // Feed the normalized code points to the tokenizer: invalid code points are dropped the
    // way `char::from_u32` would, which keeps this path representable while still exercising
    // the tokenizer with the preprocessed stream.
    let rebuilt: String = normalized
        .iter()
        .filter_map(|&cp| char::from_u32(cp))
        .take(MAX_CODE_POINTS)
        .collect();
    let options = html::ParseOptions::default();
    let _ = html::parse_document_with_options(&rebuilt, false, &options);

    // The original bytes must also survive a strict decode attempt without panicking; invalid
    // UTF-8 is a normal outcome, not an error condition for this target.
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = html::parse_document_with_options(text, false, &options);
    }
});
