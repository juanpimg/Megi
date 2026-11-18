use super::*;

#[test]
fn detects_known_signatures() {
    assert_eq!(detect_format(b"\x89PNG\r\n\x1a\nrest"), ImageFormat::Png);
    assert_eq!(detect_format(b"GIF89a...."), ImageFormat::Gif);
    assert_eq!(detect_format(b"GIF87a...."), ImageFormat::Gif);
    assert_eq!(detect_format(&[0xFF, 0xD8, 0xFF, 0xE0]), ImageFormat::Jpeg);
    assert_eq!(detect_format(b"RIFF\0\0\0\0WEBPVP8 "), ImageFormat::WebP);
    assert_eq!(detect_format(b"not an image"), ImageFormat::Unknown);
    assert_eq!(detect_format(b""), ImageFormat::Unknown);
}

#[test]
fn rgba_image_accessors_are_bounds_checked() {
    let image = RgbaImage::new(2, 2, vec![0; 16]).unwrap();
    assert_eq!(image.dimensions(), (2, 2));
    assert!(!image.is_empty());
    assert_eq!(image.pixel(1, 1), Some([0, 0, 0, 0]));
    assert_eq!(image.pixel(2, 0), None);
    assert_eq!(image.pixel(0, 2), None);
    assert_eq!(image.row(1).map(<[u8]>::len), Some(8));
    assert_eq!(image.row(2), None);
    assert_eq!(image.len_bytes(), 16);
    assert!(RgbaImage::new(2, 2, vec![0; 15]).is_err());
    assert!(RgbaImage::new(0, 2, Vec::new()).is_err());
}

#[test]
fn unsupported_and_truncated_formats_are_typed_errors() {
    // GIF/JPEG are decoded now: malformed headers are data/truncation errors.
    assert!(matches!(
        decode(b"GIF89a....").unwrap_err(),
        ImageError::Truncated { .. } | ImageError::InvalidData { .. }
    ));
    assert!(matches!(
        decode(&[0xFF, 0xD8, 0xFF, 0xE0]).unwrap_err(),
        ImageError::Truncated { .. } | ImageError::InvalidData { .. }
    ));
    // WebP chunks are parsed now: an empty/truncated container is a typed data error
    // (a complete VP8 lossy chunk decodes to pixels instead).
    assert!(matches!(
        decode(b"RIFF\0\0\0\0WEBPVP8 ").unwrap_err(),
        ImageError::InvalidData { .. } | ImageError::Truncated { .. }
    ));
    assert_eq!(
        decode(b"garbage").unwrap_err(),
        ImageError::InvalidSignature
    );
}
