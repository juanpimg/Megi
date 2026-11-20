//! WOFF1 (Web Open Font Format 1.0) → sfnt reconstruction (W3C WOFF §4).
//!
//! WOFF1 wraps an sfnt table directory where each table is stored zlib-compressed (or raw
//! when compression does not help). This module validates every declared length before
//! allocating and inflates tables with the pure-Rust `miniz_oxide` DEFLATE decoder. No `unsafe`.

use crate::FontError;

const WOFF1_HEADER_LEN: usize = 44;
const WOFF1_TABLE_ENTRY_LEN: usize = 20;
const MAX_WOFF1_TABLES: usize = 512;
const MAX_WOFF1_BYTES: usize = 64 * 1024 * 1024;

fn invalid_woff1(reason: &str) -> FontError {
    FontError::InvalidWebFont {
        reason: reason.to_owned(),
    }
}

/// Returns whether `bytes` start with the WOFF1 magic `wOFF`.
pub fn is_woff1(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && &bytes[..4] == b"wOFF"
}

fn u16be(bytes: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([bytes[offset], bytes[offset + 1]])
}

fn u32be(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

/// Decodes WOFF1 bytes into the wrapped sfnt (TTF/OTF) byte stream.
pub fn decode_woff1(bytes: &[u8]) -> Result<Vec<u8>, FontError> {
    if bytes.len() < WOFF1_HEADER_LEN {
        return Err(invalid_woff1("WOFF1 header truncated"));
    }
    if bytes.len() > MAX_WOFF1_BYTES {
        return Err(invalid_woff1("WOFF1 container too large"));
    }
    let flavor = u32be(bytes, 4);
    if !matches!(flavor, 0x0001_0000 | 0x4F54_5400 | 0x7472_7565) {
        return Err(invalid_woff1("WOFF1 has an unknown sfnt flavor"));
    }
    let total_woff = u32be(bytes, 8) as usize;
    let num_tables = u16be(bytes, 12) as usize;
    if num_tables == 0 || num_tables > MAX_WOFF1_TABLES {
        return Err(invalid_woff1("WOFF1 table count out of range"));
    }
    if total_woff != bytes.len() {
        return Err(invalid_woff1("WOFF1 length does not match input"));
    }
    let total_sfnt = u32be(bytes, 16) as usize;
    if total_sfnt < 12 + 16 * num_tables || total_sfnt > MAX_WOFF1_BYTES {
        return Err(invalid_woff1("WOFF1 totalSfntSize out of range"));
    }
    let directory_end = WOFF1_HEADER_LEN
        .checked_add(
            num_tables
                .checked_mul(WOFF1_TABLE_ENTRY_LEN)
                .ok_or_else(|| invalid_woff1("WOFF1 directory overflow"))?,
        )
        .ok_or_else(|| invalid_woff1("WOFF1 directory overflow"))?;
    if directory_end > bytes.len() {
        return Err(invalid_woff1("WOFF1 table directory truncated"));
    }

    struct Entry {
        tag: [u8; 4],
        checksum: u32,
        orig_len: usize,
        data: Vec<u8>,
    }
    let mut entries = Vec::with_capacity(num_tables);
    for index in 0..num_tables {
        let base = WOFF1_HEADER_LEN + index * WOFF1_TABLE_ENTRY_LEN;
        let tag = [
            bytes[base],
            bytes[base + 1],
            bytes[base + 2],
            bytes[base + 3],
        ];
        let offset = u32be(bytes, base + 4) as usize;
        let comp_len = u32be(bytes, base + 8) as usize;
        let orig_len = u32be(bytes, base + 12) as usize;
        let checksum = u32be(bytes, base + 16);
        if orig_len == 0
            || comp_len == 0
            || offset < directory_end
            || offset
                .checked_add(comp_len)
                .is_none_or(|end| end > bytes.len())
        {
            return Err(invalid_woff1("WOFF1 table range out of bounds"));
        }
        let stored = &bytes[offset..offset + comp_len];
        let data = if comp_len == orig_len {
            stored.to_vec()
        } else {
            if comp_len > orig_len {
                return Err(invalid_woff1("WOFF1 compressed table longer than original"));
            }
            let inflated =
                miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(stored, orig_len)
                    .map_err(|_| invalid_woff1("WOFF1 table inflation failed"))?;
            if inflated.len() != orig_len {
                return Err(invalid_woff1("WOFF1 table has wrong inflated length"));
            }
            inflated
        };
        entries.push(Entry {
            tag,
            checksum,
            orig_len,
            data,
        });
    }

    // Reconstruct the sfnt: 12-byte header, 16-byte records sorted by tag, padded table data.
    let mut out = Vec::with_capacity(total_sfnt);
    out.extend_from_slice(&flavor.to_be_bytes());
    out.extend_from_slice(&(num_tables as u16).to_be_bytes());
    // searchRange / entrySelector / rangeShift for numTables (CSS uses these conservatively).
    let mut entry_selector = 0u16;
    while (1u16 << (entry_selector + 1)) <= num_tables as u16 {
        entry_selector += 1;
    }
    let search_range = (1u16 << entry_selector) * 16;
    out.extend_from_slice(&search_range.to_be_bytes());
    out.extend_from_slice(&entry_selector.to_be_bytes());
    out.extend_from_slice(&((num_tables as u16) * 16 - search_range).to_be_bytes());

    entries.sort_by_key(|entry| entry.tag);
    let mut data_offset = 12 + 16 * num_tables;
    for entry in &entries {
        out.extend_from_slice(&entry.tag);
        out.extend_from_slice(&entry.checksum.to_be_bytes());
        out.extend_from_slice(&(data_offset as u32).to_be_bytes());
        out.extend_from_slice(&(entry.orig_len as u32).to_be_bytes());
        data_offset += entry.orig_len.div_ceil(4) * 4;
    }
    for entry in &entries {
        out.extend_from_slice(&entry.data);
        let padding = entry.orig_len.div_ceil(4) * 4 - entry.orig_len;
        out.extend(std::iter::repeat_n(0u8, padding));
    }
    if out.len() != total_sfnt {
        return Err(invalid_woff1("WOFF1 reconstructed size mismatch"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_truncated_and_bogus_headers() {
        assert!(!is_woff1(b"\x00\x01\x00\x00"));
        assert!(decode_woff1(b"wOFF").is_err());
        let mut bad = vec![0u8; WOFF1_HEADER_LEN];
        bad[..4].copy_from_slice(b"wOFF");
        assert!(decode_woff1(&bad).is_err());
    }
}
