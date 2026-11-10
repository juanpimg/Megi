//! `megi-render` CLI: render a URL or local HTML file to a deterministic PNG.
//!
//! ```text
//! render <url|file> <out.png> [--width W] [--height H] [--base URL]
//!        [--no-network] [--json]
//! ```
//!
//! Exit codes: `0` success, `1` render error (typed message on stderr), `2` usage error.

use std::path::PathBuf;
use std::process::ExitCode;

use render::{RenderOptions, Viewport, render_url};

const DEFAULT_WIDTH: u32 = 800;
const DEFAULT_HEIGHT: u32 = 600;

const USAGE: &str = "\
usage: render <url|file> <out.png> [options]

  <url|file>     absolute URL (http/https/file/data) or a local HTML file path
  <out.png>      output PNG path

options:
  --width W      viewport width in CSS pixels (default 800, max 8192)
  --height H     viewport height in CSS pixels (default 600, max 8192)
  --base URL     override the base URL used to resolve relative resources
  --no-network   refuse http(s) resources (only file:// and data: are loaded)
  --json         print machine-readable metadata to stdout
  -h, --help     print this help";

struct Args {
    input: String,
    output: PathBuf,
    width: u32,
    height: u32,
    base: Option<String>,
    no_network: bool,
    json: bool,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::Usage(message)) => {
            eprintln!("render: {message}\n\n{USAGE}");
            ExitCode::from(2)
        }
        Err(CliError::Render(message)) => {
            eprintln!("render: {message}");
            ExitCode::FAILURE
        }
    }
}

enum CliError {
    Usage(String),
    Render(String),
}

fn run() -> Result<(), CliError> {
    let args = parse_args(std::env::args().skip(1))?;
    let viewport = Viewport::new(args.width, args.height)
        .map_err(|error| CliError::Usage(error.to_string()))?;
    let options = RenderOptions {
        viewport,
        no_network: args.no_network,
        base_override: args.base.clone(),
        ..RenderOptions::default()
    };

    let input = normalize_input(&args.input)?;
    let output =
        render_url(&input, &options).map_err(|error| CliError::Render(error.to_string()))?;

    std::fs::write(&args.output, &output.png).map_err(|error| {
        CliError::Render(format!("cannot write {}: {error}", args.output.display()))
    })?;

    if args.json {
        println!("{}", metadata_json(&input, &output, &args));
    } else {
        let stats = &output.stats;
        println!(
            "rendered {input} -> {} ({}x{}, {} bytes, {} display items, {} external stylesheets, {} ms)",
            args.output.display(),
            output.pixmap.width,
            output.pixmap.height,
            output.png.len(),
            stats.display_items,
            stats.external_stylesheets,
            stats.timings.total.as_millis()
        );
        for warning in &stats.warnings {
            eprintln!("warning: {warning}");
        }
    }
    Ok(())
}

/// Accepts an absolute URL or a filesystem path (converted to a `file://` URL).
fn normalize_input(input: &str) -> Result<String, CliError> {
    if net::Url::parse(input).is_ok() {
        return Ok(input.to_string());
    }
    let path = std::fs::canonicalize(input).map_err(|error| {
        CliError::Usage(format!(
            "cannot resolve `{input}` as a URL or file path: {error}"
        ))
    })?;
    let path = path.display().to_string();
    // The URL parser percent-encodes spaces and non-ASCII UTF-8 path bytes.
    let url = format!("file://{path}");
    net::Url::parse(&url)
        .map(|parsed| parsed.as_str())
        .map_err(|error| CliError::Usage(format!("cannot build file URL for {path}: {error}")))
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Args, CliError> {
    let mut positionals: Vec<String> = Vec::new();
    let mut width = DEFAULT_WIDTH;
    let mut height = DEFAULT_HEIGHT;
    let mut base = None;
    let mut no_network = false;
    let mut json = false;
    let mut args = args;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            "--no-network" => no_network = true,
            "--json" => json = true,
            "--width" => {
                width = parse_dimension(&mut args, "--width")?;
            }
            "--height" => {
                height = parse_dimension(&mut args, "--height")?;
            }
            "--base" => {
                let value = args
                    .next()
                    .ok_or_else(|| CliError::Usage("--base requires a URL".to_string()))?;
                base = Some(value);
            }
            other if other.starts_with('-') => {
                return Err(CliError::Usage(format!("unknown option `{other}`")));
            }
            other => positionals.push(other.to_string()),
        }
    }
    if positionals.len() != 2 {
        return Err(CliError::Usage(format!(
            "expected <url|file> <out.png>, got {} positional argument(s)",
            positionals.len()
        )));
    }
    let output = PathBuf::from(&positionals[1]);
    Ok(Args {
        input: positionals[0].clone(),
        output,
        width,
        height,
        base,
        no_network,
        json,
    })
}

fn parse_dimension(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<u32, CliError> {
    let value = args
        .next()
        .ok_or_else(|| CliError::Usage(format!("{flag} requires a pixel value")))?;
    value
        .parse::<u32>()
        .map_err(|_| CliError::Usage(format!("invalid {flag} value `{value}`")))
}

/// Serializes render metadata as JSON (no serde dependency for a single object).
fn metadata_json(input: &str, output: &render::RenderOutput, args: &Args) -> String {
    let stats = &output.stats;
    let timings = &stats.timings;
    let warnings: Vec<String> = stats.warnings.iter().map(|w| json_string(w)).collect();
    format!(
        concat!(
            "{{\"input\":{},\"output\":{},\"width\":{},\"height\":{},",
            "\"document_url\":{},\"status\":{},\"charset\":{},",
            "\"document_bytes\":{},\"png_bytes\":{},\"display_items\":{},",
            "\"external_stylesheets\":{},\"inline_stylesheets\":{},\"css_bytes\":{},",
            "\"timings_us\":{{\"fetch\":{},\"decode_parse\":{},\"styles\":{},\"layout\":{},",
            "\"paint\":{},\"raster\":{},\"total\":{}}},",
            "\"warnings\":[{}]}}"
        ),
        json_string(input),
        json_string(&args.output.display().to_string()),
        output.pixmap.width,
        output.pixmap.height,
        json_string(&output.document_url.as_str()),
        stats
            .status
            .map_or_else(|| "null".to_string(), |status| status.to_string()),
        json_string(&stats.charset),
        stats.document_bytes,
        output.png.len(),
        stats.display_items,
        stats.external_stylesheets,
        stats.inline_stylesheets,
        stats.css_bytes,
        timings.fetch.as_micros(),
        timings.decode_parse.as_micros(),
        timings.styles.as_micros(),
        timings.layout.as_micros(),
        timings.paint.as_micros(),
        timings.raster.as_micros(),
        timings.total.as_micros(),
        warnings.join(","),
    )
}

/// Minimal JSON string escaping (quotes, backslashes and control characters).
fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other if (other as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", other as u32));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}
