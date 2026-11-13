//! PNG chunk-order/validation hardening (tRNS duplicates, ordering, sample range).

use image::{ImageError, decode};

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + data.len());
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
    out
}

fn png(parts: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    for (kind, data) in parts {
        out.extend_from_slice(&chunk(kind, data));
    }
    out
}

/// IHDR payload and concatenated IDAT payload of the 4x3 grayscale fixture.
fn gray_fixture() -> (Vec<u8>, Vec<u8>) {
    let source: &[u8] = include_bytes!("fixtures/png/gray8.png");
    let mut ihdr = Vec::new();
    let mut idat = Vec::new();
    let mut position = 8usize;
    while position + 12 <= source.len() {
        let length =
            u32::from_be_bytes(source[position..position + 4].try_into().unwrap()) as usize;
        let kind = &source[position + 4..position + 8];
        let data = &source[position + 8..position + 8 + length];
        match kind {
            b"IHDR" => ihdr = data.to_vec(),
            b"IDAT" => idat.extend_from_slice(data),
            _ => {}
        }
        position += 12 + length;
    }
    (ihdr, idat)
}

fn decode_or_reason(bytes: &[u8]) -> Result<(), ImageError> {
    decode(bytes).map(|_| ())
}

#[test]
fn valid_trns_still_decodes() {
    let (ihdr, idat) = gray_fixture();
    let data = png(&[
        (b"IHDR", &ihdr),
        (b"tRNS", &[0, 1]),
        (b"IDAT", &idat),
        (b"IEND", &[]),
    ]);
    decode_or_reason(&data).expect("a single tRNS decodes");
}

#[test]
fn duplicate_trns_is_rejected() {
    let (ihdr, idat) = gray_fixture();
    let data = png(&[
        (b"IHDR", &ihdr),
        (b"tRNS", &[0, 1]),
        (b"tRNS", &[0, 2]),
        (b"IDAT", &idat),
        (b"IEND", &[]),
    ]);
    match decode_or_reason(&data) {
        Err(ImageError::InvalidChunk { reason, .. }) => {
            assert!(reason.contains("duplicate tRNS"), "{reason}");
        }
        other => panic!("expected duplicate tRNS rejection, got {other:?}"),
    }
}

#[test]
fn trns_after_idat_is_rejected() {
    let (ihdr, idat) = gray_fixture();
    let data = png(&[
        (b"IHDR", &ihdr),
        (b"IDAT", &idat),
        (b"tRNS", &[0, 1]),
        (b"IEND", &[]),
    ]);
    match decode_or_reason(&data) {
        Err(ImageError::InvalidChunk { reason, .. }) => {
            assert!(reason.contains("tRNS after IDAT"), "{reason}");
        }
        other => panic!("expected ordering rejection, got {other:?}"),
    }
}

#[test]
fn out_of_range_trns_sample_is_rejected() {
    let (ihdr, idat) = gray_fixture();
    let data = png(&[
        (b"IHDR", &ihdr),
        (b"tRNS", &[0x01, 0x2C]), // 300 > 255 for 8-bit samples
        (b"IDAT", &idat),
        (b"IEND", &[]),
    ]);
    match decode_or_reason(&data) {
        Err(ImageError::InvalidChunk { reason, .. }) => {
            assert!(reason.contains("exceeds the bit depth"), "{reason}");
        }
        other => panic!("expected sample-range rejection, got {other:?}"),
    }
}

#[test]
fn trailing_bytes_after_iend_are_rejected() {
    let (ihdr, idat) = gray_fixture();
    let mut data = png(&[(b"IHDR", &ihdr), (b"IDAT", &idat), (b"IEND", &[])]);
    data.extend_from_slice(b"TRAILING-GARBAGE");
    match decode_or_reason(&data) {
        Err(ImageError::InvalidChunk { reason, .. }) => {
            assert!(reason.contains("trailing data"), "{reason}");
        }
        other => panic!("expected trailing-data rejection, got {other:?}"),
    }
}
