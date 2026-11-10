//! HPACK — header compression for HTTP/2 (RFC 7541).
//!
//! The decoder is complete for the representations a browser client can receive:
//! indexed fields (static + dynamic table), literals with incremental indexing,
//! literals without indexing, never-indexed literals, dynamic-table size updates
//! and Huffman-coded strings (Appendix B). The encoder is deliberately minimal
//! (static-table indexed fields and raw literals without indexing); it is used by
//! tests/loopback servers and keeps the production code path read-only.
//!
//! Anti-DoS limits: header block size (enforced by [`super::frame`]), dynamic
//! table size (from `SETTINGS_HEADER_TABLE_SIZE`), and a hard cap on the Huffman
//! padding/EOS handling. Malformed input returns [`FetchError::Http2`] — never a
//! panic.

use std::collections::VecDeque;

use crate::error::FetchError;

/// The 61-entry static table of RFC 7541 Appendix A (index 1..=61).
pub const STATIC_TABLE: [(&str, &str); 61] = [
    (":authority", ""),
    (":method", "GET"),
    (":method", "POST"),
    (":path", "/"),
    (":path", "/index.html"),
    (":scheme", "http"),
    (":scheme", "https"),
    (":status", "200"),
    (":status", "204"),
    (":status", "206"),
    (":status", "304"),
    (":status", "400"),
    (":status", "404"),
    (":status", "500"),
    ("accept-charset", ""),
    ("accept-encoding", "gzip, deflate"),
    ("accept-language", ""),
    ("accept-ranges", ""),
    ("accept", ""),
    ("access-control-allow-origin", ""),
    ("age", ""),
    ("allow", ""),
    ("authorization", ""),
    ("cache-control", ""),
    ("content-disposition", ""),
    ("content-encoding", ""),
    ("content-language", ""),
    ("content-length", ""),
    ("content-location", ""),
    ("content-range", ""),
    ("content-type", ""),
    ("cookie", ""),
    ("date", ""),
    ("etag", ""),
    ("expect", ""),
    ("expires", ""),
    ("from", ""),
    ("host", ""),
    ("if-match", ""),
    ("if-modified-since", ""),
    ("if-none-match", ""),
    ("if-range", ""),
    ("if-unmodified-since", ""),
    ("last-modified", ""),
    ("link", ""),
    ("location", ""),
    ("max-forwards", ""),
    ("proxy-authenticate", ""),
    ("proxy-authorization", ""),
    ("range", ""),
    ("referer", ""),
    ("refresh", ""),
    ("retry-after", ""),
    ("server", ""),
    ("set-cookie", ""),
    ("strict-transport-security", ""),
    ("transfer-encoding", ""),
    ("user-agent", ""),
    ("vary", ""),
    ("via", ""),
    ("www-authenticate", ""),
];

/// RFC 7541 Appendix B Huffman codes: `(code, bit_length)` per symbol 0..=255,
/// with EOS at index 256.
#[rustfmt::skip]
const HUFFMAN: [(u32, u8); 257] = [
    (0x1ff8, 13), (0x7fffd8, 23), (0xfffffe2, 28), (0xfffffe3, 28), (0xfffffe4, 28),
    (0xfffffe5, 28), (0xfffffe6, 28), (0xfffffe7, 28), (0xfffffe8, 28), (0xffffea, 24),
    (0x3ffffffc, 30), (0xfffffe9, 28), (0xfffffea, 28), (0x3ffffffd, 30), (0xfffffeb, 28),
    (0xfffffec, 28), (0xfffffed, 28), (0xfffffee, 28), (0xfffffef, 28), (0xffffff0, 28),
    (0xffffff1, 28), (0xffffff2, 28), (0x3ffffffe, 30), (0xffffff3, 28), (0xffffff4, 28),
    (0xffffff5, 28), (0xffffff6, 28), (0xffffff7, 28), (0xffffff8, 28), (0xffffff9, 28),
    (0xffffffa, 28), (0xffffffb, 28), (0x14, 6), (0x3f8, 10), (0x3f9, 10),
    (0xffa, 12), (0x1ff9, 13), (0x15, 6), (0xf8, 8), (0x7fa, 11),
    (0x3fa, 10), (0x3fb, 10), (0xf9, 8), (0x7fb, 11), (0xfa, 8),
    (0x16, 6), (0x17, 6), (0x18, 6), (0x0, 5), (0x1, 5),
    (0x2, 5), (0x19, 6), (0x1a, 6), (0x1b, 6), (0x1c, 6),
    (0x1d, 6), (0x1e, 6), (0x1f, 6), (0x5c, 7), (0xfb, 8),
    (0x7ffc, 15), (0x20, 6), (0xffb, 12), (0x3fc, 10), (0x1ffa, 13),
    (0x21, 6), (0x5d, 7), (0x5e, 7), (0x5f, 7), (0x60, 7),
    (0x61, 7), (0x62, 7), (0x63, 7), (0x64, 7), (0x65, 7),
    (0x66, 7), (0x67, 7), (0x68, 7), (0x69, 7), (0x6a, 7),
    (0x6b, 7), (0x6c, 7), (0x6d, 7), (0x6e, 7), (0x6f, 7),
    (0x70, 7), (0x71, 7), (0x72, 7), (0xfc, 8), (0x73, 7),
    (0xfd, 8), (0x1ffb, 13), (0x7fff0, 19), (0x1ffc, 13), (0x3ffc, 14),
    (0x22, 6), (0x7ffd, 15), (0x3, 5), (0x23, 6), (0x4, 5),
    (0x24, 6), (0x5, 5), (0x25, 6), (0x26, 6), (0x27, 6),
    (0x6, 5), (0x74, 7), (0x75, 7), (0x28, 6), (0x29, 6),
    (0x2a, 6), (0x7, 5), (0x2b, 6), (0x76, 7), (0x2c, 6),
    (0x8, 5), (0x9, 5), (0x2d, 6), (0x77, 7), (0x78, 7),
    (0x79, 7), (0x7a, 7), (0x7b, 7), (0x7ffe, 15), (0x7fc, 11),
    (0x3ffd, 14), (0x1ffd, 13), (0xffffffc, 28), (0xfffe6, 20), (0x3fffd2, 22),
    (0xfffe7, 20), (0xfffe8, 20), (0x3fffd3, 22), (0x3fffd4, 22), (0x3fffd5, 22),
    (0x7fffd9, 23), (0x3fffd6, 22), (0x7fffda, 23), (0x7fffdb, 23), (0x7fffdc, 23),
    (0x7fffdd, 23), (0x7fffde, 23), (0xffffeb, 24), (0x7fffdf, 23), (0xffffec, 24),
    (0xffffed, 24), (0x3fffd7, 22), (0x7fffe0, 23), (0xffffee, 24), (0x7fffe1, 23),
    (0x7fffe2, 23), (0x7fffe3, 23), (0x7fffe4, 23), (0x1fffdc, 21), (0x3fffd8, 22),
    (0x7fffe5, 23), (0x3fffd9, 22), (0x7fffe6, 23), (0x7fffe7, 23), (0xffffef, 24),
    (0x3fffda, 22), (0x1fffdd, 21), (0xfffe9, 20), (0x3fffdb, 22), (0x3fffdc, 22),
    (0x7fffe8, 23), (0x7fffe9, 23), (0x1fffde, 21), (0x7fffea, 23), (0x3fffdd, 22),
    (0x3fffde, 22), (0xfffff0, 24), (0x1fffdf, 21), (0x3fffdf, 22), (0x7fffeb, 23),
    (0x7fffec, 23), (0x1fffe0, 21), (0x1fffe1, 21), (0x3fffe0, 22), (0x1fffe2, 21),
    (0x7fffed, 23), (0x3fffe1, 22), (0x7fffee, 23), (0x7fffef, 23), (0xfffea, 20),
    (0x3fffe2, 22), (0x3fffe3, 22), (0x3fffe4, 22), (0x7ffff0, 23), (0x3fffe5, 22),
    (0x3fffe6, 22), (0x7ffff1, 23), (0x3ffffe0, 26), (0x3ffffe1, 26), (0xfffeb, 20),
    (0x7fff1, 19), (0x3fffe7, 22), (0x7ffff2, 23), (0x3fffe8, 22), (0x1ffffec, 25),
    (0x3ffffe2, 26), (0x3ffffe3, 26), (0x3ffffe4, 26), (0x7ffffde, 27), (0x7ffffdf, 27),
    (0x3ffffe5, 26), (0xfffff1, 24), (0x1ffffed, 25), (0x7fff2, 19), (0x1fffe3, 21),
    (0x3ffffe6, 26), (0x7ffffe0, 27), (0x7ffffe1, 27), (0x3ffffe7, 26), (0x7ffffe2, 27),
    (0xfffff2, 24), (0x1fffe4, 21), (0x1fffe5, 21), (0x3ffffe8, 26), (0x3ffffe9, 26),
    (0xffffffd, 28), (0x7ffffe3, 27), (0x7ffffe4, 27), (0x7ffffe5, 27), (0xfffec, 20),
    (0xfffff3, 24), (0xfffed, 20), (0x1fffe6, 21), (0x3fffe9, 22), (0x1fffe7, 21),
    (0x1fffe8, 21), (0x7ffff3, 23), (0x3fffea, 22), (0x3fffeb, 22), (0x1ffffee, 25),
    (0x1ffffef, 25), (0xfffff4, 24), (0xfffff5, 24), (0x3ffffea, 26), (0x7ffff4, 23),
    (0x3ffffeb, 26), (0x7ffffe6, 27), (0x3ffffec, 26), (0x3ffffed, 26), (0x7ffffe7, 27),
    (0x7ffffe8, 27), (0x7ffffe9, 27), (0x7ffffea, 27), (0x7ffffeb, 27), (0xffffffe, 28),
    (0x7ffffec, 27), (0x7ffffed, 27), (0x7ffffee, 27), (0x7ffffef, 27), (0x7fffff0, 27),
    (0x3ffffee, 26), (0x3fffffff, 30),
];

/// HPACK dynamic table entry overhead (RFC 7541 §4.1).
const ENTRY_OVERHEAD: usize = 32;
/// Hard cap for the dynamic table regardless of what `SETTINGS` advertises.
pub const MAX_DYNAMIC_TABLE_SIZE: usize = 65_536;

fn h2_error(message: impl Into<String>) -> FetchError {
    FetchError::Http2(message.into())
}

/// One node of the Huffman decoding trie.
#[derive(Clone, Copy, Default)]
struct HuffNode {
    /// Child node indices for bits 0 and 1; `-1` means absent.
    child: [i32; 2],
    /// Decoded symbol when this node is a leaf (`u16::MAX` otherwise).
    symbol: u16,
}

fn build_huffman_tree() -> Vec<HuffNode> {
    let mut nodes = vec![HuffNode {
        child: [-1, -1],
        symbol: u16::MAX,
    }];
    for (symbol, &(code, bits)) in HUFFMAN.iter().enumerate() {
        let mut node = 0usize;
        for shift in (0..bits).rev() {
            let bit = ((code >> shift) & 1) as usize;
            let next = nodes[node].child[bit];
            node = if next < 0 {
                nodes.push(HuffNode {
                    child: [-1, -1],
                    symbol: u16::MAX,
                });
                let index = nodes.len() - 1;
                nodes[node].child[bit] = index as i32;
                index
            } else {
                next as usize
            };
        }
        nodes[node].symbol = symbol as u16;
    }
    nodes
}

static HUFFMAN_TREE: std::sync::OnceLock<Vec<HuffNode>> = std::sync::OnceLock::new();

/// Decodes an HPACK Huffman string (RFC 7541 Appendix B).
///
/// Rejects the EOS symbol, invalid padding (more than 7 bits, or bits other than
/// `1`) and truncated code words.
pub fn huffman_decode(input: &[u8]) -> Result<Vec<u8>, FetchError> {
    let tree = HUFFMAN_TREE.get_or_init(build_huffman_tree);
    let mut out = Vec::with_capacity(input.len());
    let mut node = 0usize;
    let mut trailing_ones = 0u8;
    let mut trailing_bits = 0u8;
    for &byte in input {
        for shift in (0..8).rev() {
            let bit = ((byte >> shift) & 1) as usize;
            if node == 0 {
                trailing_ones = 0;
                trailing_bits = 0;
            }
            trailing_ones = if bit == 1 {
                trailing_ones.saturating_add(1)
            } else {
                0
            };
            trailing_bits = trailing_bits.saturating_add(1);
            let next = tree[node].child[bit];
            if next < 0 {
                return Err(h2_error("invalid HPACK Huffman code"));
            }
            node = next as usize;
            let symbol = tree[node].symbol;
            if symbol != u16::MAX {
                if symbol == 256 {
                    return Err(h2_error("HPACK Huffman EOS symbol in header string"));
                }
                out.push(symbol as u8);
                node = 0;
                trailing_ones = 0;
                trailing_bits = 0;
            }
        }
    }
    // Padding must be at most 7 bits and all ones (a prefix of EOS).
    if node != 0 && (trailing_bits > 7 || trailing_ones != trailing_bits) {
        return Err(h2_error("invalid HPACK Huffman padding"));
    }
    Ok(out)
}

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
            table_size: 0,
            seen_field: false,
        }
    }

    /// Updates the dynamic-table cap from `SETTINGS_HEADER_TABLE_SIZE` (evicting
    /// entries as needed).
    pub fn set_max_table_size(&mut self, size: usize) {
        self.max_table_size = size.min(MAX_DYNAMIC_TABLE_SIZE);
        self.evict();
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
        let mut headers = Vec::new();
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
                headers.push((name, value));
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
                headers.push((name, value));
                self.seen_field = true;
            } else if byte & 0x20 != 0 {
                // Dynamic table size update.
                if self.seen_field {
                    return Err(h2_error("HPACK table size update after header fields"));
                }
                let size = decode_integer(block, &mut pos, 5)?;
                self.set_max_table_size(size);
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
                headers.push((name, value));
                self.seen_field = true;
            }
        }
        Ok(headers)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_rfc7541_c4_1_first_request() {
        // RFC 7541 C.4.1: GET http://www.example.com/ with Huffman-coded authority.
        let block = [
            0x82, 0x86, 0x84, 0x41, 0x8c, 0xf1, 0xe3, 0xc2, 0xe5, 0xf2, 0x3a, 0x6b, 0xa0, 0xab,
            0x90, 0xf4, 0xff,
        ];
        let headers = Decoder::default().decode(&block).expect("valid HPACK");
        assert_eq!(
            headers,
            vec![
                (":method".to_owned(), "GET".to_owned()),
                (":scheme".to_owned(), "http".to_owned()),
                (":path".to_owned(), "/".to_owned()),
                (":authority".to_owned(), "www.example.com".to_owned()),
            ]
        );
    }

    #[test]
    fn decodes_rfc7541_c4_2_second_request_with_dynamic_table() {
        let first = [
            0x82, 0x86, 0x84, 0x41, 0x8c, 0xf1, 0xe3, 0xc2, 0xe5, 0xf2, 0x3a, 0x6b, 0xa0, 0xab,
            0x90, 0xf4, 0xff,
        ];
        let second = [
            0x82, 0x86, 0x84, 0xbe, 0x58, 0x86, 0xa8, 0xeb, 0x10, 0x64, 0x9c, 0xbf,
        ];
        let mut decoder = Decoder::default();
        decoder.decode(&first).expect("first block");
        let headers = decoder.decode(&second).expect("second block");
        assert_eq!(
            headers,
            vec![
                (":method".to_owned(), "GET".to_owned()),
                (":scheme".to_owned(), "http".to_owned()),
                (":path".to_owned(), "/".to_owned()),
                (":authority".to_owned(), "www.example.com".to_owned()),
                ("cache-control".to_owned(), "no-cache".to_owned()),
            ]
        );
        // After C.4.2 the dynamic table holds :authority (57 B) and
        // cache-control: no-cache (53 B): 110 B total.
        assert_eq!(decoder.table_size(), 110);
    }

    #[test]
    fn encoder_decoder_roundtrip() {
        let headers = vec![
            (":method".to_owned(), "GET".to_owned()),
            (":scheme".to_owned(), "https".to_owned()),
            (":authority".to_owned(), "example.com".to_owned()),
            (":path".to_owned(), "/index.html".to_owned()),
            ("accept".to_owned(), "*/*".to_owned()),
            ("user-agent".to_owned(), "megi-test/1.0".to_owned()),
        ];
        let block = Encoder::new().encode(&headers);
        let decoded = Decoder::default().decode(&block).expect("roundtrip");
        assert_eq!(decoded, headers);
    }

    #[test]
    fn huffman_known_strings() {
        assert_eq!(
            huffman_decode(&[
                0xf1, 0xe3, 0xc2, 0xe5, 0xf2, 0x3a, 0x6b, 0xa0, 0xab, 0x90, 0xf4, 0xff
            ])
            .unwrap(),
            b"www.example.com"
        );
        assert_eq!(
            huffman_decode(&[0xa8, 0xeb, 0x10, 0x64, 0x9c, 0xbf]).unwrap(),
            b"no-cache"
        );
    }

    #[test]
    fn huffman_rejects_eos_truncation_and_bad_padding() {
        // EOS (30 one bits) is not a valid symbol in a string.
        assert!(huffman_decode(&[0xff, 0xff, 0xff, 0xff]).is_err());
        // A lone high prefix that never completes a code and is not all-ones padding.
        assert!(huffman_decode(&[0x00]).is_err());
    }

    #[test]
    fn rejects_invalid_input() {
        let mut decoder = Decoder::default();
        assert!(decoder.decode(&[0x80]).is_err()); // index 0
        assert!(decoder.decode(&[0xff, 0xff, 0xff, 0xff, 0xff]).is_err()); // index too large
        assert!(decoder.decode(&[0x40, 0x01, b'A']).is_err()); // uppercase name
        assert!(decoder.decode(&[0x00, 0x05]).is_err()); // truncated name
    }
}
