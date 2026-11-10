//! Renders an SVG file to a PNG image using the `svg` crate and a tiny embedded
//! PNG writer (stored DEFLATE blocks, no external dependencies).
//!
//! ```bash
//! cargo run --example svg_to_png -- tests/fixtures/simple.svg /tmp/simple.png
//! cargo run --example svg_to_png -- image.svg out.png --width 512
//! ```

use svg::{SvgOptions, render_svg};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args().skip(1);
    let Some(input) = arguments.next() else {
        eprintln!("usage: svg_to_png <input.svg> [output.png] [--width N] [--height N]");
        std::process::exit(2);
    };
    let mut output = None;
    let mut width = None;
    let mut height = None;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--width" => {
                width = arguments.next().and_then(|value| value.parse::<u32>().ok());
            }
            "--height" => {
                height = arguments.next().and_then(|value| value.parse::<u32>().ok());
            }
            other => {
                if output.is_none() {
                    output = Some(other.to_owned());
                } else {
                    eprintln!("unexpected argument {other:?}");
                    std::process::exit(2);
                }
            }
        }
    }
    let output = output.unwrap_or_else(|| "out.png".to_owned());
    let bytes = std::fs::read(&input)?;
    let image = render_svg(&bytes, &SvgOptions { width, height })?;
    let png = encode_png(image.width, image.height, &image.data);
    std::fs::write(&output, png)?;
    println!(
        "rendered {input} -> {output} ({}x{}, {} bytes)",
        image.width,
        image.height,
        std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0)
    );
    Ok(())
}

/// Minimal deterministic PNG encoder (RGBA8, stored DEFLATE blocks).
fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut png = Vec::new();
    png.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    push_chunk(&mut png, b"IHDR", &ihdr);
    let stride = width as usize * 4;
    let mut raw = Vec::with_capacity((stride + 1) * height as usize);
    for row in 0..height as usize {
        raw.push(0); // filter: none
        raw.extend_from_slice(&rgba[row * stride..(row + 1) * stride]);
    }
    let mut zlib = Vec::with_capacity(raw.len() + 16);
    zlib.extend_from_slice(&[0x78, 0x01]);
    let mut offset = 0usize;
    while offset < raw.len() {
        let block = (raw.len() - offset).min(0xFFFF);
        let last = offset + block == raw.len();
        zlib.push(if last { 1 } else { 0 });
        zlib.extend_from_slice(&(block as u16).to_le_bytes());
        zlib.extend_from_slice(&(!(block as u16)).to_le_bytes());
        zlib.extend_from_slice(&raw[offset..offset + block]);
        offset += block;
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());
    push_chunk(&mut png, b"IDAT", &zlib);
    push_chunk(&mut png, b"IEND", &[]);
    png
}

fn push_chunk(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    png.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(data);
    png.extend_from_slice(kind);
    png.extend_from_slice(data);
    png.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}
