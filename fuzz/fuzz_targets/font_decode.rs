#![no_main]
//! Fuzz target: fonts crate (WOFF2/sfnt decode, load_font_data).

use megi_fonts::FontDatabase;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // WOFF2 / web font decode.
    let _ = megi_fonts::web::decode_web_font(data);

    // Load arbitrary bytes as a font face (must not panic).
    let mut db = FontDatabase::new();
    let _ = db.load_font_data(data.to_vec());
    let _ = db.load_font_data_family("fuzz", data.to_vec());
});
