//! DEFLATE encoder tests.

#[allow(unused_imports)]
use super::*;

#[test]
fn test_length_and_distance_code_tables_are_consistent() {
    // Length codes must be monotonically increasing and cover 3..=258.
    let mut previous = 0;
    for length in MIN_MATCH..=MAX_MATCH {
        let (symbol, extra, value) = length_code(length);
        let (_, _, base) = length_code_base(symbol);
        assert_eq!(base + value as usize, length);
        assert!(extra <= 5);
        assert!(symbol <= 285);
        assert!(symbol >= previous);
        previous = symbol;
    }
    // Distance codes must cover powers-of-two boundaries up to 32 KiB.
    for distance in [
        1usize, 2, 3, 4, 5, 6, 8, 16, 32, 64, 128, 256, 512, 1024, 4096, 16384, 32768,
    ] {
        let (symbol, extra, value) = distance_code(distance);
        let (_, _, base) = distance_code_base(symbol);
        assert_eq!(base + value as usize, distance);
        assert!(extra <= 13);
    }
}

fn length_code_base(symbol: u16) -> (u16, u32, usize) {
    for length in MIN_MATCH..=MAX_MATCH {
        let (candidate, extra, value) = length_code(length);
        if candidate == symbol {
            return (candidate, extra, length - value as usize);
        }
    }
    panic!("unknown length symbol {symbol}");
}

fn distance_code_base(symbol: u16) -> (u16, u32, usize) {
    for distance in 1..=WINDOW_SIZE {
        let (candidate, extra, value) = distance_code(distance);
        if candidate == symbol {
            return (candidate, extra, distance - value as usize);
        }
    }
    panic!("unknown distance symbol {symbol}");
}

#[test]
fn test_zlib_header_is_valid() {
    let compressed = zlib_compress(b"hello hello hello");
    assert_eq!(compressed[0], 0x78);
    assert_eq!(
        ((u32::from(compressed[0]) << 8) | u32::from(compressed[1])) % 31,
        0,
        "zlib FCHECK must make the header a multiple of 31"
    );
}

#[test]
fn test_repetitive_and_random_inputs_are_deterministic() {
    let mut pseudo_random = Vec::with_capacity(65_536);
    let mut state = 0x1234_5678u32;
    for _ in 0..65_536 {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        pseudo_random.push((state >> 24) as u8);
    }
    let inputs: [&[u8]; 4] = [b"", b"a", b"abcabcabcabcabcabc", &pseudo_random];
    for input in inputs {
        let first = zlib_compress(input);
        let second = zlib_compress(input);
        assert_eq!(first, second, "compression must be deterministic");
    }
    // Long runs of zeros (the dominant pattern of filtered PNG scanlines)
    // must compress to a handful of bytes.
    let zeros = vec![0u8; 1_920_600];
    let compressed = zlib_compress(&zeros);
    assert!(
        compressed.len() < 20_000,
        "zero runs should compress massively, got {} bytes",
        compressed.len()
    );
}
