//! WOFF2 decoding + `@font-face` family alias registration tests.
//!
//! Fixture: Open Sans Regular latin subset (SIL Open Font License 1.1) downloaded from
//! Google Fonts; see `tests/fixtures/OFL-OpenSans.txt`.

use fonts::{FontDatabase, FontError, FontStyle, decode_web_font, is_woff2};

const WOFF2: &[u8] = include_bytes!("fixtures/OpenSans-Regular-latin.woff2");

#[test]
fn decodes_real_ofl_woff2_into_sfnt() {
    assert!(is_woff2(WOFF2), "fixture must be a WOFF2 container");
    let ttf = decode_web_font(WOFF2).expect("decoded woff2");
    assert!(!is_woff2(&ttf));
    assert_eq!(&ttf[..4], b"\x00\x01\x00\x00", "sfnt version");
    let face = ttf_parser::Face::parse(&ttf, 0).expect("valid decoded sfnt");
    assert!(face.number_of_glyphs() > 10);
    assert!(face.units_per_em() > 0);
    // Reconstructed tables: glyf/loca/hmtx/cmap must be usable after decoding.
    let tables = face.tables();
    assert!(tables.glyf.is_some(), "glyf reconstructed");
    assert!(tables.hmtx.is_some(), "hmtx reconstructed");
    assert!(tables.cmap.is_some(), "cmap present");
    let a = face.glyph_index('A').expect("glyph for A");
    assert!(face.glyph_hor_advance(a).expect("advance") > 0);
}

#[test]
fn registers_under_css_family_alias_and_resolves_metrics() {
    let mut database = FontDatabase::new_with_bundled_fonts();
    let ttf = decode_web_font(WOFF2).expect("decoded");
    database
        .load_font_data_family("Open Sans Web", ttf)
        .expect("alias registered");
    assert!(database.has_family("Open Sans Web"));
    assert!(database.has_alias("Open Sans Web"));
    let font = database
        .font(&["Open Sans Web"], 400, FontStyle::Normal)
        .expect("alias resolves");
    assert!(font.info().weight > 0);
    let metrics = database
        .metrics(&["Open Sans Web"], 400, FontStyle::Normal, 32.0)
        .expect("metrics");
    assert!(metrics.ascent > 0.0 && metrics.descent > 0.0);
    let width = database
        .measure_text(&["Open Sans Web"], 400, FontStyle::Normal, 32.0, "Hello")
        .expect("width");
    assert!(width > 10.0, "width={width}");
    // Fallback list: alias first, then bundled family.
    assert!(
        database
            .measure_text(
                &["Missing", "Open Sans Web"],
                400,
                FontStyle::Normal,
                16.0,
                "Hi"
            )
            .is_ok()
    );
}

#[test]
fn corrupt_woff2_returns_typed_error() {
    let mut corrupt = WOFF2.to_vec();
    let mid = corrupt.len() / 2;
    let end = (mid + 64).min(corrupt.len());
    for byte in &mut corrupt[mid..end] {
        *byte ^= 0xFF;
    }
    let error = decode_web_font(&corrupt).expect_err("corrupt woff2 must fail");
    assert!(
        matches!(error, fonts::FontError::InvalidWebFont { .. }),
        "{error:?}"
    );
    let mut database = FontDatabase::new();
    let error = database
        .load_font_data_family("Corrupt", corrupt)
        .expect_err("corrupt woff2 cannot be registered");
    assert!(
        matches!(error, fonts::FontError::InvalidFont { .. }),
        "{error:?}"
    );
}

#[test]
fn non_woff2_bytes_pass_through_and_empty_input_fails() {
    let ttf = decode_web_font(WOFF2).expect("decoded");
    assert_eq!(decode_web_font(&ttf).expect("passthrough"), ttf);
    assert!(matches!(
        decode_web_font(&[]),
        Err(fonts::FontError::InvalidWebFont { .. })
    ));
}

#[test]
fn rejects_a_table_directory_length_past_the_compressed_stream() {
    // Bumping the last directory entry's `origLength` by one used to panic inside the
    // third-party decoder (`range end index 31408 out of range for slice of length 31407`);
    // the directory sums stay under the cap, so this is specifically the `catch_unwind`
    // regression (the crafted tests below exercise the directory validation itself).
    let mut corrupted = WOFF2.to_vec();
    corrupted[103] = corrupted[103].wrapping_add(1);
    let error = decode_web_font(&corrupted).expect_err("corrupted directory length");
    assert!(
        matches!(error, FontError::InvalidWebFont { .. }),
        "{error:?}"
    );
}

#[test]
fn rejects_an_oversized_total_sfnt_size_without_allocating() {
    // `Vec::with_capacity(totalSfntSize)` used to request 4 GiB from an 18 KB file.
    let mut corrupted = WOFF2.to_vec();
    corrupted[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
    let error = decode_web_font(&corrupted).expect_err("4 GiB totalSfntSize");
    assert!(
        matches!(error, FontError::InvalidWebFont { .. }),
        "{error:?}"
    );
    // The untouched fixture still decodes.
    assert!(decode_web_font(WOFF2).is_ok());
}

fn crafted_woff2(num_tables: u16, total_sfnt: u32, directory: &[u8], compressed: &[u8]) -> Vec<u8> {
    let total = 48 + directory.len() + compressed.len();
    let mut bytes = Vec::with_capacity(total);
    bytes.extend_from_slice(b"wOF2");
    bytes.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // TrueType flavor
    bytes.extend_from_slice(&(total as u32).to_be_bytes()); // length
    bytes.extend_from_slice(&num_tables.to_be_bytes());
    bytes.extend_from_slice(&0u16.to_be_bytes()); // reserved
    bytes.extend_from_slice(&total_sfnt.to_be_bytes());
    bytes.extend_from_slice(&(compressed.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&[0u8; 24]); // version, meta and private fields
    bytes.extend_from_slice(directory);
    bytes.extend_from_slice(compressed);
    bytes
}

#[test]
fn rejects_a_table_directory_length_larger_than_the_sfnt_cap() {
    // 55 bytes: one `cmap` entry whose base-128 origLength is 4,000,000,000. It passed the
    // scalar header checks and used to reach `Vec::with_capacity(4_000_000_000)`.
    let directory = [0x00, 0x8e, 0xf3, 0xac, 0xd0, 0x00];
    let bytes = crafted_woff2(1, 65_536, &directory, &[0x00]);
    let error = decode_web_font(&bytes).expect_err("huge directory length");
    assert!(
        matches!(error, FontError::InvalidWebFont { .. }),
        "{error:?}"
    );
}

#[test]
fn rejects_a_directory_that_overruns_the_input() {
    // `numTables = 1` but the file ends before the directory entry.
    let bytes = crafted_woff2(1, 65_536, &[], &[0x00]);
    let error = decode_web_font(&bytes).expect_err("truncated directory");
    assert!(
        matches!(error, FontError::InvalidWebFont { .. }),
        "{error:?}"
    );
}

#[test]
fn rejects_compressed_data_longer_than_the_input() {
    // A valid-looking directory, but `totalCompressedSize` exceeds the remaining bytes.
    let directory = [0x00, 0x01]; // cmap, origLength 1
    let mut bytes = crafted_woff2(1, 65_536, &directory, &[0x00]);
    bytes[20..24].copy_from_slice(&3u32.to_be_bytes());
    let error = decode_web_font(&bytes).expect_err("compressed data past EOF");
    assert!(
        matches!(error, FontError::InvalidWebFont { .. }),
        "{error:?}"
    );
}

#[test]
fn rejects_woff2_collections() {
    // `ttcf` adds a collection directory after the table directory; validate it or reject
    // it explicitly instead of bounding the compressed stream from the wrong offset.
    let directory = [0x00, 0x01];
    let mut bytes = crafted_woff2(1, 65_536, &directory, &[0x00]);
    bytes[4..8].copy_from_slice(b"ttcf");
    let error = decode_web_font(&bytes).expect_err("WOFF2 collections are rejected");
    assert!(
        matches!(error, FontError::InvalidWebFont { .. }),
        "{error:?}"
    );
}
