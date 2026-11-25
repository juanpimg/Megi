use super::*;

/// Parses raw arguments (no normalization, no defaults applied).
pub fn parse_args(args: impl Iterator<Item = String>) -> Result<RawArgs, ShellError> {
    let arguments: Vec<String> = args.collect();
    let mut raw = RawArgs::default();
    let mut index = 0usize;
    while index < arguments.len() {
        let argument = arguments[index].clone();
        index += 1;
        match argument.as_str() {
            "-h" | "--help" => raw.help = true,
            "--width" => raw.width = Some(parse_value_u32(&arguments, &mut index, "--width")?),
            "--height" => raw.height = Some(parse_value_u32(&arguments, &mut index, "--height")?),
            "--screenshot" => {
                let value = next_value(&arguments, &mut index, "--screenshot")?;
                raw.screenshot = Some(PathBuf::from(value));
            }
            "--print-to-pdf" => {
                let value = next_value(&arguments, &mut index, "--print-to-pdf")?;
                raw.print_to_pdf = Some(PathBuf::from(value));
            }
            "--title" => {
                raw.title = Some(next_value(&arguments, &mut index, "--title")?);
            }
            "--cookies" => raw.cookies = true,
            "--origin" => {
                raw.origin = Some(next_value(&arguments, &mut index, "--origin")?);
            }
            "--find" => {
                raw.find = Some(next_value(&arguments, &mut index, "--find")?);
            }
            "--tabs" => {
                let value = next_value(&arguments, &mut index, "--tabs")?;
                if raw.tabs.len() >= 32 {
                    return Err(ShellError::Usage("--tabs count exceeds 32".to_string()));
                }
                raw.tabs.push(value);
            }
            "--active-tab" => {
                let value = next_value(&arguments, &mut index, "--active-tab")?;
                raw.active_tab = Some(value.parse::<usize>().map_err(|_| {
                    ShellError::Usage(format!("invalid --active-tab value `{value}`"))
                })?);
            }
            "--chrome" => raw.chrome = true,
            "--chrome-menu" => raw.chrome_menu = true,
            "--chrome-compact" => raw.chrome_compact = true,
            "--rail" => raw.rail = true,
            "--compact" => raw.compact = true,
            "--reduced-motion" => raw.reduced_motion = true,
            "--reader" => raw.reader = true,
            "--ipc-render" => raw.ipc_render = true,
            "--remote-debugging-port" => {
                let value = next_value(&arguments, &mut index, "--remote-debugging-port")?;
                let port = value.parse::<u16>().map_err(|_| {
                    ShellError::Usage(format!(
                        "invalid --remote-debugging-port value `{value}` (expected 1..=65535)"
                    ))
                })?;
                if port == 0 {
                    return Err(ShellError::Usage(
                        "--remote-debugging-port must be 1..=65535".to_string(),
                    ));
                }
                raw.remote_debugging_port = Some(port);
            }
            "--webdriver-bidi-port" => {
                let value = next_value(&arguments, &mut index, "--webdriver-bidi-port")?;
                let port = value.parse::<u16>().map_err(|_| {
                    ShellError::Usage(format!(
                        "invalid --webdriver-bidi-port value `{value}` (expected 1..=65535)"
                    ))
                })?;
                if port == 0 {
                    return Err(ShellError::Usage(
                        "--webdriver-bidi-port must be 1..=65535".to_string(),
                    ));
                }
                raw.webdriver_bidi_port = Some(port);
            }
            "--window" => raw.window = true,
            "--no-session" => raw.no_session = true,
            "--session-file" => {
                raw.session_file = Some(PathBuf::from(next_value(
                    &arguments,
                    &mut index,
                    "--session-file",
                )?));
            }
            "--download-dir" => {
                raw.download_dir = Some(PathBuf::from(next_value(
                    &arguments,
                    &mut index,
                    "--download-dir",
                )?));
            }
            "--save-png" => {
                raw.save_png = Some(PathBuf::from(next_value(
                    &arguments,
                    &mut index,
                    "--save-png",
                )?));
            }
            "--save-html" => {
                raw.save_html = Some(PathBuf::from(next_value(
                    &arguments,
                    &mut index,
                    "--save-html",
                )?));
            }
            "--zoom" => {
                let value = next_value(&arguments, &mut index, "--zoom")?;
                raw.zoom = Some(value.parse::<f64>().map_err(|_| {
                    ShellError::Usage(format!(
                        "invalid --zoom value `{value}` (expected a number)"
                    ))
                })?);
            }
            "--time" => {
                let value = next_value(&arguments, &mut index, "--time")?;
                raw.time_ms = Some(value.parse::<f64>().map_err(|_| {
                    ShellError::Usage(format!(
                        "invalid --time value `{value}` (expected a number)"
                    ))
                })?);
            }
            "--refresh-ms" => {
                // Optional value: use the next argument when it is a bare integer, otherwise
                // fall back to the default interval without consuming it. A present but
                // unparseable value is an error (not silently swallowed). Values are
                // bounded to [16, 60000] ms to avoid overflow in Instant addition
                // (issue #594).
                if let Some(value) = arguments.get(index) {
                    match value.parse::<u64>() {
                        Ok(interval) => {
                            if interval != 0 && !(16..=60000).contains(&interval) {
                                return Err(ShellError::Usage(format!(
                                    "invalid --refresh-ms value `{value}` (expected 0 or 16..=60000)"
                                )));
                            }
                            index += 1;
                            raw.refresh_ms = Some(interval);
                        }
                        Err(_) => {
                            // Not an integer: leave it unconsumed (e.g. flag, URL, or local path)
                            // and fall back to the default interval.
                            raw.refresh_ms = Some(crate::refresh::DEFAULT_REFRESH_MS);
                        }
                    }
                } else {
                    raw.refresh_ms = Some(crate::refresh::DEFAULT_REFRESH_MS);
                }
            }
            other if other.starts_with('-') => {
                return Err(ShellError::Usage(format!("unknown option `{other}`")));
            }
            other => {
                if raw.input.is_some() {
                    return Err(ShellError::Usage(format!(
                        "unexpected extra positional argument `{other}`"
                    )));
                }
                raw.input = Some(other.to_string());
            }
        }
    }
    Ok(raw)
}

/// Consumes the value of a CLI flag.
pub(crate) fn next_value(
    arguments: &[String],
    index: &mut usize,
    flag: &str,
) -> Result<String, ShellError> {
    let value = arguments
        .get(*index)
        .cloned()
        .ok_or_else(|| ShellError::Usage(format!("{flag} requires a value")))?;
    *index += 1;
    Ok(value)
}

/// Consumes and parses a `u32` CLI flag value.
pub(crate) fn parse_value_u32(
    arguments: &[String],
    index: &mut usize,
    flag: &str,
) -> Result<u32, ShellError> {
    let value = next_value(arguments, index, flag)?;
    value
        .parse::<u32>()
        .map_err(|_| ShellError::Usage(format!("invalid {flag} value `{value}`")))
}
