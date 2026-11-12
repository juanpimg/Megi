#![no_main]
//! Direct fuzz target for `fonts::FontDatabase` family queries and text shaping (#834).

use fonts::{FontDatabase, FontStyle};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let db = FontDatabase::new_with_bundled_fonts();
    let _ = db.has_family(&text);
    let _ = db.font(&[text.as_ref()], 400, FontStyle::Normal);
});
