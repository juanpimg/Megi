//! Selectable text layer built from the paint display list.
//!
//! ISO 32000-1 §9.4 defines a text object (`BT … ET`) and the `Tf`, `Tm` and
//! `Tj` operators used here; §7.3.4 defines the literal string syntax escaped
//! by [`pdf_string`]. Glyphs stay in the raster image, so painting is untouched.

use megi_webview::display::{DisplayItem, DisplayList};

use super::{PdfTextRun, page::PageGeometry};

/// One PDF text line after merging same-baseline runs.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PdfLine {
    text: String,
    x: f32,
    baseline_y: f32,
    font_size: f32,
    width: f32,
}

const MAX_GROUP_DEPTH: u8 = 8;
const BASELINE_TOLERANCE: f32 = 2.0;

/// Collects visible text runs from the display list (including effect groups).
pub(super) fn runs_from_display_list(list: &DisplayList) -> Vec<PdfTextRun> {
    let mut runs = Vec::new();
    for item in list.iter() {
        collect(item, 0, &mut runs);
    }
    runs
}

fn collect(item: &DisplayItem, depth: u8, out: &mut Vec<PdfTextRun>) {
    match item {
        DisplayItem::Text {
            text,
            rect,
            baseline,
            font_size,
            ..
        } => out.push(PdfTextRun {
            text: text.clone(),
            x: rect.x,
            baseline_y: rect.y + baseline,
            font_size: *font_size,
        }),
        DisplayItem::EffectGroup { items, .. } if depth < MAX_GROUP_DEPTH => {
            for child in items {
                collect(child, depth + 1, out);
            }
        }
        _ => {}
    }
}

/// Groups runs into lines in reading order, inserting a space across real gaps.
fn lines(runs: &[PdfTextRun]) -> Vec<PdfLine> {
    let mut sorted: Vec<&PdfTextRun> = runs
        .iter()
        .filter(|run| {
            run.x.is_finite()
                && run.baseline_y.is_finite()
                && run.font_size.is_finite()
                && run.font_size > 0.0
                && !run.text.is_empty()
        })
        .collect();
    sorted.sort_by(|a, b| {
        a.baseline_y
            .total_cmp(&b.baseline_y)
            .then_with(|| a.x.total_cmp(&b.x))
    });

    let mut merged: Vec<PdfLine> = Vec::new();
    for run in sorted {
        let width = run.text.chars().count() as f32 * run.font_size * 0.5;
        if let Some(last) = merged.last_mut()
            && (last.baseline_y - run.baseline_y).abs() <= BASELINE_TOLERANCE
        {
            if run.x > last.x + last.width + run.font_size * 0.3 {
                last.text.push(' ');
            }
            last.text.push_str(&run.text);
            last.width = (run.x + width - last.x).max(0.0);
            continue;
        }
        merged.push(PdfLine {
            text: run.text.clone(),
            x: run.x,
            baseline_y: run.baseline_y,
            font_size: run.font_size,
            width,
        });
    }
    merged
}

/// Splits lines into pages by their baseline within the `@page` content height.
pub(super) fn paginate(runs: &[PdfTextRun], height: f32) -> Vec<Vec<PdfLine>> {
    let lines = lines(runs);
    let max_baseline = lines.last().map_or(0.0, |line| line.baseline_y.max(0.0));
    let pages = ((max_baseline / height).ceil() as usize).max(1);
    let mut result: Vec<Vec<PdfLine>> = (0..pages).map(|_| Vec::new()).collect();
    for line in lines {
        let page = ((line.baseline_y / height).floor() as usize).clamp(0, pages - 1);
        result[page].push(line);
    }
    result
}

/// Encodes a string as a PDF literal string (§7.3.4), WinAnsi without `/ToUnicode`.
///
/// ASCII (0x20–0x7E) and Latin-1 (0xA0–0xFF) are identical in WinAnsiEncoding;
/// characters outside that range degrade to `?` rather than emitting an
/// undefined code point. Parentheses and backslashes are escaped per §7.3.4.
fn pdf_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        let code = ch as u32;
        let byte = if (0x20..=0x7e).contains(&code) || (0xa0..=0xff).contains(&code) {
            code as u8
        } else {
            b'?'
        };
        if matches!(byte, b'(' | b')' | b'\\') {
            out.push('\\');
        }
        out.push(byte as char);
    }
    out
}

/// Builds one page's content stream: the raster background on page 1, then a
/// single ISO 32000-1 §9.4 text object holding that page's positioned lines.
///
/// CSS Paged Media 3 §4 margins inset the text from the `/MediaBox` edges; the
/// raster keeps its natural size and is not scaled to the page box.
pub(super) fn content_stream(
    width: u32,
    height: u32,
    page_index: usize,
    lines: &[PdfLine],
    page: &PageGeometry,
) -> String {
    let mut content = String::new();
    if page_index == 0 {
        content.push_str(&format!("q\n{width} 0 0 {height} 0 0 cm\n/Im0 Do\nQ\n"));
    }
    if !lines.is_empty() {
        content.push_str("BT\n");
        for line in lines {
            let local_baseline = line.baseline_y - page_index as f32 * page.content_height();
            let y = page.height - page.margin[0] - local_baseline;
            content.push_str(&format!(
                "/F1 {} Tf\n1 0 0 1 {} {} Tm\n({}) Tj\n",
                line.font_size,
                page.margin[3] + line.x,
                y,
                pdf_string(&line.text)
            ));
        }
        content.push_str("ET\n");
    }
    content
}
