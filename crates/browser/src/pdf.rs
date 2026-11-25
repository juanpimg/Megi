//! Deterministic, dependency-free PDF 1.4 writer for headless printing (M5).
//!
//! ISO 32000-1 §7.5: a PDF file is a sequence of indirect objects
//! (`n g obj … endobj`) followed by a cross-reference table (§7.5.8) and a
//! `trailer` that locates the root object via `startxref`. The rendered page
//! is embedded as an image XObject (unchanged visual fidelity); document text
//! runs are additionally emitted as §9.4 text objects using Helvetica (a
//! base-14 font) and WinAnsi strings (§7.3.4), so extractors see real text.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use crate::error::ShellError;

mod objects;
mod page;
mod text;

use objects::PdfWriter;
use page::PageGeometry;

/// One selectable text run in document (top-left CSS) coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct PdfTextRun {
    /// Visible characters of the run (already in logical reading order).
    pub text: String,
    /// Left edge of the run, in CSS px.
    pub x: f32,
    /// Glyph baseline measured from the top of the document, in CSS px.
    pub baseline_y: f32,
    /// Font size in CSS px.
    pub font_size: f32,
}

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

/// Pads short RGB inputs with white so the image stream is always valid.
fn samples(width: u32, height: u32, rgb: &[u8]) -> Cow<'_, [u8]> {
    let needed = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(3);
    if rgb.len() >= needed {
        Cow::Borrowed(&rgb[..needed])
    } else {
        let mut padded = vec![255u8; needed];
        let copy = rgb.len().min(needed);
        padded[..copy].copy_from_slice(&rgb[..copy]);
        Cow::Owned(padded)
    }
}

/// Builds a one-page PDF 1.4 document embedding `rgb` as an image XObject.
///
/// `rgb` holds `width * height * 3` samples (8 bits per component, DeviceRGB).
/// The compressor is the in-tree RFC 1950 zlib writer, so `/FlateDecode` needs
/// no external crate and the output stays byte-for-byte deterministic.
pub fn build_pdf(width: u32, height: u32, rgb: &[u8]) -> Vec<u8> {
    build_pdf_with_text(width, height, rgb, &[])
}

/// [`build_pdf`] plus a selectable text layer and pagination by baseline.
///
/// Object numbers are deterministic: 1 catalog, 2 pages, 3 image, 4 font,
/// `5..5+N` page dictionaries and `5+N..5+2N` content streams. The raster is
/// drawn only on page 1; later pages carry their own slice of the text layer.
pub fn build_pdf_with_text(width: u32, height: u32, rgb: &[u8], runs: &[PdfTextRun]) -> Vec<u8> {
    build_pdf_with_geometry(
        width,
        height,
        rgb,
        runs,
        &PageGeometry::from_raster(width, height),
    )
}

/// [`build_pdf_with_text`] with a resolved `@page` geometry.
///
/// The raster is still drawn at its natural size; the `@page` size sets the
/// ISO 32000-1 §7.7.3.5 `/MediaBox` and the margins inset text placement.
fn build_pdf_with_geometry(
    width: u32,
    height: u32,
    rgb: &[u8],
    runs: &[PdfTextRun],
    page: &PageGeometry,
) -> Vec<u8> {
    let samples = samples(width, height, rgb);
    let compressed = megi_webview::paint::zlib_compress(samples.as_ref());
    let pages_lines = text::paginate(runs, page.content_height());
    let pages = pages_lines.len();

    let mut writer = PdfWriter::new(256 + compressed.len());
    writer.object(b"<< /Type /Catalog /Pages 2 0 R >>");
    let kids = (0..pages)
        .map(|index| format!("{} 0 R", 5 + index))
        .collect::<Vec<_>>()
        .join(" ");
    writer.object(format!("<< /Type /Pages /Kids [{kids}] /Count {pages} >>").as_bytes());
    let image = format!(
        "<< /Type /XObject /Subtype /Image /Width {width} /Height {height} \
         /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode \
         /Length {} >>",
        compressed.len()
    );
    writer.stream(&image, &compressed);
    writer.object(
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    );

    for (index, _) in pages_lines.iter().enumerate() {
        let page_dict = format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] \
             /Resources << /XObject << /Im0 3 0 R >> /Font << /F1 4 0 R >> >> \
             /Contents {} 0 R >>",
            page.width,
            page.height,
            5 + pages + index
        );
        writer.object(page_dict.as_bytes());
    }
    for (index, lines) in pages_lines.iter().enumerate() {
        let content = text::content_stream(width, height, index, lines, page);
        let dict = format!("<< /Length {} >>", content.len());
        writer.stream(&dict, content.as_bytes());
    }
    writer.finish()
}

/// Builds a PDF from raw RGB samples and atomically writes it to `path`.
pub fn write_pdf(
    width: u32,
    height: u32,
    rgb: &[u8],
    path: impl AsRef<Path>,
) -> Result<PdfInfo, ShellError> {
    write_pdf_with_text(width, height, rgb, path, &[])
}

/// [`write_pdf`] with an added selectable text layer.
pub fn write_pdf_with_text(
    width: u32,
    height: u32,
    rgb: &[u8],
    path: impl AsRef<Path>,
    runs: &[PdfTextRun],
) -> Result<PdfInfo, ShellError> {
    write_pdf_with_geometry(
        width,
        height,
        rgb,
        path,
        runs,
        &PageGeometry::from_raster(width, height),
    )
}

/// [`write_pdf_with_text`] with a resolved `@page` geometry.
fn write_pdf_with_geometry(
    width: u32,
    height: u32,
    rgb: &[u8],
    path: impl AsRef<Path>,
    runs: &[PdfTextRun],
    page: &PageGeometry,
) -> Result<PdfInfo, ShellError> {
    let bytes = build_pdf_with_geometry(width, height, rgb, runs, page);
    let path = path.as_ref().to_path_buf();
    crate::headless::write_bytes(&path, &bytes)?;
    Ok(PdfInfo {
        path,
        width: page.width.round() as u32,
        height: page.height.round() as u32,
        pdf_bytes: bytes.len(),
    })
}

/// Renders `input` once and writes its raster plus text layer to a PDF.
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
    let runs = text::runs_from_display_list(&output.display_list);
    let (width, height) = (output.pixmap.width, output.pixmap.height);
    let geometry = page::geometry_from_document(&output.document, width, height);
    let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
    for pixel in &output.pixmap.pixels {
        rgb.push(pixel.r);
        rgb.push(pixel.g);
        rgb.push(pixel.b);
    }
    write_pdf_with_geometry(width, height, &rgb, path, &runs, &geometry)
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
    fn pdf_text_layer_is_selectable_and_paginates_lines() {
        let runs = vec![
            PdfTextRun {
                text: "Megi PDF".to_string(),
                x: 8.0,
                baseline_y: 20.0,
                font_size: 12.0,
            },
            PdfTextRun {
                text: "second page".to_string(),
                x: 8.0,
                baseline_y: 108.0,
                font_size: 12.0,
            },
        ];
        let bytes = build_pdf_with_text(100, 100, &tiny_rgb(), &runs);
        let document = String::from_utf8_lossy(&bytes);
        assert!(
            document.contains("/Type /Pages /Kids [5 0 R 6 0 R] /Count 2"),
            "a 108px baseline on a 100px page needs two pages; got {document:?}"
        );
        assert!(
            document.contains("BT\n/F1 12 Tf\n1 0 0 1 8 80 Tm\n(Megi PDF) Tj\nET"),
            "page 1 must carry a BT/Tj/ET text object with the visible text; got {document:?}"
        );
        assert!(
            document.contains("1 0 0 1 8 92 Tm\n(second page) Tj"),
            "a line below the first page must be emitted on page 2 at its local y"
        );
        assert!(
            document.contains("/BaseFont /Helvetica /Encoding /WinAnsiEncoding"),
            "text needs a standard base-14 font with WinAnsi encoding"
        );
    }

    /// First `Tm` translation in a content stream (ISO 32000-1 §9.4.1).
    fn first_tm(bytes: &[u8]) -> (f32, f32) {
        let document = String::from_utf8_lossy(bytes);
        let marker = "1 0 0 1 ";
        let start = document.find(marker).expect("a positioned text object") + marker.len();
        let mut parts = document[start..].split_whitespace();
        let x: f32 = parts.next().expect("Tm x").parse().expect("numeric x");
        let y: f32 = parts.next().expect("Tm y").parse().expect("numeric y");
        (x, y)
    }

    /// CSS Paged Media 3 §4: `@page { size; margin }` must resize the MediaBox
    /// (ISO 32000-1 §7.7.3.5) and inset positioned text by the page margins.
    #[test]
    fn page_rule_sets_mediabox_and_text_inset() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path =
            std::env::temp_dir().join(format!("megi-pdf-page-{}-{nanos}.pdf", std::process::id()));
        let plain = print_pdf(&crate::pages::data_url("<p>Hello</p>"), 400, 300, &path)
            .expect("plain print");
        let plain_bytes = std::fs::read(&path).expect("plain PDF bytes");
        assert_eq!((plain.width, plain.height), (400, 300), "default page size");

        let paged = print_pdf(
            &crate::pages::data_url(
                "<style>@page { size: 200px 100px; margin: 10px }</style><p>Hello</p>",
            ),
            400,
            300,
            &path,
        )
        .expect("paged print");
        let paged_bytes = std::fs::read(&path).expect("paged PDF bytes");
        let _ = std::fs::remove_file(&path);

        let document = String::from_utf8_lossy(&paged_bytes);
        assert!(
            document.contains("/MediaBox [0 0 200 100]"),
            "size must size the MediaBox; got {document:?}"
        );
        let (px, py) = first_tm(&plain_bytes);
        let (qx, qy) = first_tm(&paged_bytes);
        assert!(
            (qx - (px + 10.0)).abs() < 0.01,
            "margin-left must inset text by 10px: plain x={px}, paged x={qx}"
        );
        // Plain: py = 300 - baseline. Paged: qy = 100 - 10 - baseline.
        assert!(
            (qy - (py - 210.0)).abs() < 0.01,
            "margin-top must inset text by 10px from the new page top: plain y={py}, paged y={qy}"
        );
        assert_eq!(
            (paged.width, paged.height),
            (200, 100),
            "reported page size follows the @page size"
        );
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
