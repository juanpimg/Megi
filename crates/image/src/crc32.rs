//! Standard CRC-32 (ISO-HDLC / PNG polynomial) helpers.

/// CRC-32 polynomial used by PNG (`x^32 + x^26 + … + 1`, reflected form).
const CRC32_POLYNOMIAL: u32 = 0xEDB8_8320;

/// Reflected CRC-32 table, built at compile time.
const CRC32_TABLE: [u32; 256] = build_crc32_table();

const fn build_crc32_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut index = 0;
    while index < 256 {
        let mut crc = index as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ CRC32_POLYNOMIAL
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[index] = crc;
        index += 1;
    }
    table
}

/// Updates a running (non-finalised) CRC-32 state with `data`.
///
/// Used by the PNG chunk reader to checksum `type || data` without copying
/// them into a single buffer.
pub fn crc32_update(state: u32, data: &[u8]) -> u32 {
    let mut crc = state;
    for &byte in data {
        crc = CRC32_TABLE[((crc ^ byte as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc
}

/// Computes the CRC-32 of `data` (reflected, init `0xFFFF_FFFF`, final XOR).
///
/// Only used by the crate tests: the decoder streams chunks through
/// [`crc32_update`] and never materialises `type || data`.
#[cfg(test)]
pub fn crc32(data: &[u8]) -> u32 {
    !crc32_update(0xFFFF_FFFF, data)
}

#[cfg(test)]
mod tests {
    use super::crc32;

    #[test]
    fn known_crc32_vectors() {
        assert_eq!(crc32(b""), 0x0000_0000);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(
            crc32(b"The quick brown fox jumps over the lazy dog"),
            0x414F_A339
        );
        // CRC of the PNG signature (used by tests as a sanity anchor).
        assert_eq!(crc32(b"\x89PNG\r\n\x1a\n"), 0x7A07_09A4);
    }
}
