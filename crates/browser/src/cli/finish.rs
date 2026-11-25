use super::*;

/// Parses and validates the full CLI configuration (`--help` is an error here).
pub fn parse_cli(args: impl Iterator<Item = String>) -> Result<CliArgs, ShellError> {
    finish_args(parse_args(args)?)
}

/// Applies defaults/validation/normalization to [`RawArgs`].
///
/// The binary calls [`parse_args`] first so it can handle `--help` before this step.
pub fn finish_args(raw: RawArgs) -> Result<CliArgs, ShellError> {
    if raw.help {
        return Err(ShellError::Usage(USAGE.to_string()));
    }
    const MAX_TABS: usize = 32;
    const MAX_INPUT_BYTES: usize = 4096;
    if raw.tabs.len() > MAX_TABS {
        return Err(ShellError::Usage(format!(
            "--tabs count exceeds {MAX_TABS}"
        )));
    }
    let mut extra_tabs = Vec::with_capacity(raw.tabs.len());
    for tab in &raw.tabs {
        if tab.len() > MAX_INPUT_BYTES {
            return Err(ShellError::Usage(format!(
                "input too long (max {MAX_INPUT_BYTES} bytes)"
            )));
        }
        extra_tabs.push(normalize_input(tab)?);
    }
    if let Some(ref find) = raw.find
        && find.len() > MAX_INPUT_BYTES
    {
        return Err(ShellError::Usage(format!(
            "--find value too long (max {MAX_INPUT_BYTES} bytes)"
        )));
    }
    if let Some(ref title) = raw.title
        && title.len() > MAX_INPUT_BYTES
    {
        return Err(ShellError::Usage(format!(
            "--title value too long (max {MAX_INPUT_BYTES} bytes)"
        )));
    }
    if let Some(ref origin) = raw.origin
        && origin.len() > MAX_INPUT_BYTES
    {
        return Err(ShellError::Usage(format!(
            "--origin value too long (max {MAX_INPUT_BYTES} bytes)"
        )));
    }
    if let Some(ref input) = raw.input
        && input.len() > MAX_INPUT_BYTES
    {
        return Err(ShellError::Usage(format!(
            "input too long (max {MAX_INPUT_BYTES} bytes)"
        )));
    }
    let (input, tabs) = match raw.input {
        Some(input) => {
            let input = normalize_input(&input)?;
            let mut tabs = vec![input.clone()];
            tabs.extend(extra_tabs);
            (input, tabs)
        }
        None if !extra_tabs.is_empty() => {
            let input = extra_tabs[0].clone();
            (input, extra_tabs)
        }
        None => {
            return Err(ShellError::Usage(format!("missing <url|file>\n{USAGE}")));
        }
    };
    let active_tab = raw.active_tab.unwrap_or(0);
    if active_tab >= tabs.len() {
        return Err(ShellError::Usage(format!(
            "--active-tab {active_tab} is out of range ({} tab(s))",
            tabs.len()
        )));
    }
    let width = raw.width.unwrap_or(DEFAULT_WIDTH);
    let height = raw.height.unwrap_or(DEFAULT_HEIGHT);
    validate_dimensions(width, height)?;
    if raw.window && raw.screenshot.is_some() {
        return Err(ShellError::Usage(
            "--window cannot be combined with --screenshot".to_string(),
        ));
    }
    if raw.window && raw.print_to_pdf.is_some() {
        return Err(ShellError::Usage(
            "--window cannot be combined with --print-to-pdf".to_string(),
        ));
    }
    if raw.screenshot.is_some() && raw.print_to_pdf.is_some() {
        return Err(ShellError::Usage(
            "--screenshot cannot be combined with --print-to-pdf".to_string(),
        ));
    }
    if raw.print_to_pdf.is_some()
        && (raw.chrome || raw.chrome_menu || raw.find.is_some() || raw.ipc_render || raw.reader)
    {
        return Err(ShellError::Usage(
            "--print-to-pdf cannot be combined with --chrome, --chrome-menu, --find, \
             --ipc-render or --reader"
                .to_string(),
        ));
    }
    let mode = if let Some(path) = &raw.screenshot {
        LaunchMode::Screenshot(path.clone())
    } else if let Some(path) = &raw.print_to_pdf {
        LaunchMode::PrintPdf(path.clone())
    } else {
        LaunchMode::Window
    };
    if raw.chrome_menu && !raw.chrome {
        return Err(ShellError::Usage(
            "--chrome-menu requires --chrome".to_string(),
        ));
    }
    let zoom = match raw.zoom {
        Some(zoom) => {
            if !crate::zoom::is_valid_cli_zoom(zoom) {
                return Err(ShellError::Usage(format!(
                    "invalid --zoom value `{zoom}` (expected {min}..={max})",
                    min = crate::zoom::MIN_ZOOM,
                    max = crate::zoom::MAX_ZOOM
                )));
            }
            zoom
        }
        None => crate::zoom::DEFAULT_ZOOM,
    };
    let time_ms = raw.time_ms.unwrap_or(0.0);
    if !time_ms.is_finite() || time_ms < 0.0 {
        return Err(ShellError::Usage(format!(
            "invalid --time value `{time_ms}` (expected a finite non-negative number)"
        )));
    }
    if let Some(0) = raw.refresh_ms {
        // `--refresh-ms 0` explicitly disables auto-refresh; normalise to `None`.
        return Ok(CliArgs {
            input,
            title: raw.title.unwrap_or_else(|| DEFAULT_TITLE.to_string()),
            width,
            height,
            mode,
            print_to_pdf: raw.print_to_pdf.clone(),
            cookies: raw.cookies,
            origin: raw.origin,
            zoom,
            refresh_ms: None,
            time_ms,
            find: raw.find,
            tabs,
            active_tab,
            chrome: raw.chrome,
            chrome_menu: raw.chrome_menu,
            reader: raw.reader,
            ipc_render: raw.ipc_render,
            remote_debugging_port: raw.remote_debugging_port,
            no_session: raw.no_session,
            session_file: raw.session_file,
            download_dir: raw.download_dir,
            save_png: raw.save_png,
            save_html: raw.save_html,
        });
    }
    Ok(CliArgs {
        input,
        title: raw.title.unwrap_or_else(|| DEFAULT_TITLE.to_string()),
        width,
        height,
        mode,
        print_to_pdf: raw.print_to_pdf.clone(),
        cookies: raw.cookies,
        origin: raw.origin,
        zoom,
        refresh_ms: raw.refresh_ms,
        time_ms,
        find: raw.find,
        tabs,
        active_tab,
        chrome: raw.chrome,
        chrome_menu: raw.chrome_menu,
        reader: raw.reader,
        ipc_render: raw.ipc_render,
        remote_debugging_port: raw.remote_debugging_port,
        no_session: raw.no_session,
        session_file: raw.session_file,
        download_dir: raw.download_dir,
        save_png: raw.save_png,
        save_html: raw.save_html,
    })
}

/// Validates the requested viewport/window dimensions.
pub fn validate_dimensions(width: u32, height: u32) -> Result<(), ShellError> {
    if !(1..=MAX_DIMENSION).contains(&width) || !(1..=MAX_DIMENSION).contains(&height) {
        return Err(ShellError::Usage(format!(
            "invalid dimensions {width}x{height} (each axis must be 1..={MAX_DIMENSION})"
        )));
    }
    Ok(())
}

/// Normalizes the CLI input into an absolute URL.
///
/// A string that parses as an absolute URL is used as-is; otherwise it is treated as a
/// filesystem path, canonicalized and converted to a `file://` URL. The render URL parser
/// percent-encodes spaces and non-ASCII path bytes.
pub fn normalize_input(input: &str) -> Result<String, ShellError> {
    if crate::history::is_history_url(input)
        || crate::pages::is_about_page_url(input)
        || crate::pages::is_settings_url(input)
    {
        return Ok(input.to_string());
    }
    if megi_webview::net::Url::parse(input).is_ok() {
        return Ok(input.to_string());
    }
    let path = std::fs::canonicalize(input).map_err(|error| {
        ShellError::Usage(format!(
            "cannot resolve `{input}` as a URL or local file: {error}"
        ))
    })?;
    // Escape `#`, `?`, `%` and spaces: a raw `file:///tmp/a#b.html` parses as path
    // `/tmp/a` with fragment `b.html` and loads the wrong file.
    let url = crate::pages::file_url(&path);
    megi_webview::net::Url::parse(&url)
        .map(|parsed| parsed.as_str())
        .map_err(|error| {
            ShellError::Usage(format!(
                "cannot build file URL for {}: {error}",
                path.display()
            ))
        })
}
