//! `megi` — Megi browser shell CLI.
//!
//! ```text
//! megi <url|file> [--width W] [--height H] [--screenshot PATH] [--title T]
//! ```
//!
//! Without `--screenshot` this creates a real winit window (requires an X11/Wayland
//! session). With `--screenshot` it renders headlessly and writes a PNG, which is what CI
//! uses: no `EventLoop`, no display server.

use std::process::ExitCode;

use shell::{LaunchMode, ShellError, finish_args, parse_args, screenshot};

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
    match args.mode {
        LaunchMode::Screenshot(path) => {
            let info = screenshot(&args.input, args.width, args.height, &path)?;
            println!(
                "wrote {} ({}x{}, {} bytes)",
                info.path.display(),
                info.width,
                info.height,
                info.png_bytes
            );
            Ok(())
        }
        LaunchMode::Window => shell::run_window(args.input, args.title, args.width, args.height),
    }
}
