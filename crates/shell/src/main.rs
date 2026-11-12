//! `megi` — Megi browser shell CLI.
//!
//! ```text
//! megi <url|file> [--width W] [--height H] [--screenshot PATH] [--title T]
//!      [--cookies] [--origin URL] [--zoom FACTOR] [--refresh-ms [MS]] [--find QUERY] [--chrome] [--tabs URL]...
//! ```
//!
//! Without `--screenshot` this creates a real winit window (requires an X11/Wayland
//! session). With `--screenshot` it renders headlessly and writes a PNG, which is what CI
//! uses: no `EventLoop`, no display server.

use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use shell::{
    CliArgs, LaunchMode, RefreshPolicy, ShellError, WindowViewOptions, finish_args, parse_args,
    zoom,
};

/// Builds the render options for one CLI invocation.
///
/// `--cookies` installs a **session** cookie jar shared by every render of the process
/// (document, subresources, redirects and later navigations in window mode).
/// `--origin` overrides the CORS `Origin` used for subresource fetches (otherwise the
/// document's own origin is used automatically by `render`).
fn render_options(args: &CliArgs) -> render::RenderOptions {
    let cookie_jar = args
        .cookies
        .then(|| Arc::new(Mutex::new(net::CookieJar::new())));
    render::RenderOptions {
        cookie_jar,
        origin: args.origin.clone(),
        time_ms: args.time_ms,
        ..render::RenderOptions::default()
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(ShellError::Usage(message)) => {
            eprintln!("megi: {message}");
            ExitCode::from(2)
        }
        Err(error) => {
            eprintln!("megi: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), ShellError> {
    let raw = parse_args(std::env::args().skip(1))?;
    if raw.help {
        println!("{}", shell::cli::USAGE);
        return Ok(());
    }
    let args = finish_args(raw)?;
    let options = render_options(&args);
    let view = WindowViewOptions::new(
        args.zoom,
        args.refresh_ms
            .map_or_else(RefreshPolicy::disabled, RefreshPolicy::every_ms),
    )
    .with_tabs(args.tabs.clone());
    let mut view = view;
    view.session_path = if args.no_session {
        None
    } else {
        shell::session::resolve_session_path(args.session_file.as_deref())
    };
    view.download_dir = args.download_dir.clone();
    match args.mode {
        LaunchMode::Screenshot(path) => {
            // Headless screenshots render at the CSS viewport (physical / zoom): `--zoom 2`
            // writes a PNG half the requested physical size (the window would upscale it).
            let (css_width, css_height) =
                zoom::zoomed_viewport(args.width, args.height, 1.0, args.zoom);

            // `--reader` extracts the article from the source page first and renders the
            // generated reader data URL; chrome/find then apply to that clean page.
            let input = if args.reader {
                shell::headless::reader_url(&args.input, css_width, css_height, &options)?
            } else {
                args.input.clone()
            };
            let output = if args.chrome && args.chrome_menu {
                shell::headless::render_target_with_chrome_menu(
                    &input,
                    css_width,
                    css_height,
                    &options,
                    args.find.as_deref(),
                )?
            } else if args.chrome {
                shell::headless::render_target_with_chrome(
                    &input,
                    css_width,
                    css_height,
                    &options,
                    args.find.as_deref(),
                )?
            } else if let Some(query) = &args.find {
                let (output, _hits) = shell::headless::render_target_with_find(
                    &input, css_width, css_height, &options, query,
                )?;
                output
            } else {
                shell::headless::render_target_with_options(
                    &input, css_width, css_height, &options,
                )?
            };
            // The render either produced the PNG eagerly (plain path) or skipped it and the
            // overlay helpers encoded it once at the end (chrome/find). Reuse those bytes so
            // exactly one encode happens per screenshot (R201).
            let png = if output.png.is_empty() {
                output.pixmap.to_png()
            } else {
                output.png.clone()
            };
            if png.is_empty() {
                return Err(ShellError::Io {
                    path: path.display().to_string(),
                    reason: "the render produced no PNG bytes".to_string(),
                });
            }
            shell::download::write_png_bytes_to(&path, &png).map_err(|reason| ShellError::Io {
                path: path.display().to_string(),
                reason,
            })?;
            if let Some(save_path) = &args.save_png {
                let bytes =
                    shell::download::write_png_bytes_to(save_path, &png).map_err(|reason| {
                        ShellError::Io {
                            path: save_path.display().to_string(),
                            reason,
                        }
                    })?;
                println!("saved png ({bytes} bytes) to {}", save_path.display());
            }
            if let Some(save_path) = &args.save_html {
                let bytes = shell::download::write_html_to(save_path, &output.document).map_err(
                    |reason| ShellError::Io {
                        path: save_path.display().to_string(),
                        reason,
                    },
                )?;
                println!("saved html ({bytes} bytes) to {}", save_path.display());
            }
            println!(
                "wrote {} ({}x{}, {} bytes, zoom {})",
                path.display(),
                output.pixmap.width,
                output.pixmap.height,
                png.len(),
                zoom::zoom_label(args.zoom)
            );
            Ok(())
        }
        LaunchMode::Window => shell::run_window_with_view_options(
            args.input,
            args.title,
            args.width,
            args.height,
            options,
            view,
        ),
    }
}
