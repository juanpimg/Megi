//! Deterministic, dependency-free PDF 1.4 writer for headless printing (M5).
//!
//! ISO 32000-1 §7.3/§7.5: a PDF file is a sequence of indirect objects
//! (`n g obj … endobj`) followed by a cross-reference table (`xref`) and a
//! `trailer` that locates the root object via `startxref`. The rendered page
//! is embedded as a single image XObject; no painting code changes.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use crate::error::ShellError;

/// Metadata of a written PDF.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PdfInfo {
    /// Output file path.
    pub path: PathBuf,
    /// Page width in points (one pixel = one point).
    pub width: u32,
    /// Page height in points (one pixel = one point).
    pub height: u32,
    /// Size of the written PDF in bytes.
    pub pdf_bytes: usize,
}

/// Appends one indirect object and records its byte offset for the xref table.
fn push_object(out: &mut Vec<u8>, offsets: &mut Vec<usize>, body: &[u8]) {
    offsets.push(out.len());
    out.extend_from_slice(format!("{} 0 obj\n", offsets.len()).as_bytes());
    out.extend_from_slice(body);
    out.extend_from_slice(b"\nendobj\n");
}

/// Appends a dictionary + stream object; `/Length` counts only the stream bytes.
fn push_stream_object(out: &mut Vec<u8>, offsets: &mut Vec<usize>, dict: &str, data: &[u8]) {
    let mut body = format!("{dict}\nstream\n").into_bytes();
    body.extend_from_slice(data);
    body.extend_from_slice(b"\nendstream");
    push_object(out, offsets, &body);
}

/// Builds a one-page PDF 1.4 document embedding `rgb` as an image XObject.
///
/// `rgb` holds `width * height * 3` samples (8 bits per component, DeviceRGB).
/// The compressor is the in-tree RFC 1950 zlib writer, so `/FlateDecode` needs
/// no external crate and the output stays byte-for-byte deterministic.
pub fn build_pdf(width: u32, height: u32, rgb: &[u8]) -> Vec<u8> {
    let needed = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(3);
    let samples: Cow<'_, [u8]> = if rgb.len() >= needed {
        Cow::Borrowed(&rgb[..needed])
    } else {
        // Short input is padded with white so the image stream is always valid.
        let mut padded = vec![255u8; needed];
        let copy = rgb.len().min(needed);
        padded[..copy].copy_from_slice(&rgb[..copy]);
        Cow::Owned(padded)
    };
    let compressed = megi_webview::paint::zlib_compress(samples.as_ref());

    let mut out = Vec::with_capacity(256 + compressed.len());
    let mut offsets: Vec<usize> = Vec::with_capacity(5);
    out.extend_from_slice(b"%PDF-1.4\n");

    push_object(&mut out, &mut offsets, b"<< /Type /Catalog /Pages 2 0 R >>");
    push_object(
        &mut out,
        &mut offsets,
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    );
    let page = format!(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width} {height}] \
         /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>"
    );
    push_object(&mut out, &mut offsets, page.as_bytes());

    let image = format!(
        "<< /Type /XObject /Subtype /Image /Width {width} /Height {height} \
         /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode \
         /Length {} >>",
        compressed.len()
    );
    push_stream_object(&mut out, &mut offsets, &image, &compressed);

    let content = format!("q\n{width} 0 0 {height} 0 0 cm\n/Im0 Do\nQ\n");
    let contents = format!("<< /Length {} >>", content.len());
    push_stream_object(&mut out, &mut offsets, &contents, content.as_bytes());

    let xref_offset = out.len();
    let size = offsets.len() + 1;
    out.extend_from_slice(format!("xref\n0 {size}\n").as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for offset in &offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n")
            .as_bytes(),
    );
    out
}

/// Builds a PDF from raw RGB samples and atomically writes it to `path`.
pub fn write_pdf(
    width: u32,
    height: u32,
    rgb: &[u8],
    path: impl AsRef<Path>,
) -> Result<PdfInfo, ShellError> {
    let bytes = build_pdf(width, height, rgb);
    let path = path.as_ref().to_path_buf();
    crate::headless::write_bytes(&path, &bytes)?;
    Ok(PdfInfo {
        path,
        width,
        height,
        pdf_bytes: bytes.len(),
    })
}

/// Renders `input` once and writes its raster as a one-page PDF.
///
/// The PNG encode is skipped: the pixmap is the single source for both the
/// screenshot PNG path and this PDF path, so no pixel data is produced twice.
pub fn print_pdf_with_options(
    input: &str,
    width: u32,
    height: u32,
    path: impl AsRef<Path>,
    options: &megi_webview::render::RenderOptions,
) -> Result<PdfInfo, ShellError> {
    let mut base = options.clone();
    base.encode_png = false;
    let output = crate::headless::render_target_with_options(input, width, height, &base)?;
    let (width, height) = (output.pixmap.width, output.pixmap.height);
    let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
    for pixel in &output.pixmap.pixels {
        rgb.push(pixel.r);
        rgb.push(pixel.g);
        rgb.push(pixel.b);
    }
    write_pdf(width, height, &rgb, path)
}

/// [`print_pdf_with_options`] with default render options.
pub fn print_pdf(
    input: &str,
    width: u32,
    height: u32,
    path: impl AsRef<Path>,
) -> Result<PdfInfo, ShellError> {
    print_pdf_with_options(
        input,
        width,
        height,
        path,
        &megi_webview::render::RenderOptions::default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
    }

    fn tiny_rgb() -> Vec<u8> {
        let mut rgb = Vec::new();
        for _ in 0..4 {
            rgb.extend_from_slice(&[255, 0, 0]);
        }
        for _ in 0..2 {
            rgb.extend_from_slice(&[0, 0, 255]);
        }
        rgb
    }

    #[test]
    fn pdf_has_header_xref_image_and_page_size() {
        let bytes = build_pdf(4, 2, &tiny_rgb());
        assert!(bytes.starts_with(b"%PDF-1.4\n"), "PDF 1.4 header");
        assert!(
            find(&bytes, b"/Subtype /Image").is_some(),
            "embedded image XObject"
        );
        assert!(
            find(&bytes, b"/MediaBox [0 0 4 2]").is_some(),
            "page size matches the PNG dimensions"
        );
        assert!(bytes.ends_with(b"%%EOF\n"), "EOF marker");

        let xref = find(&bytes, b"xref\n0 ").expect("xref table");
        let startxref = find(&bytes, b"startxref\n").expect("startxref");
        let value_start = startxref + b"startxref\n".len();
        let value_end = value_start + find(&bytes[value_start..], b"\n").expect("newline");
        let declared: usize = std::str::from_utf8(&bytes[value_start..value_end])
            .expect("ascii offset")
            .parse()
            .expect("numeric offset");
        assert_eq!(declared, xref, "startxref must locate the xref table");

        let keyword_end = xref + find(&bytes[xref..], b"\n").expect("keyword newline");
        assert_eq!(&bytes[xref..keyword_end], b"xref");
        let size_start = keyword_end + 1;
        let size_end = size_start + find(&bytes[size_start..], b"\n").expect("size newline");
        let size_line = std::str::from_utf8(&bytes[size_start..size_end]).expect("ascii size");
        let fields: Vec<&str> = size_line.split_whitespace().collect();
        assert_eq!(fields[0], "0");
        let count: usize = fields[1].parse().expect("entry count");
        let mut cursor = size_end + 1;
        for object_id in 0..count {
            let line_end = cursor + find(&bytes[cursor..], b"\n").expect("entry newline");
            let line = std::str::from_utf8(&bytes[cursor..line_end]).expect("ascii entry");
            let offset: usize = line[..10].parse().expect("10-digit offset");
            let kind = line.as_bytes()[17];
            if object_id == 0 {
                assert_eq!(kind, b'f', "first entry is free");
            } else {
                assert_eq!(kind, b'n', "generated entries are in use");
                let header = format!("{object_id} 0 obj");
                assert_eq!(
                    find(&bytes[offset..], header.as_bytes()),
                    Some(0),
                    "xref offset for object {object_id} must point at its header"
                );
            }
            cursor = line_end + 1;
        }
    }
}
