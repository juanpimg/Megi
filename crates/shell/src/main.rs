//! `megi` — Megi browser shell CLI.
//!
//! ```text
//! megi <url|file> [--width W] [--height H] [--screenshot PATH] [--title T]
//!      [--cookies] [--origin URL] [--zoom FACTOR] [--refresh-ms [MS]]
//! ```
//!
//! Without `--screenshot` this creates a real winit window (requires an X11/Wayland
//! session). With `--screenshot` it renders headlessly and writes a PNG, which is what CI
//! uses: no `EventLoop`, no display server.

use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use shell::{
    CliArgs, LaunchMode, RefreshPolicy, ShellError, WindowViewOptions, finish_args, parse_args,
    screenshot_with_options, zoom,
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
    );
    match args.mode {
        LaunchMode::Screenshot(path) => {
            // Headless screenshots render at the CSS viewport (physical / zoom): `--zoom 2`
            // writes a PNG half the requested physical size (the window would upscale it).
            let (css_width, css_height) =
                zoom::zoomed_viewport(args.width, args.height, 1.0, args.zoom);
            let info =
                screenshot_with_options(&args.input, css_width, css_height, &path, &options)?;
            println!(
                "wrote {} ({}x{}, {} bytes, zoom {})",
                info.path.display(),
                info.width,
                info.height,
                info.png_bytes,
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
