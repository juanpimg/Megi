//! HPACK integer/string coding, decoder with dynamic table, and minimal encoder.

use std::collections::VecDeque;

use crate::error::FetchError;

use super::huffman::huffman_decode;
use super::{
    DEFAULT_MAX_HEADER_LIST_SIZE, ENTRY_OVERHEAD, MAX_DYNAMIC_TABLE_SIZE, STATIC_TABLE, h2_error,
};

/// Reads an HPACK integer with a `prefix`-bit prefix from `input[*pos..]`.
fn decode_integer(input: &[u8], pos: &mut usize, prefix: u8) -> Result<usize, FetchError> {
    let Some(&first) = input.get(*pos) else {
        return Err(h2_error("truncated HPACK integer"));
    };
    *pos += 1;
    let mask = (1u16 << prefix) as u8;
    let mut value = usize::from(first & (mask.wrapping_sub(1)));
    if value < usize::from(mask.wrapping_sub(1)) {
        return Ok(value);
    }
    let mut shift = 0u32;
    loop {
        let Some(&byte) = input.get(*pos) else {
            return Err(h2_error("truncated HPACK integer"));
        };
        *pos += 1;
        if shift >= 28 {
            return Err(h2_error("HPACK integer overflow"));
        }
        value = value
            .checked_add(usize::from(byte & 0x7f) << shift)
            .ok_or_else(|| h2_error("HPACK integer overflow"))?;
        shift += 7;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
}

/// Reads an HPACK string (Huffman flag + length prefix) into a `String`.
fn decode_string(input: &[u8], pos: &mut usize) -> Result<String, FetchError> {
    let Some(&first) = input.get(*pos) else {
        return Err(h2_error("truncated HPACK string"));
    };
    let huffman = first & 0x80 != 0;
    let length = decode_integer(input, pos, 7)?;
    let end = pos
        .checked_add(length)
        .filter(|&end| end <= input.len())
        .ok_or_else(|| h2_error("HPACK string length exceeds header block"))?;
    let raw = &input[*pos..end];
    *pos = end;
    let bytes = if huffman {
        huffman_decode(raw)?
    } else {
        raw.to_vec()
    };
    String::from_utf8(bytes).map_err(|_| h2_error("HPACK header string is not valid UTF-8"))
}

/// Encodes an integer with a `prefix`-bit prefix (used by the minimal encoder).
fn encode_integer(value: usize, prefix: u8, out: &mut Vec<u8>, first_byte: u8) {
    let mask = (1usize << prefix) - 1;
    if value < mask {
        out.push(first_byte | value as u8);
        return;
    }
    out.push(first_byte | mask as u8);
    let mut remaining = value - mask;
    while remaining >= 128 {
        out.push((remaining & 0x7f) as u8 | 0x80);
        remaining >>= 7;
    }
    out.push(remaining as u8);
}

/// Encodes a raw (non-Huffman) HPACK string.
fn encode_string(value: &str, out: &mut Vec<u8>) {
    encode_integer(value.len(), 7, out, 0);
    out.extend_from_slice(value.as_bytes());
}

/// Looks up an exact `(name, value)` pair in the static table (1-based index).
fn static_index(name: &str, value: &str) -> Option<u64> {
    STATIC_TABLE
        .iter()
        .position(|&(entry_name, entry_value)| entry_name == name && entry_value == value)
        .map(|index| index as u64 + 1)
}

/// Looks up a static-table name for literal field names (1-based index).
fn static_name_index(name: &str) -> Option<usize> {
    STATIC_TABLE
        .iter()
        .position(|&(entry_name, _)| entry_name == name)
        .map(|index| index + 1)
}

/// HPACK decoder with a dynamic table (RFC 7541 §2.3).
pub struct Decoder {
    dynamic: VecDeque<(String, String)>,
    max_table_size: usize,
    /// Ceiling a dynamic table size update may raise the table to: the value this decoder
    /// advertised in `SETTINGS_HEADER_TABLE_SIZE` (RFC 7541 §4.2).
    settings_max_table_size: usize,
    /// Cap for the decoded header list (`name + value + 32` per field).
    max_header_list_size: usize,
    table_size: usize,
    /// Set once a dynamic table size update is seen; further updates are allowed
    /// only before the first header field of the block.
    seen_field: bool,
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new(4096)
    }
}

impl Decoder {
    /// Creates a decoder whose dynamic table can grow up to `max_table_size`.
    pub fn new(max_table_size: usize) -> Self {
        let max_table_size = max_table_size.min(MAX_DYNAMIC_TABLE_SIZE);
        Self {
            dynamic: VecDeque::new(),
            max_table_size,
            settings_max_table_size: max_table_size,
            max_header_list_size: DEFAULT_MAX_HEADER_LIST_SIZE,
            table_size: 0,
            seen_field: false,
        }
    }

    /// Sets the table cap *and* the ceiling a peer update may raise it to.
    ///
    /// This is the value advertised in `SETTINGS_HEADER_TABLE_SIZE`; an update above it is
    /// a decoding error (RFC 7541 §4.2), which bounds the memory a peer can make us use.
    pub fn set_max_table_size(&mut self, size: usize) {
        let size = size.min(MAX_DYNAMIC_TABLE_SIZE);
        self.max_table_size = size;
        self.settings_max_table_size = size;
        self.evict();
    }

    /// Caps the decoded header list size (`name + value + 32` bytes per field).
    pub fn set_max_header_list_size(&mut self, limit: usize) {
        self.max_header_list_size = limit;
    }

    /// Current dynamic-table byte size (entries + 32 bytes each).
    pub fn table_size(&self) -> usize {
        self.table_size
    }

    fn evict(&mut self) {
        while self.table_size > self.max_table_size {
            let Some((name, value)) = self.dynamic.pop_back() else {
                self.table_size = 0;
                break;
            };
            self.table_size -= name.len() + value.len() + ENTRY_OVERHEAD;
        }
    }

    fn insert(&mut self, name: String, value: String) {
        let size = name.len() + value.len() + ENTRY_OVERHEAD;
        if size > self.max_table_size {
            self.dynamic.clear();
            self.table_size = 0;
            return;
        }
        self.dynamic.push_front((name, value));
        self.table_size += size;
        self.evict();
    }

    fn lookup(&self, index: usize) -> Result<(String, String), FetchError> {
        if index == 0 {
            return Err(h2_error("HPACK index 0 is invalid"));
        }
        if index <= STATIC_TABLE.len() {
            let (name, value) = STATIC_TABLE[index - 1];
            return Ok((name.to_owned(), value.to_owned()));
        }
        let dynamic_index = index - STATIC_TABLE.len() - 1;
        self.dynamic
            .get(dynamic_index)
            .cloned()
            .ok_or_else(|| h2_error(format!("HPACK index {index} outside dynamic table")))
    }

    /// Decodes a complete header block into `(name, value)` pairs.
    pub fn decode(&mut self, block: &[u8]) -> Result<Vec<(String, String)>, FetchError> {
        // Size updates are allowed at the start of *every* header block, not just the first
        // one (RFC 7541 §4.2); reset the per-block state here.
        self.seen_field = false;
        let mut headers = Vec::new();
        let mut decoded_size = 0usize;
        let mut pos = 0usize;
        while pos < block.len() {
            let byte = block[pos];
            if byte & 0x80 != 0 {
                // Indexed header field.
                let index = decode_integer(block, &mut pos, 7)?;
                let (name, value) = self.lookup(index)?;
                if !name.is_ascii() || name.bytes().any(|b| b.is_ascii_uppercase()) {
                    return Err(h2_error("HPACK header name must be lowercase ASCII"));
                }
                push_header(
                    &mut headers,
                    &mut decoded_size,
                    self.max_header_list_size,
                    name,
                    value,
                )?;
                self.seen_field = true;
            } else if byte & 0x40 != 0 {
                // Literal header field with incremental indexing.
                let index = decode_integer(block, &mut pos, 6)?;
                let name = if index == 0 {
                    decode_string(block, &mut pos)?
                } else {
                    self.lookup(index)?.0
                };
                let value = decode_string(block, &mut pos)?;
                if !name.is_ascii() || name.bytes().any(|b| b.is_ascii_uppercase()) {
                    return Err(h2_error("HPACK header name must be lowercase ASCII"));
                }
                self.insert(name.clone(), value.clone());
                push_header(
                    &mut headers,
                    &mut decoded_size,
                    self.max_header_list_size,
                    name,
                    value,
                )?;
                self.seen_field = true;
            } else if byte & 0x20 != 0 {
                // Dynamic table size update.
                if self.seen_field {
                    return Err(h2_error("HPACK table size update after header fields"));
                }
                let size = decode_integer(block, &mut pos, 5)?;
                if size > self.settings_max_table_size {
                    return Err(h2_error(format!(
                        "HPACK dynamic table size update {size} exceeds the advertised \
                         maximum {}",
                        self.settings_max_table_size
                    )));
                }
                self.max_table_size = size;
                self.evict();
            } else {
                // Literal without indexing (0000) or never indexed (0001).
                let index = decode_integer(block, &mut pos, 4)?;
                let name = if index == 0 {
                    decode_string(block, &mut pos)?
                } else {
                    self.lookup(index)?.0
                };
                let value = decode_string(block, &mut pos)?;
                if !name.is_ascii() || name.bytes().any(|b| b.is_ascii_uppercase()) {
                    return Err(h2_error("HPACK header name must be lowercase ASCII"));
                }
                push_header(
                    &mut headers,
                    &mut decoded_size,
                    self.max_header_list_size,
                    name,
                    value,
                )?;
                self.seen_field = true;
            }
        }
        Ok(headers)
    }
}

/// Appends one decoded field, enforcing the decoded header-list cap.
fn push_header(
    headers: &mut Vec<(String, String)>,
    decoded_size: &mut usize,
    limit: usize,
    name: String,
    value: String,
) -> Result<(), FetchError> {
    *decoded_size = decoded_size.saturating_add(name.len() + value.len() + ENTRY_OVERHEAD);
    if *decoded_size > limit {
        return Err(h2_error(
            "HPACK decoded header list exceeds the configured maximum",
        ));
    }
    headers.push((name, value));
    Ok(())
}

/// Minimal HPACK encoder: static-table indexed fields where possible and raw
/// literals without indexing otherwise (no Huffman, no dynamic table). Suitable
/// for tests and the in-process loopback server.
#[derive(Default)]
pub struct Encoder;

impl Encoder {
    pub fn new() -> Self {
        Self
    }

    /// Encodes `headers` (names must already be lowercase).
    pub fn encode(&self, headers: &[(String, String)]) -> Vec<u8> {
        let mut out = Vec::new();
        for (name, value) in headers {
            let name = name.to_ascii_lowercase();
            if let Some(index) = static_index(&name, value) {
                encode_integer(index as usize, 7, &mut out, 0x80);
                continue;
            }
            match static_name_index(&name) {
                Some(index) => {
                    encode_integer(index, 4, &mut out, 0x00);
                }
                None => {
                    out.push(0x00);
                    encode_string(&name, &mut out);
                }
            }
            encode_string(value, &mut out);
        }
        out
    }
}
