//! EXIF metadata subset: image orientation (`IFD0` tag `0x0112`, values 1–8).
//!
//! Parses the `APP1` segment (`Exif\0\0` + TIFF header) of a JPEG stream and
//! returns the orientation. The TIFF header may be little (`II`) or big (`MM`)
//! endian; only `IFD0` is walked (no sub-IFDs, no thumbnails, no other tags).
//! Any malformed structure yields `None` instead of an error: EXIF is optional
//! metadata and a bad segment must not make an otherwise valid image fail.

/// Returns the EXIF orientation (1–8) of a JPEG byte stream, if present.
///
/// `None` means "no usable APP1/EXIF orientation"; callers treat it as 1
/// (no transformation). Values outside 1–8 are ignored as well.
pub fn orientation(bytes: &[u8]) -> Option<u8> {
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
}
