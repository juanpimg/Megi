//! Shared helpers for `paint` integration tests.
//!
//! This module is included by several test binaries (`png_format.rs`,
//! `end_to_end_html.rs`); it is not a test target itself.

#![allow(dead_code)]

/// The 8-byte PNG signature.
pub const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// CRC-32 (IEEE 802.3) implemented independently of `paint::png`, so the
/// tests do not assert the encoder against itself.
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                0xEDB8_8320 ^ (crc >> 1)
            } else {
                crc >> 1
            };
        }
    }
    crc ^ 0xFFFF_FFFF
}

/// Adler-32 (RFC 1950 §9), independent of the encoder implementation.
pub fn adler32(bytes: &[u8]) -> u32 {
    const MOD: u32 = 65_521;
    let mut a = 1u32;
    let mut b = 0u32;
    for &byte in bytes {
        a = (a + u32::from(byte)) % MOD;
        b = (b + a) % MOD;
    }
    (b << 16) | a
}

/// A parsed PNG file: its `(type, payload)` chunks and the inflated image data.
pub struct ParsedPng {
    pub chunks: Vec<(String, Vec<u8>)>,
    pub raw: Vec<u8>,
}

impl ParsedPng {
    /// Returns the payload of the first chunk of the given type.
    pub fn chunk(&self, kind: &str) -> Option<&[u8]> {
        self.chunks
            .iter()
            .find(|(name, _)| name == kind)
            .map(|(_, payload)| payload.as_slice())
    }

    /// Returns the ordered chunk type names.
    pub fn chunk_names(&self) -> Vec<&str> {
        self.chunks.iter().map(|(name, _)| name.as_str()).collect()
    }
}

/// Parses and structurally validates a PNG stream: signature, chunk CRCs,
/// chunk order (IHDR first, IEND last, IDAT contiguous) and the zlib payload.
pub fn parse_png(png: &[u8]) -> ParsedPng {
    assert!(
        png.starts_with(PNG_SIGNATURE),
        "PNG is missing the 8-byte signature"
    );

    let mut pos = PNG_SIGNATURE.len();
    let mut chunks: Vec<(String, Vec<u8>)> = Vec::new();
    let mut idat = Vec::new();
    loop {
        assert!(pos + 8 <= png.len(), "truncated PNG chunk header");
        let length = u32::from_be_bytes(png[pos..pos + 4].try_into().unwrap()) as usize;
        let kind_bytes: [u8; 4] = png[pos + 4..pos + 8].try_into().unwrap();
        let payload_start = pos + 8;
        let payload_end = payload_start + length;
        assert!(payload_end + 4 <= png.len(), "truncated PNG chunk payload");
        let payload = &png[payload_start..payload_end];
        let stored_crc = u32::from_be_bytes(png[payload_end..payload_end + 4].try_into().unwrap());

        let mut crc_input = Vec::with_capacity(4 + payload.len());
        crc_input.extend_from_slice(&kind_bytes);
        crc_input.extend_from_slice(payload);
        assert_eq!(
            stored_crc,
            crc32(&crc_input),
            "bad CRC in chunk {:?}",
            String::from_utf8_lossy(&kind_bytes)
        );

        let kind = String::from_utf8_lossy(&kind_bytes).into_owned();
        if kind == "IDAT" {
            idat.extend_from_slice(payload);
        }
        chunks.push((kind.clone(), payload.to_vec()));
        pos = payload_end + 4;
        if kind == "IEND" {
            break;
        }
    }
    assert_eq!(pos, png.len(), "trailing bytes after IEND");

    let names: Vec<&str> = chunks.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names.first(), Some(&"IHDR"), "IHDR must be first");
    assert_eq!(names.last(), Some(&"IEND"), "IEND must be last");
    assert_eq!(
        names.iter().filter(|name| **name == "IHDR").count(),
        1,
        "exactly one IHDR expected"
    );
    let first_idat = names.iter().position(|name| *name == "IDAT").expect("IDAT");
    let last_idat = names
        .iter()
        .rposition(|name| *name == "IDAT")
        .expect("IDAT");
    assert_eq!(
        last_idat - first_idat + 1,
        names.iter().filter(|name| **name == "IDAT").count(),
        "IDAT chunks must be contiguous"
    );

    let raw = inflate_stored_zlib(&idat);
    ParsedPng { chunks, raw }
}

/// Minimal zlib/DEFLATE decompressor for the stored-block subset produced by
/// the dependency-free encoder in `paint::png`.
pub fn inflate_stored_zlib(zlib: &[u8]) -> Vec<u8> {
    assert!(zlib.len() >= 6, "zlib stream too short");
    assert_eq!(zlib[0] & 0x0F, 8, "CM must be 8 (deflate)");
    assert_eq!(
        (u16::from(zlib[0]) << 8 | u16::from(zlib[1])) % 31,
        0,
        "zlib FCHECK must make the header a multiple of 31"
    );

    let mut pos = 2;
    let mut out = Vec::new();
    loop {
        assert!(pos < zlib.len(), "missing stored DEFLATE block header");
        let header = zlib[pos];
        assert_eq!(header & 0b110, 0, "expected a stored DEFLATE block");
        pos += 1;
        let len = u16::from_le_bytes(zlib[pos..pos + 2].try_into().unwrap());
        let nlen = u16::from_le_bytes(zlib[pos + 2..pos + 4].try_into().unwrap());
        assert_eq!(len, !nlen, "stored block LEN/NLEN mismatch");
        pos += 4;
        out.extend_from_slice(&zlib[pos..pos + len as usize]);
        pos += len as usize;
        if header & 1 == 1 {
            break;
        }
    }
    let stored_adler = u32::from_be_bytes(zlib[pos..pos + 4].try_into().unwrap());
    assert_eq!(stored_adler, adler32(&out), "zlib Adler-32 mismatch");
    out
}

/// Reads a big-endian `u32` at `offset` of `bytes`.
pub fn read_u32_be(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

/// Extracts the IHDR fields `(width, height, bit_depth, color_type)`.
pub fn ihdr_fields(parsed: &ParsedPng) -> (u32, u32, u8, u8) {
    let ihdr = parsed.chunk("IHDR").expect("IHDR chunk");
    assert_eq!(ihdr.len(), 13, "IHDR payload must be 13 bytes");
    (read_u32_be(ihdr, 0), read_u32_be(ihdr, 4), ihdr[8], ihdr[9])
}

/// Converts the inflated scanlines of an 8-bit RGBA PNG into raw RGBA bytes,
/// asserting that every scanline uses filter type 0.
pub fn rgba_from_scanlines(raw: &[u8], width: u32, height: u32) -> Vec<u8> {
    let stride = width as usize * 4;
    assert_eq!(
        raw.len(),
        height as usize * (stride + 1),
        "unexpected inflated image size"
    );
    let mut rgba = Vec::with_capacity(stride * height as usize);
    for row in raw.chunks(stride + 1) {
        assert_eq!(row[0], 0, "expected filter type 0 (None)");
        rgba.extend_from_slice(&row[1..]);
    }
    rgba
}
