//! Renderer-process lifecycle and framed I/O (ADR-004).

use super::*;
/// A renderer process: the current executable launched with `--megi-ipc-worker` and
/// driven over length-prefixed JSON on stdin/stdout (ADR-004 phase 3).
pub struct ProcessRenderer {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: Option<std::process::ChildStdout>,
}

impl ProcessRenderer {
    /// Spawns an isolated renderer child process.
    pub fn spawn() -> Result<Self, IpcError> {
        let executable =
            std::env::current_exe().map_err(|error| IpcError::Io(error.to_string()))?;
        Self::spawn_with_executable(&executable)
    }

    /// Spawns a renderer from an explicit executable path (used by integration tests that
    /// drive the real `megi --megi-ipc-worker` binary).
    pub fn spawn_with_executable(executable: &std::path::Path) -> Result<Self, IpcError> {
        let mut child = std::process::Command::new(executable)
            .arg("--megi-ipc-worker")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|error| IpcError::Io(error.to_string()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| IpcError::Io("no stdin".to_owned()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| IpcError::Io("no stdout".to_owned()))?;
        Ok(Self {
            child,
            stdin,
            stdout: Some(stdout),
        })
    }

    /// Renders one request in the child process and returns the serializable result.
    pub fn render(&mut self, request: IpcRenderRequest) -> Result<IpcRenderResponse, IpcError> {
        write_message(&mut self.stdin, &request)?;
        let stdout = self.stdout.as_mut().ok_or_else(|| {
            IpcError::Io("renderer stdout is owned by the reader thread".to_owned())
        })?;
        read_message(stdout)
    }

    /// Spawns a renderer whose stdout is drained by a reader thread into a channel, so the
    /// event loop can send requests and poll replies without blocking (ADR-004 phase 3).
    pub fn spawn_with_reader() -> Result<
        (
            Self,
            std::sync::mpsc::Receiver<Result<IpcRenderResponse, IpcError>>,
        ),
        IpcError,
    > {
        let executable =
            std::env::current_exe().map_err(|error| IpcError::Io(error.to_string()))?;
        Self::spawn_with_reader_from(&executable)
    }

    /// Like [`Self::spawn_with_reader`] but spawning an explicit executable (used by
    /// integration tests that drive the real `megi --megi-ipc-worker` binary).
    pub fn spawn_with_reader_from(
        executable: &std::path::Path,
    ) -> Result<
        (
            Self,
            std::sync::mpsc::Receiver<Result<IpcRenderResponse, IpcError>>,
        ),
        IpcError,
    > {
        let mut renderer = Self::spawn_with_executable(executable)?;
        let stdout = renderer
            .stdout
            .take()
            .ok_or_else(|| IpcError::Io("no stdout".to_owned()))?;
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut stdout = stdout;
            loop {
                match read_message::<_, IpcRenderResponse>(&mut stdout) {
                    Ok(response) => {
                        if sender.send(Ok(response)).is_err() {
                            break;
                        }
                    }
                    Err(IpcError::Truncated) => break,
                    Err(error) => {
                        let _ = sender.send(Err(error));
                        break;
                    }
                }
            }
        });
        Ok((renderer, receiver))
    }

    /// Writes one render request without waiting for the response.
    pub fn write_request(&mut self, request: &IpcRenderRequest) -> Result<(), IpcError> {
        write_message(&mut self.stdin, request)
    }

    /// PID of the renderer process (diagnostics/tests).
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// True while the child process is still running (ADR-004 crash recovery).
    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Terminates the renderer process (best effort).
    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for ProcessRenderer {
    fn drop(&mut self) {
        // An evicted or replaced renderer must not leave an orphan child behind.
        // Reaping here also makes `join()`/pool shutdown deterministic.
        if self.is_alive() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// Renders `input` in a child process and returns the PNG bytes (ADR-004 phase 3).
/// Useful for the headless screenshot path, where no DOM round-trip is needed.
pub fn render_png_in_process(
    input: &str,
    css_width: u32,
    css_height: u32,
) -> Result<Vec<u8>, IpcError> {
    render_png_in_process_with_options(
        input,
        css_width,
        css_height,
        &megi_webview::render::RenderOptions::default(),
    )
}

/// [`render_png_in_process`] forwarding `options` (time, cookies, origin, resource caps)
/// across the process boundary (#2165).
pub fn render_png_in_process_with_options(
    input: &str,
    css_width: u32,
    css_height: u32,
    options: &megi_webview::render::RenderOptions,
) -> Result<Vec<u8>, IpcError> {
    let mut renderer = ProcessRenderer::spawn()?;
    let request = IpcRenderRequest {
        generation: 1,
        input: input.to_owned(),
        css_width,
        css_height,
        base_override: None,
        scripting: true,
        find_query: None,
        options: IpcRenderOptions::from_render_options(options),
    };
    let response = renderer.render(request)?;
    renderer.kill();
    if let Some(error) = response.error {
        return Err(IpcError::Io(error));
    }
    Ok(response.png_bytes)
}
