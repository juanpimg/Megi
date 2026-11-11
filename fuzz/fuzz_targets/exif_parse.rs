#![no_main]
//! Fuzz target: EXIF `APP1` orientation parser on arbitrary JPEG-ish bytes.
//!
//! [`image::exif::orientation`] walks JPEG marker segments up to `SOS`/`EOI`
//! looking for an `Exif\0\0` APP1 payload and then parses the TIFF header/IFD0
//! orientation tag (little/big endian). Malformed structures must return `None`
//! (metadata never makes a valid image fail), so the target looks for panics,
//! aborts, ASan/UBSan findings, integer overflows and timeouts only.
//!
//! Input is capped at 64 KiB: EXIF payloads are far smaller and this keeps each
//! iteration cheap. Seeds are the crate's `exif_o1..o8.jpg` fixtures.

use libfuzzer_sys::fuzz_target;

const MAX_FUZZ_INPUT: usize = 64 * 1024;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_FUZZ_INPUT {
        return;
    }
    let _ = image::exif::orientation(data);
});
