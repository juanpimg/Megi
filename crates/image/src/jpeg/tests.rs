#[allow(unused_imports)]
use super::*;

#[cfg(test)]
mod dequantize_overflow_tests {
    use super::*;

    #[test]
    pub(crate) fn dequantize_rejects_i32_overflow() {
        let mut quant = QuantTable {
            values: [1u16; BLOCK_LEN],
        };
        quant.values[0] = u16::MAX;
        let mut coefficients = [0i32; BLOCK_LEN];
        coefficients[0] = i32::MAX;
        let result = dequantize(&mut coefficients, &quant);
        assert!(result.is_err());

        quant.values[0] = 2;
        let mut small = [0i32; BLOCK_LEN];
        small[0] = 3;
        assert!(dequantize(&mut small, &quant).is_ok());
        assert_eq!(small[0], 6);
    }
}

/// Real-world progressive JPEGs (GNU/Debian sites) must decode (#1888).
///
/// Both files contain AC successive-approximation refinement scans whose EOB
/// runs carry buffered correction bits for already-significant coefficients.
#[test]
fn decodes_real_progressive_jpegs() {
    for (name, bytes) in [
        (
            "dragora-tde-thumb.jpg",
            include_bytes!("../../tests/fixtures/jpeg/dragora-tde-thumb.jpg") as &[u8],
        ),
        (
            "debconf26_group_photo.jpg",
            include_bytes!("../../tests/fixtures/jpeg/debconf26_group_photo.jpg") as &[u8],
        ),
    ] {
        let result = decode_jpeg(bytes, &PngLimits::default());
        assert!(result.is_ok(), "{name}: {:?}", result.err());
    }
}
