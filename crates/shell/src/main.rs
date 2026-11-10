//! `megi` — Megi browser shell CLI.
//!
//! ```text
//! megi <url|file> [--width W] [--height H] [--screenshot PATH] [--title T]
//!      [--cookies] [--origin URL]
//! ```
//!
//! Without `--screenshot` this creates a real winit window (requires an X11/Wayland
//! session). With `--screenshot` it renders headlessly and writes a PNG, which is what CI
//! uses: no `EventLoop`, no display server.

use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use shell::{CliArgs, LaunchMode, ShellError, finish_args, parse_args, screenshot_with_options};

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
    match args.mode {
        LaunchMode::Screenshot(path) => {
            let info =
                screenshot_with_options(&args.input, args.width, args.height, &path, &options)?;
            println!(
                "wrote {} ({}x{}, {} bytes)",
                info.path.display(),
                info.width,
                info.height,
                info.png_bytes
            );
            Ok(())
        }
        LaunchMode::Window => {
            shell::run_window_with_options(args.input, args.title, args.width, args.height, options)
        }
    }
}
