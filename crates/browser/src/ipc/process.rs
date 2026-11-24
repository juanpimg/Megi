//! Renderer-process lifecycle and framed I/O (ADR-004).

use super::*;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static SCRATCH_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Creates a private scratch directory used as renderer cwd and grant root.
pub fn renderer_scratch_dir() -> Result<PathBuf, IpcError> {
    let path = std::env::temp_dir().join(format!(
        "megi-ipc-{}-{}",
        std::process::id(),
        SCRATCH_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&path).map_err(|error| IpcError::Io(error.to_string()))?;
    Ok(path)
}

/// Builds a renderer command with a scrubbed environment and its own process group.
///
/// Plan §Task 3 (T9): only a pinned `PATH` (plus an explicit `RUST_BACKTRACE` override)
/// crosses `execve(2)`, and POSIX process group 0 makes the child its group leader so a
/// watchdog can terminate the whole tree without unsafe FFI.
pub fn renderer_command(
    executable: &Path,
    args: &[String],
    scratch: &Path,
) -> std::process::Command {
    let mut command = std::process::Command::new(executable);
    command.args(args).env_clear().env("PATH", "/usr/bin:/bin");
    if let Ok(backtrace) = std::env::var("RUST_BACKTRACE") {
        command.env("RUST_BACKTRACE", backtrace);
    }
    command
        .current_dir(scratch)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    #[cfg(unix)]
    command.process_group(0);
    command
}

/// Same harness as [`renderer_command`] plus the sandbox network policy for this slot.
/// The public helper above stays byte-identical for the environment-hardening tests.
pub(crate) fn renderer_command_with_network(
    executable: &Path,
    args: &[String],
    scratch: &Path,
    allow_network: bool,
    read_path: Option<&Path>,
) -> std::process::Command {
    let mut command = renderer_command(executable, args, scratch);
    command.env(
        "MEGI_SANDBOX_ALLOW_NETWORK",
        if allow_network { "1" } else { "0" },
    );
    if let Some(path) = read_path {
        command.env("MEGI_SANDBOX_READ_PATH", path);
    }
    command
}

/// Terminates the renderer's whole process group with SIGTERM (POSIX pgid == pid).
#[cfg(unix)]
pub fn kill_process_group(pid: u32) {
    let _ = std::process::Command::new("/usr/bin/kill")
        .args(["-TERM", &format!("-{pid}")])
        .status();
}

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
    pub fn spawn_with_executable(executable: &Path) -> Result<Self, IpcError> {
        Self::spawn_with_args(executable, &["--megi-ipc-worker".to_owned()])
    }

    /// Spawns a renderer with explicit argv (tests inject a real non-responding child).
    pub fn spawn_with_args(executable: &Path, args: &[String]) -> Result<Self, IpcError> {
        Self::spawn_with_args_with_network(executable, args, true, None)
    }

    /// Spawns a renderer with an explicit per-slot network policy (M3#4).
    pub(crate) fn spawn_with_args_with_network(
        executable: &Path,
        args: &[String],
        allow_network: bool,
        read_path: Option<&Path>,
    ) -> Result<Self, IpcError> {
        let scratch = renderer_scratch_dir()?;
        let mut child =
            renderer_command_with_network(executable, args, &scratch, allow_network, read_path)
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
        Self::spawn_with_reader_with_network(true, None)
    }

    /// In-process executable variant of [`Self::spawn_with_reader`] with a network policy.
    pub(crate) fn spawn_with_reader_with_network(
        allow_network: bool,
        read_path: Option<&Path>,
    ) -> Result<
        (
            Self,
            std::sync::mpsc::Receiver<Result<IpcRenderResponse, IpcError>>,
        ),
        IpcError,
    > {
        let executable =
            std::env::current_exe().map_err(|error| IpcError::Io(error.to_string()))?;
        Self::spawn_with_reader_from_args_with_network(
            &executable,
            &["--megi-ipc-worker".to_owned()],
            allow_network,
            read_path,
        )
    }

    /// Like [`Self::spawn_with_reader`] but spawning an explicit executable (used by
    /// integration tests that drive the real `megi --megi-ipc-worker` binary).
    pub fn spawn_with_reader_from(
        executable: &Path,
    ) -> Result<
        (
            Self,
            std::sync::mpsc::Receiver<Result<IpcRenderResponse, IpcError>>,
        ),
        IpcError,
    > {
        Self::spawn_with_reader_from_args(executable, &["--megi-ipc-worker".to_owned()])
    }

    /// Like [`Self::spawn_with_reader_from`] with explicit argv for isolation tests.
    pub fn spawn_with_reader_from_args(
        executable: &Path,
        args: &[String],
    ) -> Result<
        (
            Self,
            std::sync::mpsc::Receiver<Result<IpcRenderResponse, IpcError>>,
        ),
        IpcError,
    > {
        Self::spawn_with_reader_from_args_with_network(executable, args, true, None)
    }

    /// Like [`Self::spawn_with_reader_from_args`] with the slot's sandbox network policy.
    pub(crate) fn spawn_with_reader_from_args_with_network(
        executable: &Path,
        args: &[String],
        allow_network: bool,
        read_path: Option<&Path>,
    ) -> Result<
        (
            Self,
            std::sync::mpsc::Receiver<Result<IpcRenderResponse, IpcError>>,
        ),
        IpcError,
    > {
        let mut renderer =
            Self::spawn_with_args_with_network(executable, args, allow_network, read_path)?;
        let stdout = renderer
            .stdout
            .take()
            .ok_or_else(|| IpcError::Io("no stdout".to_owned()))?;
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut stdout = stdout;
            loop {
                match read_message_checked::<_, IpcRenderResponse>(&mut stdout) {
                    Ok(Some(response)) => {
                        if sender.send(Ok(response)).is_err() {
                            break;
                        }
                    }
                    Ok(None) => break,
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

    /// Terminates the renderer's process group, then reaps the direct child (T5 watchdog).
    pub(crate) fn kill_group(&mut self) {
        #[cfg(unix)]
        kill_process_group(self.id());
        self.kill();
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
    let allow_network = super::process_pool::request_sandbox_allows_network(&request);
    let read_path = super::process_pool::request_sandbox_read_path(&request);
    let executable = std::env::current_exe().map_err(|error| IpcError::Io(error.to_string()))?;
    let mut renderer = ProcessRenderer::spawn_with_args_with_network(
        &executable,
        &["--megi-ipc-worker".to_owned()],
        allow_network,
        read_path.as_deref(),
    )?;
    let response = renderer.render(request)?;
    renderer.kill();
    if let Some(error) = response.error {
        return Err(IpcError::Io(error));
    }
    Ok(response.png_bytes)
}
