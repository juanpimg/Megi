//! HPACK Huffman decoder (RFC 7541 Appendix B).

use crate::error::FetchError;

use super::{HUFFMAN, h2_error};

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
