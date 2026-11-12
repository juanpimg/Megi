//! EXIF metadata subset: image orientation (`IFD0` tag `0x0112`, values 1–8).
//!
//! Parses the `APP1` segment (`Exif\0\0` + TIFF header) of a JPEG stream and
//! returns the orientation. The TIFF header may be little (`II`) or big (`MM`)
//! endian; only `IFD0` is walked (no sub-IFDs, no thumbnails, no other tags).
//! Any malformed structure yields `None` instead of an error: EXIF is optional
//! metadata and a bad segment must not make an otherwise valid image fail.

/// Maximum embedded EXIF thumbnail size accepted (64 KiB, typical IFD1 thumbnails are
/// 4–16 KiB; this bounds the work/allocation a hostile file can request).
pub const MAX_THUMBNAIL_BYTES: usize = 64 * 1024;

/// Returns the embedded EXIF (IFD1) JPEG thumbnail bytes, if present.
///
/// Only the first `APP1`/`Exif\0\0` segment before `SOS` is considered; the TIFF
/// `IFD1` `JPEGInterchangeFormat` (0x0201) and `JPEGInterchangeFormatLength` (0x0202)
/// tags must be LONG/1 and point inside the segment. Malformed offsets, lengths above
/// [`MAX_THUMBNAIL_BYTES`], non-JPEG payloads or a missing IFD1 return `None` (never a
/// panic). The returned slice borrows `bytes`.
pub fn thumbnail(bytes: &[u8]) -> Option<&[u8]> {
    if !bytes.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut index = 2usize;
    while index + 4 <= bytes.len() {
        if bytes[index] != 0xFF {
            return None;
        }
        let marker = bytes[index + 1];
        index += 2;
        if marker == 0xD9 || marker == 0xDA {
            return None; // EOI / start of scan: EXIF APP1 must precede the scan.
        }
        if (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            continue;
        }
        let length_bytes = bytes.get(index..index + 2)?;
        let length = usize::from(u16::from_be_bytes([length_bytes[0], length_bytes[1]]));
        if length < 2 || index + length > bytes.len() {
            return None;
        }
        let segment = &bytes[index + 2..index + length];
        if marker == 0xE1 && segment.starts_with(b"Exif\0\0") {
            return tiff_thumbnail(&segment[6..]);
        }
        index += length;
    }
    None
}

/// Parses IFD1 of a TIFF/EXIF payload and returns the embedded JPEG thumbnail slice.
fn tiff_thumbnail(tiff: &[u8]) -> Option<&[u8]> {
    let byte_order = tiff.get(..2)?;
    let little = match byte_order {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let read_u16 = |offset: usize| -> Option<u16> {
        let raw = tiff.get(offset..offset + 2)?;
        Some(if little {
            u16::from_le_bytes([raw[0], raw[1]])
        } else {
            u16::from_be_bytes([raw[0], raw[1]])
        })
    };
    let read_u32 = |offset: usize| -> Option<u32> {
        let raw = tiff.get(offset..offset + 4)?;
        Some(if little {
            u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]])
        } else {
            u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]])
        })
    };
    if read_u16(2)? != 42 {
        return None;
    }
    let ifd0 = read_u32(4)? as usize;
    let count0 = read_u16(ifd0)? as usize;
    if count0 > 512 {
        return None;
    }
    // The next-IFD pointer follows the IFD0 entries.
    let next_ifd_offset = ifd0.checked_add(2 + count0.checked_mul(12)?)?;
    let ifd1 = read_u32(next_ifd_offset)? as usize;
    if ifd1 == 0 {
        return None;
    }
    let count1 = read_u16(ifd1)? as usize;
    if count1 > 512 {
        return None;
    }
    let mut thumbnail_offset = None;
    let mut thumbnail_length = None;
    for entry in 0..count1 {
        let offset = ifd1.checked_add(2 + entry.checked_mul(12)?)?;
        let tag = read_u16(offset)?;
        let kind = read_u16(offset + 2)?;
        let quantity = read_u32(offset + 4)?;
        if quantity != 1 {
            continue;
        }
        if kind != 4 {
            continue;
        }
        let value = read_u32(offset + 8)? as usize;
        match tag {
            0x0201 => thumbnail_offset = Some(value),
            0x0202 => thumbnail_length = Some(value),
            _ => {}
        }
    }
    let offset = thumbnail_offset?;
    let length = thumbnail_length?;
    if length == 0 || length > MAX_THUMBNAIL_BYTES {
        return None;
    }
    let end = offset.checked_add(length)?;
    let thumbnail = tiff.get(offset..end)?;
    thumbnail.starts_with(&[0xFF, 0xD8]).then_some(thumbnail)
}

/// Returns the EXIF orientation (1–8) of a JPEG or WebP byte stream, if present.
///
/// `None` means "no usable EXIF orientation"; callers treat it as 1
/// (no transformation). Values outside 1–8 are ignored as well.
pub fn orientation(bytes: &[u8]) -> Option<u8> {
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        let exif = crate::webp::find_exif_chunk(bytes)?;
        let tiff = exif.strip_prefix(b"Exif\0\0").unwrap_or(exif);
        return parse_tiff_orientation(tiff);
    }
    if !bytes.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut index = 2usize;
    while index + 4 <= bytes.len() {
        if bytes[index] != 0xFF {
            // Inside entropy-coded data; EXIF APP1 only appears before SOS, so stop.
            return None;
        }
        let marker = bytes[index + 1];
        index += 2;
        if marker == 0xD9 || marker == 0xDA {
            return None; // EOI / start of scan: no EXIF before the image data.
        }
        if (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            continue; // standalone markers have no length.
        }
        let length_bytes = bytes.get(index..index + 2)?;
        let length = usize::from(u16::from_be_bytes([length_bytes[0], length_bytes[1]]));
        if length < 2 || index + length > bytes.len() {
            return None;
        }
        let segment = &bytes[index + 2..index + length];
        if marker == 0xE1 && segment.starts_with(b"Exif\0\0") {
            return parse_tiff_orientation(&segment[6..]);
        }
        index += length;
    }
    None
}

fn parse_tiff_orientation(tiff: &[u8]) -> Option<u8> {
    let byte_order = tiff.get(..2)?;
    let little = match byte_order {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let read_u16 = |offset: usize| -> Option<u16> {
        let raw = tiff.get(offset..offset + 2)?;
        Some(if little {
            u16::from_le_bytes([raw[0], raw[1]])
        } else {
            u16::from_be_bytes([raw[0], raw[1]])
        })
    };
    let read_u32 = |offset: usize| -> Option<u32> {
        let raw = tiff.get(offset..offset + 4)?;
        Some(if little {
            u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]])
        } else {
            u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]])
        })
    };
    if read_u16(2)? != 42 {
        return None;
    }
    let ifd0 = read_u32(4)? as usize;
    let count = read_u16(ifd0)? as usize;
    if count > 512 {
        return None; // hostile entry count
    }
    for entry in 0..count {
        let offset = ifd0.checked_add(2 + entry * 12)?;
        let tag = read_u16(offset)?;
        if tag != 0x0112 {
            continue;
        }
        let kind = read_u16(offset + 2)?;
        let quantity = read_u32(offset + 4)?;
        let value = read_u16(offset + 8)?;
        // SHORT (3), one value; LONG (4) with count 1 is also accepted for robustness.
        if kind == 3 && quantity >= 1 {
            return (1..=8).contains(&value).then_some(value as u8);
        }
        if kind == 4 && quantity == 1 {
            let long = read_u32(offset + 8)?;
            return u8::try_from(long)
                .ok()
                .filter(|value| (1..=8).contains(value));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::orientation;

    #[test]
    fn rejects_non_jpeg_and_missing_exif() {
        assert_eq!(orientation(b"not a jpeg"), None);
        assert_eq!(orientation(&[0xFF, 0xD8, 0xFF, 0xD9]), None);
        assert_eq!(orientation(&[0xFF, 0xD8, 0xFF, 0xE1, 0x00]), None);
    }

    #[test]
    fn reads_webp_exif_orientation_chunk() {
        // Minimal TIFF little-endian IFD0 with orientation tag 0x0112 = 6 (Rotate 90 CW).
        let mut tiff = Vec::new();
        tiff.extend_from_slice(b"II");
        tiff.extend_from_slice(&42u16.to_le_bytes());
        tiff.extend_from_slice(&8u32.to_le_bytes());
        tiff.extend_from_slice(&1u16.to_le_bytes()); // 1 IFD entry
        tiff.extend_from_slice(&0x0112u16.to_le_bytes()); // Orientation
        tiff.extend_from_slice(&3u16.to_le_bytes()); // SHORT
        tiff.extend_from_slice(&1u32.to_le_bytes()); // count = 1
        tiff.extend_from_slice(&6u16.to_le_bytes()); // value = 6
        tiff.extend_from_slice(&0u16.to_le_bytes()); // padding

        let mut webp = Vec::new();
        webp.extend_from_slice(b"RIFF");
        let riff_size = (4 + 8 + tiff.len() + (tiff.len() & 1)) as u32;
        webp.extend_from_slice(&riff_size.to_le_bytes());
        webp.extend_from_slice(b"WEBP");
        webp.extend_from_slice(b"EXIF");
        webp.extend_from_slice(&(tiff.len() as u32).to_le_bytes());
        webp.extend_from_slice(&tiff);
        if !tiff.len().is_multiple_of(2) {
            webp.push(0);
        }
        assert_eq!(orientation(&webp), Some(6));
    }
}
