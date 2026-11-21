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

/// Interleaved DC-refinement scans (multi-component, `Ss == 0`, `Ah > 0`) are decoded,
/// not rejected. The module docs claimed the opposite (#1661): only interleaved **AC**
/// scans (`Ss > 0`) are unsupported. This fixture is a colour progressive JPEG whose
/// scans include `(Ns=3, Ss=0, Se=0, Ah=1, Al=0)`.
#[test]
fn interlaced_multi_component_dc_refinement_is_decoded() {
    let bytes = include_bytes!("../../tests/fixtures/jpeg/progressive.jpg") as &[u8];
    let image = decode_jpeg(bytes, &PngLimits::default()).expect("colour refinement scan decodes");
    assert!(image.width > 0 && image.height > 0);
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
