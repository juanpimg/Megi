use super::testing::{deflate_stored, gzip_bomb_zeros_1mib, gzip_stored, zlib_stored};
use super::*;

fn limits() -> CompressionLimits {
    CompressionLimits::default()
}

#[test]
fn content_encoding_parses_supported_codings() {
    assert!(matches!(
        ContentEncoding::parse("gzip"),
        Ok(ContentEncoding::Gzip)
    ));
    assert!(matches!(
        ContentEncoding::parse(" GZIP "),
        Ok(ContentEncoding::Gzip)
    ));
    assert!(matches!(
        ContentEncoding::parse("x-gzip"),
        Ok(ContentEncoding::Gzip)
    ));
    assert!(matches!(
        ContentEncoding::parse("deflate"),
        Ok(ContentEncoding::Deflate)
    ));
    assert!(matches!(
        ContentEncoding::parse("identity"),
        Ok(ContentEncoding::Identity)
    ));
    assert!(matches!(
        ContentEncoding::parse(""),
        Ok(ContentEncoding::Identity)
    ));
    assert!(matches!(
        ContentEncoding::parse("br"),
        Ok(ContentEncoding::Brotli)
    ));
    assert!(matches!(
        ContentEncoding::parse("gzip, br"),
        Err(FetchError::ContentEncoding(reason)) if reason.contains("stacked")
    ));
}

#[test]
fn accept_encoding_header_modes() {
    assert_eq!(
        AcceptEncoding::Gzip.header_value(),
        Some("gzip, deflate, br")
    );
    assert_eq!(AcceptEncoding::Identity.header_value(), None);
    assert_eq!(AcceptEncoding::default(), AcceptEncoding::Gzip);
}

#[test]
fn crc32_matches_known_vectors() {
    assert_eq!(crc32_ieee(b""), 0);
    assert_eq!(crc32_ieee(b"123456789"), 0xCBF4_3926);
    assert_eq!(
        crc32_ieee(b"The quick brown fox jumps over the lazy dog"),
        0x414F_A339
    );
}

#[test]
fn gzip_roundtrip_decodes_and_checks_limits() {
    let data = b"<!DOCTYPE html><html><body>hello gzip</body></html>";
    let compressed = gzip_stored(data);
    assert_eq!(decode_gzip(&compressed, limits()).unwrap(), data);
}

#[test]
fn gzip_identity_is_passthrough() {
    let data = b"plain";
    assert_eq!(
        decode_body(ContentEncoding::Identity, data, limits()).unwrap(),
        data
    );
}

#[test]
fn gzip_truncated_stream_is_rejected() {
    let compressed = gzip_stored(b"data");
    for cut in [0usize, 1, 2, 5, 10, compressed.len() - 1] {
        let error = decode_gzip(&compressed[..cut], limits()).unwrap_err();
        assert!(matches!(error, FetchError::Decompression(_)), "{error}");
    }
}

#[test]
fn gzip_bad_magic_is_rejected() {
    let mut compressed = gzip_stored(b"data");
    compressed[1] = 0x00;
    assert!(matches!(
        decode_gzip(&compressed, limits()),
        Err(FetchError::Decompression(reason)) if reason.contains("magic")
    ));
}

#[test]
fn gzip_unsupported_method_is_rejected() {
    let mut compressed = gzip_stored(b"data");
    compressed[2] = 7;
    assert!(matches!(
        decode_gzip(&compressed, limits()),
        Err(FetchError::ContentEncoding(reason)) if reason.contains("method")
    ));
}

#[test]
fn gzip_reserved_flag_bits_are_rejected() {
    let mut compressed = gzip_stored(b"data");
    compressed[3] = 0xE0;
    assert!(matches!(
        decode_gzip(&compressed, limits()),
        Err(FetchError::Decompression(reason)) if reason.contains("reserved")
    ));
}

#[test]
fn gzip_optional_header_fields_are_skipped() {
    let data = b"with optional fields";
    let mut compressed = vec![0x1F, 0x8B, 0x08, 0x1C, 0, 0, 0, 0, 0x00, 0xFF];
    compressed.extend_from_slice(&3u16.to_le_bytes());
    compressed.extend_from_slice(b"abc"); // FEXTRA
    compressed.extend_from_slice(b"name\0"); // FNAME
    compressed.extend_from_slice(b"comment\0"); // FCOMMENT
    compressed.extend_from_slice(&deflate_stored(data));
    compressed.extend_from_slice(&crc32_ieee(data).to_le_bytes());
    compressed.extend_from_slice(&(data.len() as u32).to_le_bytes());
    assert_eq!(decode_gzip(&compressed, limits()).unwrap(), data);
}

#[test]
fn gzip_truncated_optional_field_is_rejected() {
    let mut compressed = vec![0x1F, 0x8B, 0x08, 0x08, 0, 0, 0, 0, 0x00, 0xFF];
    compressed.extend_from_slice(b"unterminated");
    assert!(matches!(
        decode_gzip(&compressed, limits()),
        Err(FetchError::Decompression(reason)) if reason.contains("FNAME")
    ));
}

#[test]
fn gzip_corrupt_crc_is_rejected() {
    let mut compressed = gzip_stored(b"checksum me");
    let crc_offset = compressed.len() - 8;
    compressed[crc_offset] ^= 0xFF;
    assert!(matches!(
        decode_gzip(&compressed, limits()),
        Err(FetchError::Decompression(reason)) if reason.contains("CRC-32")
    ));
}

#[test]
fn gzip_corrupt_isize_is_rejected() {
    let mut compressed = gzip_stored(b"size me");
    let isize_offset = compressed.len() - 4;
    compressed[isize_offset] ^= 0x01;
    assert!(matches!(
        decode_gzip(&compressed, limits()),
        Err(FetchError::Decompression(reason)) if reason.contains("ISIZE")
    ));
}

#[test]
fn deflate_accepts_zlib_and_raw_streams() {
    let data = b"deflate body";
    assert_eq!(decode_deflate(&zlib_stored(data), limits()).unwrap(), data);
    assert_eq!(
        decode_deflate(&deflate_stored(data), limits()).unwrap(),
        data
    );
}

#[test]
fn deflate_invalid_stream_is_rejected() {
    assert!(matches!(
        decode_deflate(&[0xFF; 8], limits()),
        Err(FetchError::Decompression(_))
    ));
}

#[test]
fn output_cap_is_enforced_with_typed_error() {
    let data = vec![b'a'; 2048];
    let compressed = gzip_stored(&data);
    let tight = CompressionLimits::new(1024, DEFAULT_MAX_COMPRESSION_RATIO);
    assert!(matches!(
        decode_gzip(&compressed, tight),
        Err(FetchError::DecompressionLimit {
            max_output: 1024,
            ..
        })
    ));
    assert!(matches!(
        decode_deflate(&zlib_stored(&data), tight),
        Err(FetchError::DecompressionLimit { .. })
    ));
}

#[test]
fn zero_output_cap_rejects_everything_non_empty() {
    let compressed = gzip_stored(b"x");
    let tight = CompressionLimits::new(0, DEFAULT_MAX_COMPRESSION_RATIO);
    assert!(matches!(
        decode_gzip(&compressed, tight),
        Err(FetchError::DecompressionLimit { max_output: 0, .. })
    ));
}

#[test]
fn ratio_cap_rejects_high_ratio_bomb() {
    let bomb = gzip_bomb_zeros_1mib();
    // The decompressed size (1 MiB) is under the 64 MiB default cap, so the ratio
    // cap (1000:1) must not reject it by default...
    let output = decode_gzip(&bomb, limits()).expect("1 MiB bomb survives default caps");
    assert_eq!(output.len(), 1_048_576);
    // ...but a tighter ratio cap rejects it with the typed error.
    let tight = CompressionLimits::new(DEFAULT_MAX_DECOMPRESSED_BYTES, 100);
    assert!(matches!(
        decode_gzip(&bomb, tight),
        Err(FetchError::DecompressionLimit { max_ratio: 100, .. })
    ));
}

#[test]
fn absolute_cap_rejects_bomb_larger_than_limit() {
    let bomb = gzip_bomb_zeros_1mib();
    let tight = CompressionLimits::new(64 * 1024, DEFAULT_MAX_COMPRESSION_RATIO);
    assert!(matches!(
        decode_gzip(&bomb, tight),
        Err(FetchError::DecompressionLimit {
            max_output: 65536,
            ..
        })
    ));
}

#[test]
fn limits_display_is_documented() {
    assert_eq!(
        CompressionLimits::new(64, 10).to_string(),
        "max_output=64 bytes, max_ratio=10:1"
    );
}

#[test]
fn decode_body_dispatches_by_encoding() {
    let data = b"dispatch";
    assert_eq!(
        decode_body(ContentEncoding::Gzip, &gzip_stored(data), limits()).unwrap(),
        data
    );
    assert_eq!(
        decode_body(ContentEncoding::Deflate, &zlib_stored(data), limits()).unwrap(),
        data
    );
}

#[test]
fn brotli_content_encoding_is_supported() {
    // #1918: real-world CDN JS bundles use `Content-Encoding: br`.
    assert!(matches!(
        ContentEncoding::parse("br"),
        Ok(ContentEncoding::Brotli)
    ));
    assert!(matches!(
        ContentEncoding::parse(" BR "),
        Ok(ContentEncoding::Brotli)
    ));
    assert_eq!(ContentEncoding::Brotli.as_str(), "br");
}

#[test]
fn brotli_roundtrip_decodes_a_real_stream() {
    const HTML: &[u8] = b"<!DOCTYPE html><html><body>hello brotli</body></html>";
    let compressed = brotli_compress(HTML);
    assert_ne!(
        compressed, HTML,
        "fixture must actually be Brotli-compressed"
    );
    assert_eq!(decode_brotli(&compressed, limits()).unwrap(), HTML);
    assert_eq!(
        decode_body(ContentEncoding::Brotli, &compressed, limits()).unwrap(),
        HTML
    );
}

#[test]
fn brotli_output_cap_is_enforced_with_typed_error() {
    const ZEROS: &[u8] = &[0u8; 16 * 1024];
    let compressed = brotli_compress(ZEROS);
    let tight = CompressionLimits::new(1024, 1000);
    assert!(matches!(
        decode_brotli(&compressed, tight),
        Err(FetchError::DecompressionLimit { .. })
    ));
}

/// Test-only Brotli compressor (RFC 7932) so the fixture is a genuine `br` stream.
fn brotli_compress(data: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut compressed = Vec::new();
    {
        let mut writer = brotli::CompressorWriter::new(&mut compressed, 4096, 5, 22);
        writer.write_all(data).expect("write brotli");
    }
    compressed
}
