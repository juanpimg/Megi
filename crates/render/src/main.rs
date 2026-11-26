#![forbid(unsafe_code)]
//! `megi-render` CLI: render a URL or local HTML file to a deterministic PNG.
//!
//! ```text
//! render <url|file> <out.png> [--width W] [--height H] [--base URL]
//!        [--no-network] [--no-scripting] [--system-fonts] [--cookies] [--origin ORIGIN] [--json]
//! ```
//!
//! Exit codes: `0` success, `1` render error (typed message on stderr), `2` usage error.

use std::path::PathBuf;
use std::process::ExitCode;

use megi_core::terminal_text::sanitize_terminal_text;
use megi_render::{RenderOptions, ScriptingLimits, ScriptingMode, Viewport, render_url};

mod dom_summary;

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
  --sw-offline   <url|file> is an HTML fixture: register a service worker,
                 cut the network, reload the cached document and write its PNG
  --no-scripting ignore <script> elements (no JS execution, no re-render)
  --system-fonts also load installed system fonts (generic families match Chrome)
  --cookies      enable a per-render cookie jar (Set-Cookie stored and sent to
                 subresources/redirects; also enables credentialed CORS)
  --origin URL   CORS Origin override for subresource fetches (default: the
                 document origin; cross-origin CORS failures are non-fatal)
  --json         print machine-readable metadata to stdout
  --script-budget small|real  JS budget profile (default: real; hard anti-DoS caps apply)
  -h, --help     print this help";

#[derive(Debug)]
struct Args {
    input: String,
    output: PathBuf,
    width: u32,
    height: u32,
    base: Option<String>,
    no_network: bool,
    sw_offline: bool,
    no_scripting: bool,
    system_fonts: bool,
    cookies: bool,
    origin: Option<String>,
    json: bool,
    script_budget: ScriptBudgetProfile,
    max_image_dim: Option<u32>,
    time_ms: Option<f64>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::Usage(message)) => {
            eprintln!("render: {}\n\n{USAGE}", sanitize_terminal_text(&message));
            ExitCode::from(2)
        }
        Err(CliError::Render(message)) => {
            eprintln!("render: {}", sanitize_terminal_text(&message));
            ExitCode::FAILURE
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ScriptBudgetProfile {
    #[default]
    Real,
    Small,
}

#[derive(Debug)]
enum CliError {
    Usage(String),
    Render(String),
}

fn run() -> Result<(), CliError> {
    let args = parse_args(std::env::args().skip(1))?;
    let viewport = Viewport::new(args.width, args.height)
        .map_err(|error| CliError::Usage(error.to_string()))?;
    if args.sw_offline {
        #[cfg(feature = "js")]
        return render_offline_cli(&args, viewport);
        #[cfg(not(feature = "js"))]
        return Err(CliError::Usage(
            "--sw-offline requires the `js` feature".to_owned(),
        ));
    }
    let options = RenderOptions {
        viewport,
        no_network: args.no_network,
        base_override: args.base.clone(),
        scripting: if args.no_scripting {
            ScriptingMode::Disabled
        } else {
            ScriptingMode::default()
        },
        system_fonts: args.system_fonts,
        cookies: args.cookies,
        origin: args.origin.clone(),
        max_image_dim: args.max_image_dim,
        time_ms: args.time_ms.unwrap_or(0.0),
        scripting_limits: match args.script_budget {
            ScriptBudgetProfile::Small => ScriptingLimits::default().clamped(),
            ScriptBudgetProfile::Real => ScriptingLimits::for_real_sites().clamped(),
        },
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
            "rendered {input} -> {} ({}x{}, {} bytes, {} display items, {} hit regions, {} images, {} background images, {} SVGs ({} inline), {} scripts + {} timer tasks / {} events, {} external stylesheets, {} referrer policy, {} ms)",
            args.output.display(),
            output.pixmap.width,
            output.pixmap.height,
            output.png.len(),
            stats.display_items,
            stats.hit_regions,
            stats.images_loaded,
            stats.background_images_loaded,
            stats.svgs_rendered,
            stats.inline_svgs,
            stats.scripts_executed,
            stats.script_tasks,
            stats.script_events,
            stats.external_stylesheets,
            stats.referrer_policy.as_deref().unwrap_or("-"),
            stats.timings.total.as_millis()
        );
        for warning in &stats.warnings {
            eprintln!("warning: {}", sanitize_terminal_text(warning));
        }
    }
    Ok(())
}

#[cfg(feature = "js")]
fn render_offline_cli(args: &Args, viewport: Viewport) -> Result<(), CliError> {
    let html = std::fs::read_to_string(&args.input)
        .map_err(|error| CliError::Render(format!("cannot read {}: {error}", args.input)))?;
    let reload = megi_render::offline_demo::render_sw_offline(&html, viewport)
        .map_err(|error| CliError::Render(error.to_string()))?;
    std::fs::write(&args.output, &reload.png).map_err(|error| {
        CliError::Render(format!("cannot write {}: {error}", args.output.display()))
    })?;
    if args.json {
        println!(
            "{{\"mode\":\"sw-offline\",\"png_bytes\":{},\"cached_bytes\":{},\"provider_calls_after_cut\":{},\"network_cut_verified\":{}}}",
            reload.png.len(),
            reload.cached.body.len(),
            reload.provider_calls_after_cut,
            if reload.network_cut_verified {
                "true"
            } else {
                "false"
            },
        );
    } else {
        println!(
            "offline reload -> {} ({} bytes PNG, {} cached bytes, {} provider calls after cut, network cut verified: {})",
            args.output.display(),
            reload.png.len(),
            reload.cached.body.len(),
            reload.provider_calls_after_cut,
            reload.network_cut_verified
        );
    }
    Ok(())
}

/// Accepts an absolute URL or a filesystem path (converted to a `file://` URL).
fn normalize_input(input: &str) -> Result<String, CliError> {
    if megi_net::Url::parse(input).is_ok() {
        return Ok(input.to_string());
    }
    let path = std::fs::canonicalize(input).map_err(|error| {
        CliError::Usage(format!(
            "cannot resolve `{input}` as a URL or file path: {error}"
        ))
    })?;
    let path = path.display().to_string();
    // Percent-encode path characters that would be misinterpreted as URL syntax (#1219).
    let encoded = path
        .replace('%', "%25")
        .replace('#', "%23")
        .replace('?', "%3F")
        .replace(' ', "%20");
    let url = format!("file://{encoded}");
    megi_net::Url::parse(&url)
        .map(|parsed| parsed.as_str())
        .map_err(|error| CliError::Usage(format!("cannot build file URL for {path}: {error}")))
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Args, CliError> {
    let mut positionals: Vec<String> = Vec::new();
    let mut width = DEFAULT_WIDTH;
    let mut height = DEFAULT_HEIGHT;
    let mut base = None;
    let mut no_network = false;
    let mut sw_offline = false;
    let mut no_scripting = false;
    let mut system_fonts = false;
    let mut cookies = false;
    let mut origin = None;
    let mut json = false;
    let mut max_image_dim = None;
    let mut time_ms = None;
    let mut script_budget = ScriptBudgetProfile::Real;
    let mut args = args;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            "--no-network" => no_network = true,
            "--sw-offline" => sw_offline = true,
            "--no-scripting" => no_scripting = true,
            "--system-fonts" => system_fonts = true,
            "--script-budget" => {
                let value = args.next().ok_or_else(|| {
                    CliError::Usage("--script-budget requires small|real".to_owned())
                })?;
                script_budget = match value.as_str() {
                    "small" => ScriptBudgetProfile::Small,
                    "real" => ScriptBudgetProfile::Real,
                    other => {
                        return Err(CliError::Usage(format!(
                            "unknown --script-budget profile `{other}` (use small|real)"
                        )));
                    }
                };
            }
            "--cookies" => cookies = true,
            "--origin" => {
                origin = Some(
                    args.next()
                        .ok_or_else(|| CliError::Usage("--origin requires a URL".to_string()))?,
                );
            }
            "--json" => json = true,
            "--max-image-dim" => {
                max_image_dim = Some(parse_dimension(&mut args, "--max-image-dim")?);
            }
            "--time" => {
                let value = args
                    .next()
                    .ok_or_else(|| CliError::Usage("--time requires a number".to_string()))?;
                let parsed: f64 = value
                    .parse()
                    .map_err(|_| CliError::Usage(format!("invalid --time value `{value}`")))?;
                if !parsed.is_finite() {
                    return Err(CliError::Usage(format!("invalid --time value `{value}`")));
                }
                time_ms = Some(parsed);
            }
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
        sw_offline,
        no_scripting,
        system_fonts,
        cookies,
        origin,
        json,
        script_budget,
        max_image_dim,
        time_ms,
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
fn metadata_json(input: &str, output: &megi_render::RenderOutput, args: &Args) -> String {
    let stats = &output.stats;
    let timings = &stats.timings;
    let warnings: Vec<String> = stats.warnings.iter().map(|w| json_string(w)).collect();
    let (element_count, body_text) = dom_summary::dom_summary(&output.document);
    format!(
        concat!(
            "{{\"input\":{},\"output\":{},\"width\":{},\"height\":{},",
            "\"document_url\":{},\"status\":{},\"charset\":{},",
            "\"element_count\":{},\"body_text\":{},",
            "\"document_bytes\":{},\"png_bytes\":{},\"display_items\":{},\"hit_regions\":{},",
            "\"cookies_stored\":{},\"content_encoding\":{},\"referrer_policy\":{},",
            "\"images_loaded\":{},\"images_failed\":{},\"images_capped\":{},\"image_bytes\":{},\"svgs_rendered\":{},\"inline_svgs\":{},\"inline_svgs_capped\":{},",
            "\"background_images_loaded\":{},\"background_images_failed\":{},\"background_bytes\":{},",
            "\"fonts_loaded\":{},\"fonts_failed\":{},\"font_bytes\":{},",
            "\"scripts_executed\":{},\"scripts_failed\":{},\"script_bytes\":{},\"console_entries\":{},\"console_errors\":{},",
            "\"script_events\":{},\"script_tasks\":{},\"script_loop_iterations\":{},",
            "\"script_virtual_time_ms\":{},\"script_timers_pending\":{},",
            "\"location_actions\":{},\"history_actions\":{},\"pushed_used\":{},\"pushed_discarded\":{},",
            "\"external_stylesheets\":{},\"inline_stylesheets\":{},\"css_bytes\":{},",
            "\"timings_us\":{{\"fetch\":{},\"decode_parse\":{},\"styles\":{},\"styles_pre_script\":{},\"fonts\":{},\"images\":{},",
            "\"backgrounds\":{},\"scripting\":{},\"event_loop\":{},\"layout\":{},",
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
        element_count,
        json_string(&body_text),
        stats.document_bytes,
        output.png.len(),
        stats.display_items,
        stats.hit_regions,
        stats.cookies_stored,
        stats
            .content_encoding
            .as_deref()
            .map_or_else(|| "null".to_string(), json_string),
        stats
            .referrer_policy
            .as_deref()
            .map_or_else(|| "null".to_string(), json_string),
        stats.images_loaded,
        stats.images_failed,
        stats.images_capped,
        stats.image_bytes,
        stats.svgs_rendered,
        stats.inline_svgs,
        stats.inline_svgs_capped,
        stats.background_images_loaded,
        stats.background_images_failed,
        stats.background_bytes,
        stats.fonts_loaded,
        stats.fonts_failed,
        stats.font_bytes,
        stats.scripts_executed,
        stats.scripts_failed,
        stats.script_bytes,
        stats.console.len(),
        stats.console_errors,
        stats.script_events,
        stats.script_tasks,
        stats.script_loop_iterations,
        stats.script_virtual_time_ms,
        stats.script_timers_pending,
        stats.location_actions,
        stats.history_actions,
        stats.pushed_used,
        stats.pushed_discarded,
        stats.external_stylesheets,
        stats.inline_stylesheets,
        stats.css_bytes,
        timings.fetch.as_micros(),
        timings.decode_parse.as_micros(),
        timings.styles.as_micros(),
        timings.styles_pre_script.as_micros(),
        timings.fonts.as_micros(),
        timings.images.as_micros(),
        timings.backgrounds.as_micros(),
        timings.scripting.as_micros(),
        timings.event_loop.as_micros(),
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

#[cfg(test)]
mod script_budget_cli_tests {
    use super::*;

    fn args(values: &[&str]) -> impl Iterator<Item = String> {
        values.iter().map(|v| (*v).to_owned())
    }

    #[test]
    fn script_budget_flag_defaults_to_real_and_parses_small() {
        let default = parse_args(args(&["data:text/html,x", "/tmp/x.png"])).expect("default");
        assert_eq!(default.script_budget, ScriptBudgetProfile::Real);
        let small = parse_args(args(&[
            "data:text/html,x",
            "/tmp/x.png",
            "--script-budget",
            "small",
        ]))
        .expect("small");
        assert_eq!(small.script_budget, ScriptBudgetProfile::Small);
    }

    #[test]
    fn script_budget_flag_rejects_unknown_profiles() {
        let err = parse_args(args(&[
            "data:text/html,x",
            "/tmp/x.png",
            "--script-budget",
            "huge",
        ]))
        .expect_err("unknown profile");
        assert!(format!("{err:?}").contains("unknown --script-budget"));
    }

    #[test]
    fn system_fonts_flag_is_opt_in() {
        let base = parse_args(args(&["data:text/html,x", "/tmp/x.png"])).expect("default");
        assert!(!base.system_fonts, "system fonts must be opt-in");
        let on = parse_args(args(&["data:text/html,x", "/tmp/x.png", "--system-fonts"]))
            .expect("--system-fonts must be accepted");
        assert!(on.system_fonts);
    }
}
