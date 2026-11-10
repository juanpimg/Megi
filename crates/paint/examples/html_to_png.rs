//! # `html_to_png` — Phase 4 end-to-end pipeline driver
//!
//! First *real* HTML → PNG pipeline of the Megi engine (WP-4.1):
//!
//! ```text
//! HTML bytes ──html::parse_document──► DOM
//!            ──paint::collect_style_text + css::resolve_document_styles──► styles
//!            ──layout::layout_document──► box tree
//!            ──paint::build_display_list──► display list
//!            ──paint::rasterize_display_list──► Pixmap
//!            ──Pixmap::to_png──► PNG file
//! ```
//!
//! ## Usage
//!
//! ```text
//! cargo run -p paint --example html_to_png -- <input.html> <output.png> [width] [height] [--rgba <dump.rgba>]
//! ```
//!
//! - `width`/`height` default to 800×600 (the Phase 4 fixture viewport).
//! - `--rgba <path>` additionally dumps the raw row-major RGBA bytes used by
//!   the golden-image tests (`tests/fixtures/phase4/basic.rgba`).
//!
//! `html` is a dev-dependency on purpose: the production `paint` crate must not
//! gain an `html` dependency (audit decision D8), so the orchestration lives in
//! this example (and in the integration tests).
//!
//! The canvas is painted by `paint::build_display_list` following CSS 2.2
//! §14.2; the rasterizer starts from an opaque white canvas, so pages without
//! any root/`body` background still produce a white PNG instead of a
//! transparent one.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use layout::{FontTextMeasurer, Rect, layout_document_with_measurer};

const DEFAULT_WIDTH: u32 = 800;
const DEFAULT_HEIGHT: u32 = 600;
/// Upper bound to avoid absurd allocations from typo'd arguments.
const MAX_DIMENSION: u32 = 8192;

const USAGE: &str = "\
usage: html_to_png <input.html> <output.png> [width] [height] [--rgba <dump.rgba>]

  width/height   viewport size in CSS pixels (default 800x600)
  --rgba <path>  also write the raw RGBA bytes of the render (golden images)
  -h, --help     print this help";

struct Options {
    input: PathBuf,
    output: PathBuf,
    width: u32,
    height: u32,
    rgba_dump: Option<PathBuf>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("html_to_png: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let options = parse_options(env::args().skip(1))?;

    let source = fs::read_to_string(&options.input)
        .map_err(|error| format!("cannot read {}: {error}", options.input.display()))?;

    let doc = html::parse_document(&source, false);
    let styles = paint::resolve_embedded_styles(&doc);

    let viewport = Rect::new(0.0, 0.0, options.width as f32, options.height as f32);
    // Deterministic real typography: bundled DejaVu Sans, no system fonts.
    let measurer = FontTextMeasurer::bundled()
        .map_err(|error| format!("cannot load bundled fonts: {error}"))?;
    let layout_root = layout_document_with_measurer(&doc, &styles, viewport, &measurer)
        .ok_or_else(|| {
            format!(
                "layout produced no root box for {}",
                options.input.display()
            )
        })?;

    let display_list = paint::build_display_list(&layout_root, viewport);
    let pixmap = paint::rasterize_display_list(&display_list, options.width, options.height);
    let png = pixmap.to_png();
    if png.is_empty() {
        return Err("PNG encoding failed (empty output)".to_string());
    }

    fs::write(&options.output, &png)
        .map_err(|error| format!("cannot write {}: {error}", options.output.display()))?;

    println!(
        "wrote {} ({}x{}, {} bytes, {} display items)",
        options.output.display(),
        options.width,
        options.height,
        png.len(),
        display_list.len()
    );

    if let Some(rgba_path) = &options.rgba_dump {
        let rgba = pixmap.to_rgba_bytes();
        fs::write(rgba_path, &rgba)
            .map_err(|error| format!("cannot write {}: {error}", rgba_path.display()))?;
        println!("wrote {} ({} RGBA bytes)", rgba_path.display(), rgba.len());
    }

    Ok(())
}

/// Parses the CLI arguments.
///
/// Positional arguments are, in order: `<input.html> <output.png> [width]
/// [height]`. `--rgba <path>` and `-h`/`--help` are the only flags.
fn parse_options(args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut positionals: Vec<String> = Vec::new();
    let mut rgba_dump = None;

    let mut args = args;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            "--rgba" => {
                let path = args
                    .next()
                    .ok_or_else(|| "--rgba requires a path".to_string())?;
                rgba_dump = Some(PathBuf::from(path));
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown option {other:?}\n{USAGE}"));
            }
            other => positionals.push(other.to_string()),
        }
    }

    if positionals.len() < 2 {
        return Err(format!("missing <input.html> or <output.png>\n{USAGE}"));
    }
    if positionals.len() > 4 {
        return Err(format!("unexpected extra argument {:?}", positionals[4]));
    }

    let parse_dimension = |value: &str, name: &str| -> Result<u32, String> {
        let parsed = value
            .parse::<u32>()
            .map_err(|_| format!("invalid {name} {value:?}"))?;
        if !(1..=MAX_DIMENSION).contains(&parsed) {
            return Err(format!("{name} {parsed} out of range 1..={MAX_DIMENSION}"));
        }
        Ok(parsed)
    };

    let width = match positionals.get(2) {
        Some(value) => parse_dimension(value, "width")?,
        None => DEFAULT_WIDTH,
    };
    let height = match positionals.get(3) {
        Some(value) => parse_dimension(value, "height")?,
        None => DEFAULT_HEIGHT,
    };

    Ok(Options {
        input: PathBuf::from(&positionals[0]),
        output: PathBuf::from(&positionals[1]),
        width,
        height,
        rgba_dump,
    })
}
