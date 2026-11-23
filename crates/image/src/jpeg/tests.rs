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

#[cfg(test)]
struct TestBitWriter {
    bytes: Vec<u8>,
    acc: u32,
    bits: u8,
}

#[cfg(test)]
impl TestBitWriter {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            acc: 0,
            bits: 0,
        }
    }

    fn write_bits(&mut self, value: u32, count: u8) {
        for shift in (0..count).rev() {
            let bit = (value >> shift) & 1;
            self.acc = (self.acc << 1) | bit;
            self.bits += 1;
            if self.bits == 8 {
                let byte = self.acc as u8;
                self.bytes.push(byte);
                if byte == 0xFF {
                    self.bytes.push(0x00);
                }
                self.acc = 0;
                self.bits = 0;
            }
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.bits > 0 {
            let pad = 8 - self.bits;
            self.write_bits((1u32 << pad) - 1, pad);
        }
        self.bytes
    }
}

#[cfg(test)]
fn push_dht(out: &mut Vec<u8>, class_id: u8, counts: [u8; 16], values: &[u8]) {
    let len = 2 + 1 + 16 + values.len();
    out.extend_from_slice(&[0xFF, 0xC4, (len >> 8) as u8, len as u8, class_id]);
    out.extend_from_slice(&counts);
    out.extend_from_slice(values);
}

#[cfg(test)]
fn push_sos(out: &mut Vec<u8>, comps: &[(u8, u8)], ss: u8, se: u8, ah: u8, al: u8) {
    let len = 2 + 1 + comps.len() * 2 + 3;
    out.extend_from_slice(&[0xFF, 0xDA, (len >> 8) as u8, len as u8, comps.len() as u8]);
    for &(id, tables) in comps {
        out.push(id);
        out.push(tables);
    }
    out.push(ss);
    out.push(se);
    out.push((ah << 4) | al);
}

#[cfg(test)]
fn build_24x17_420_header(progressive: bool) -> Vec<u8> {
    let mut out = vec![0xFF, 0xD8];
    // DQT: table 0, 8-bit, all 1s.
    out.extend_from_slice(&[0xFF, 0xDB, 0x00, 0x43, 0x00]);
    out.extend_from_slice(&[1u8; 64]);
    // SOF0 or SOF2: 24x17, 3 components (4:2:0).
    out.extend_from_slice(&[
        0xFF,
        if progressive { 0xC2 } else { 0xC0 },
        0x00,
        0x11,
        0x08,
    ]);
    out.extend_from_slice(&17u16.to_be_bytes());
    out.extend_from_slice(&24u16.to_be_bytes());
    out.extend_from_slice(&[3, 1, 0x22, 0, 2, 0x11, 0, 3, 0x11, 0]);
    // DC table 0: code `0` -> cat 0; code `10` -> cat 5 (magnitude +16 is `10000`).
    let mut dc_counts = [0u8; 16];
    dc_counts[0] = 1;
    dc_counts[1] = 1;
    push_dht(&mut out, 0x00, dc_counts, &[0, 5]);
    // AC table 0: code `0` -> EOB (0x00).
    let mut ac_counts = [0u8; 16];
    ac_counts[0] = 1;
    push_dht(&mut out, 0x10, ac_counts, &[0x00]);
    out
}

/// Regression for #2147: interleaved progressive DC scans must decode dummy/padding
/// blocks at the right/bottom edges of non-MCU-aligned images so DC prediction and
/// refinement bit positions stay synchronized with the bitstream (ITU-T T.81 §A.2.3).
#[test]
fn interleaved_progressive_dc_dummy_blocks_preserve_prediction_and_refine_sync() {
    // 24x17, 4:2:0 => 2x2 MCUs of 16x16. Y has 3x3 valid blocks and 7 dummy blocks
    // (where block_col == 3 or block_row == 3). Give dummy Y blocks a +16 DC diff
    // (code `10` + bits `10000`) and all valid blocks a 0 DC diff (code `0`).
    let mcu_layout = [
        // (mcu_x, mcu_y)
        (0usize, 0usize),
        (1, 0),
        (0, 1),
        (1, 1),
    ];

    // 1) Baseline (SOF0) reference.
    let mut base = build_24x17_420_header(false);
    push_sos(&mut base, &[(1, 0x00), (2, 0x00), (3, 0x00)], 0, 63, 0, 0);
    let mut bw = TestBitWriter::new();
    for &(mx, my) in &mcu_layout {
        for by in 0..2 {
            for bx in 0..2 {
                let col = mx * 2 + bx;
                let row = my * 2 + by;
                if col >= 3 || row >= 3 {
                    bw.write_bits(0b10, 2); // DC cat 5
                    bw.write_bits(0b10000, 5); // +16
                } else {
                    bw.write_bits(0, 1); // DC cat 0
                }
                bw.write_bits(0, 1); // AC EOB
            }
        }
        // Cb, Cr: DC cat 0 + AC EOB.
        bw.write_bits(0, 1);
        bw.write_bits(0, 1);
        bw.write_bits(0, 1);
        bw.write_bits(0, 1);
    }
    base.extend_from_slice(&bw.finish());
    base.extend_from_slice(&[0xFF, 0xD9]);

    // 2) Progressive (SOF2) with interleaved DC first (Ss=0, Se=0, Ah=0, Al=0).
    let mut prog = build_24x17_420_header(true);
    push_sos(&mut prog, &[(1, 0x00), (2, 0x00), (3, 0x00)], 0, 0, 0, 0);
    let mut pw = TestBitWriter::new();
    for &(mx, my) in &mcu_layout {
        for by in 0..2 {
            for bx in 0..2 {
                let col = mx * 2 + bx;
                let row = my * 2 + by;
                if col >= 3 || row >= 3 {
                    pw.write_bits(0b10, 2);
                    pw.write_bits(0b10000, 5);
                } else {
                    pw.write_bits(0, 1);
                }
            }
        }
        pw.write_bits(0, 1); // Cb DC
        pw.write_bits(0, 1); // Cr DC
    }
    prog.extend_from_slice(&pw.finish());
    for &(comp_id, blocks) in &[(1u8, 9usize), (2, 4), (3, 4)] {
        push_sos(&mut prog, &[(comp_id, 0x00)], 1, 63, 0, 0);
        let mut aw = TestBitWriter::new();
        for _ in 0..blocks {
            aw.write_bits(0, 1); // AC EOB
        }
        prog.extend_from_slice(&aw.finish());
    }
    prog.extend_from_slice(&[0xFF, 0xD9]);

    let base_img = decode_jpeg(&base, &PngLimits::default()).expect("baseline 24x17 decodes");
    let prog_img = decode_jpeg(&prog, &PngLimits::default()).expect("progressive 24x17 decodes");
    assert_eq!(
        prog_img.data, base_img.data,
        "interleaved progressive DC dummy blocks must yield identical pixels to baseline"
    );

    // 3) Interleaved DC refinement (Ah=1, Al=0) also consumes 1 bit per dummy block.
    // Encode DC shifted by Al=1 in scan 1 (0 in all blocks), then in scan 2 put 1-bit
    // refinement = 1 ONLY on dummy blocks and 0 on valid blocks; valid pixels must stay
    // unchanged (all zero DC -> gray 128), not pick up dummy refinement bits.
    let mut prog_ref = build_24x17_420_header(true);
    push_sos(
        &mut prog_ref,
        &[(1, 0x00), (2, 0x00), (3, 0x00)],
        0,
        0,
        0,
        1,
    );
    let mut w1 = TestBitWriter::new();
    for _ in 0..(4 * 6) {
        w1.write_bits(0, 1);
    }
    prog_ref.extend_from_slice(&w1.finish());
    push_sos(
        &mut prog_ref,
        &[(1, 0x00), (2, 0x00), (3, 0x00)],
        0,
        0,
        1,
        0,
    );
    let mut w2 = TestBitWriter::new();
    for &(mx, my) in &mcu_layout {
        for by in 0..2 {
            for bx in 0..2 {
                let col = mx * 2 + bx;
                let row = my * 2 + by;
                w2.write_bits(u32::from(col >= 3 || row >= 3), 1);
            }
        }
        w2.write_bits(0, 1);
        w2.write_bits(0, 1);
    }
    prog_ref.extend_from_slice(&w2.finish());
    prog_ref.extend_from_slice(&[0xFF, 0xD9]);

    let ref_img = decode_jpeg(&prog_ref, &PngLimits::default()).expect("DC refine 24x17 decodes");
    assert_eq!(
        ref_img.pixel(0, 0),
        ref_img.pixel(23, 16),
        "dummy DC refinement bits must not shift into valid blocks"
    );
    assert_eq!(ref_img.pixel(23, 16), Some([128, 128, 128, 255]));
}
