#![forbid(unsafe_code)]
//! `megi` — Megi browser shell CLI.
//!
//! ```text
//! megi <url|file> [--width W] [--height H] [--screenshot PATH] [--print-to-pdf PATH] [--title T]
//!      [--cookies] [--origin URL] [--zoom FACTOR] [--refresh-ms [MS]] [--find QUERY] [--chrome] [--tabs URL]...
//! ```
//!
//! Without `--screenshot` this creates a real winit window (requires an X11/Wayland
//! session). With `--screenshot` it renders headlessly and writes a PNG, which is what CI
//! uses: no `EventLoop`, no display server.

use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use megi_browser::{
    CliArgs, LaunchMode, RefreshPolicy, ShellError, WindowViewOptions, finish_args, parse_args,
    sanitize_terminal_text, zoom,
};

/// Builds the render options for one CLI invocation.
///
/// `--cookies` installs a **session** cookie jar shared by every render of the process
/// (document, subresources, redirects and later navigations in window mode).
/// `--origin` overrides the CORS `Origin` used for subresource fetches (otherwise the
/// document's own origin is used automatically by `render`).
fn render_options(args: &CliArgs) -> megi_webview::render::RenderOptions {
    let cookie_jar = args
        .cookies
        .then(|| Arc::new(Mutex::new(megi_webview::net::CookieJar::new())));
    megi_webview::render::RenderOptions {
        cookie_jar,
        origin: args.origin.clone(),
        time_ms: args.time_ms,
        ..megi_webview::render::RenderOptions::default()
    }
}

fn main() -> ExitCode {
    // ADR-004 phase 3: child renderer mode, driven over safe length-prefixed JSON IPC.
    if std::env::args().any(|arg| arg == "--megi-ipc-worker") {
        return match megi_browser::ipc::run_ipc_worker() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!(
                    "megi renderer: {}",
                    sanitize_terminal_text(&error.to_string())
                );
                ExitCode::FAILURE
            }
        };
    }
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(ShellError::Usage(message)) => {
            eprintln!("megi: {}", sanitize_terminal_text(&message));
            ExitCode::from(2)
        }
        Err(error) => {
            eprintln!("megi: {}", sanitize_terminal_text(&error.to_string()));
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), ShellError> {
    let raw = parse_args(std::env::args().skip(1))?;
    if raw.help {
        println!("{}", megi_browser::cli::USAGE);
        return Ok(());
    }
    let args = finish_args(raw)?;
    // ADR-004: the process backend returns only the child's PNG, so overlays that need the
    // parent's display list cannot be composed. Reject the combination clearly instead of
    // silently returning an image without the requested chrome/find highlights.
    if args.ipc_render && (args.chrome || args.chrome_menu || args.find.is_some()) {
        return Err(ShellError::Usage(
            "--ipc-render cannot be combined with --chrome, --chrome-menu or --find: \
             those overlays require the in-process renderer"
                .to_owned(),
        ));
    }
    // M7 foundation: opt-in DevTools endpoint, bound to loopback only. The handle lives
    // until the process exits so requests are served while the window event loop (or a
    // headless render) runs. `full` supplies the JS runtime used by `Runtime.evaluate`.
    #[cfg(feature = "full")]
    let _devtools = match args.remote_debugging_port {
        Some(port) => Some(
            megi_browser::devtools::spawn(
                port,
                megi_browser::devtools::DebugTarget {
                    title: args.title.clone(),
                    url: args.input.clone(),
                },
            )
            .map_err(|reason| ShellError::Io {
                path: format!("127.0.0.1:{port}"),
                reason,
            })?,
        ),
        None => None,
    };
    #[cfg(not(feature = "full"))]
    if args.remote_debugging_port.is_some() {
        return Err(ShellError::Usage(
            "--remote-debugging-port requires the `full` feature".to_owned(),
        ));
    }
    let options = render_options(&args);
    #[cfg(feature = "full")]
    let _webdriver_bidi = match args.webdriver_bidi_port {
        Some(port) => Some(
            megi_browser::webdriver::spawn(port, options.clone()).map_err(|reason| {
                ShellError::Io {
                    path: format!("127.0.0.1:{port}"),
                    reason,
                }
            })?,
        ),
        None => None,
    };
    #[cfg(not(feature = "full"))]
    if args.webdriver_bidi_port.is_some() {
        return Err(ShellError::Usage(
            "--webdriver-bidi-port requires the `full` feature".to_owned(),
        ));
    }
    let view = WindowViewOptions::new(
        args.zoom,
        args.refresh_ms
            .map_or_else(RefreshPolicy::disabled, RefreshPolicy::every_ms),
    )
    .with_tabs(args.tabs.clone())
    .with_process_render(args.ipc_render)
    .with_reduced_motion(args.reduced_motion);
    let mut view = view;
    view.session_path = if args.no_session {
        None
    } else {
        megi_browser::session::resolve_session_path(args.session_file.as_deref())
    };
    view.download_dir = args.download_dir.clone();
    match args.mode {
        LaunchMode::Screenshot(path) => {
            // Headless screenshots render at the CSS viewport (physical / zoom): `--zoom 2`
            // writes a PNG half the requested physical size (the window would upscale it).
            let (css_width, css_height) =
                zoom::zoomed_viewport(args.width, args.height, 1.0, args.zoom);

            // #1689: with `--tabs`, headless renders the selected tab (0-based `--active-tab`),
            // not always the positional first one. This makes `--tabs` meaningful in screenshots.
            let active_url = view
                .tabs
                .get(args.active_tab)
                .cloned()
                .unwrap_or_else(|| args.input.clone());
            // `--reader` extracts the article from the source page first and renders the
            // generated reader data URL; chrome/find then apply to that clean page.
            let input = if args.reader {
                megi_browser::headless::reader_url(&active_url, css_width, css_height, &options)?
            } else {
                active_url
            };
            // ADR-004 phase 3: opt-in process isolation for headless renders. The child
            // returns only the serializable PNG payload; chrome/find overlays are a parent
            // concern and are therefore incompatible with this mode (documented limitation).
            if args.ipc_render {
                let png = megi_browser::ipc::render_png_in_process_with_options(
                    &input, css_width, css_height, &options,
                )
                .map_err(|error| ShellError::Io {
                    path: "ipc-render".to_owned(),
                    reason: error.to_string(),
                })?;
                if png.is_empty() {
                    return Err(ShellError::Io {
                        path: "ipc-render".to_owned(),
                        reason: "renderer produced no PNG".to_owned(),
                    });
                }
                std::fs::write(&path, &png).map_err(|error| ShellError::Io {
                    path: path.display().to_string(),
                    reason: error.to_string(),
                })?;
                return Ok(());
            }
            let output = if args.chrome && args.chrome_menu {
                megi_browser::headless::render_target_with_chrome_menu(
                    &input,
                    css_width,
                    css_height,
                    &options,
                    args.find.as_deref(),
                )?
            } else if args.chrome && args.chrome_compact {
                megi_browser::headless::render_target_with_chrome_compact(
                    &input,
                    css_width,
                    css_height,
                    &options,
                    args.find.as_deref(),
                )?
            } else if args.chrome {
                megi_browser::headless::render_target_with_chrome(
                    &input,
                    css_width,
                    css_height,
                    &options,
                    args.find.as_deref(),
                )?
            } else if let Some(query) = &args.find {
                let (output, _hits) = megi_browser::headless::render_target_with_find(
                    &input, css_width, css_height, &options, query,
                )?;
                output
            } else {
                megi_browser::headless::render_target_with_options(
                    &input, css_width, css_height, &options,
                )?
            };
            // The render either produced the PNG eagerly (plain path) or skipped it and the
            // overlay helpers encoded it once at the end (chrome/find). Reuse those bytes so
            // exactly one encode happens per screenshot (R201).
            // M7: attach the real loaded document to the DevTools target so DOM/CSS commands
            // and `Runtime.evaluate` run against the page arena, not the isolated realm.
            #[cfg(feature = "full")]
            if let Some(endpoint) = &_devtools {
                endpoint.attach(megi_browser::devtools::DevToolsPage::from_render(&output));
            }
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
            megi_browser::download::write_png_bytes_to(&path, &png).map_err(|reason| {
                ShellError::Io {
                    path: path.display().to_string(),
                    reason,
                }
            })?;
            if let Some(save_path) = &args.save_png {
                let bytes = megi_browser::download::write_png_bytes_to(save_path, &png).map_err(
                    |reason| ShellError::Io {
                        path: save_path.display().to_string(),
                        reason,
                    },
                )?;
                println!("saved png ({bytes} bytes) to {}", save_path.display());
            }
            if let Some(save_path) = &args.save_html {
                let bytes = megi_browser::download::write_html_to(save_path, &output.document)
                    .map_err(|reason| ShellError::Io {
                        path: save_path.display().to_string(),
                        reason,
                    })?;
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
            // A degraded capture must be visible to CI, not reported as clean (#1457).
            let violations = output.stats.limit_violations.total();
            for warning in &output.stats.warnings {
                // #2309: warnings carry page-derived console/script text.
                eprintln!("warning: {}", sanitize_terminal_text(warning));
            }
            if violations > 0 {
                eprintln!("warning: {violations} parser/layout limit violation(s) during render");
            }
            if violations > 0 {
                return Err(ShellError::Io {
                    path: path.display().to_string(),
                    reason: format!("{violations} parser/layout limit violation(s) during render"),
                });
            }
            Ok(())
        }
        LaunchMode::PrintPdf(path) => {
            // Same CSS-viewport rule as --screenshot: --zoom 2 yields a PDF page
            // half the requested physical size (the window would upscale it).
            let (css_width, css_height) =
                zoom::zoomed_viewport(args.width, args.height, 1.0, args.zoom);
            let active_url = view
                .tabs
                .get(args.active_tab)
                .cloned()
                .unwrap_or_else(|| args.input.clone());
            let info = megi_browser::pdf::print_pdf_with_options(
                &active_url,
                css_width,
                css_height,
                &path,
                &options,
            )?;
            println!(
                "wrote {} ({}x{}, {} bytes)",
                info.path.display(),
                info.width,
                info.height,
                info.pdf_bytes
            );
            Ok(())
        }
        LaunchMode::Window => megi_browser::run_window_with_view_options(
            args.input,
            args.title,
            args.width,
            args.height,
            options,
            view,
        ),
    }
}

#[cfg(test)]
mod screenshot_diagnostics_tests {

    #[test]
    fn render_diagnostics_are_detected() {
        // A screenshot with warnings or limit violations must be treated as degraded.
        let mut violations = megi_webview::html::LimitViolations::default();
        let mut warnings: Vec<String> = Vec::new();
        assert!(
            warnings.is_empty() && violations.is_empty(),
            "clean baseline"
        );
        warnings.push("skipping image: failed".into());
        assert!(!warnings.is_empty());
        violations.nodes_exceeded = 1;
        assert!(violations.total() > 0);
    }
}
