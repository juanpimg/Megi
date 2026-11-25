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

/// Decodes a renderer response envelope and proves that it answers this slot's request.
///
/// #2308: a response is only accepted when its kind, request id and per-site token match
/// the request that was sent, and when its generation follows the last accepted one.
/// Semantic validation is delegated to [`validate_response`] so both the synchronous and
/// reader-thread paths share one trust boundary.
pub(crate) fn decode_validated_response(
    header: &WireHeader,
    response: IpcRenderResponse,
    expected_token: &[u8; 32],
    last_generation: u64,
) -> Result<Validated<IpcRenderResponse>, IpcError> {
    if header.kind != IPC_KIND_RESPONSE {
        return Err(IpcError::Validation(format!(
            "unexpected response kind {}",
            header.kind
        )));
    }
    if header.request_id != response.generation {
        return Err(IpcError::Validation(format!(
            "response request id {} does not match generation {}",
            header.request_id, response.generation
        )));
    }
    if header.site_token != *expected_token {
        return Err(IpcError::Validation(
            "response site token does not match this slot".to_owned(),
        ));
    }
    validate_response(response, last_generation)
        .map_err(|error| IpcError::Validation(error.to_string()))
}

/// A response that has crossed the trust boundary of a renderer slot (#2308).
pub(crate) type ValidatedResponse = Result<Validated<IpcRenderResponse>, IpcError>;

/// Token for standalone renderers that are not owned by a pool slot. The counter keeps
/// tokens unique across spawns in one process; pooled renderers use the slot's token.
fn default_token() -> [u8; 32] {
    crate::site::derive_renderer_token(
        &crate::site::SiteKey::Opaque("renderer"),
        SCRATCH_COUNTER.fetch_add(1, Ordering::Relaxed),
    )
}

/// A renderer process: the current executable launched with `--megi-ipc-worker` and
/// driven over versioned envelopes on stdin/stdout (ADR-004 phase 3, #2308).
pub struct ProcessRenderer {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: Option<std::process::ChildStdout>,
    site_token: [u8; 32],
    /// Generations are global to the browser, so a newly spawned slot may legitimately
    /// answer generation N as its first frame. `None` pins that first frame via its
    /// request id rather than assuming generation 1 (#2308).
    last_generation: Option<u64>,
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
        Self::spawn_with_args_with_network_and_token(
            executable,
            args,
            allow_network,
            read_path,
            default_token(),
        )
    }

    /// Core spawn path: the slot's token is stamped on every request envelope and used to
    /// authenticate every response envelope (#2308).
    pub(crate) fn spawn_with_args_with_network_and_token(
        executable: &Path,
        args: &[String],
        allow_network: bool,
        read_path: Option<&Path>,
        site_token: [u8; 32],
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
            site_token,
            last_generation: None,
        })
    }

    /// Renders one request in the child process and returns the validated result.
    pub fn render(&mut self, request: IpcRenderRequest) -> Result<IpcRenderResponse, IpcError> {
        Ok(self.render_validated(request)?.into_inner())
    }

    /// Envelope-aware variant of [`Self::render`] kept under the newtype proof for the
    /// stateful session and for the pool's reader thread.
    pub(crate) fn render_validated(
        &mut self,
        request: IpcRenderRequest,
    ) -> Result<Validated<IpcRenderResponse>, IpcError> {
        self.write_request(&request)?;
        let stdout = self.stdout.as_mut().ok_or_else(|| {
            IpcError::Io("renderer stdout is owned by the reader thread".to_owned())
        })?;
        let (header, response) =
            read_framed_envelope::<_, IpcRenderResponse>(stdout)?.ok_or(IpcError::Truncated)?;
        // Generations are globally allocated, so one slot can see gaps; only a stale or
        // repeated generation is fatal. The exact generation is pinned by the request id.
        if let Some(last) = self.last_generation
            && request.generation <= last
        {
            return Err(IpcError::Validation(format!(
                "stale generation {} after {last}",
                request.generation
            )));
        }
        let last = request.generation.saturating_sub(1);
        let validated = decode_validated_response(&header, response, &self.site_token, last)?;
        self.last_generation = Some(validated.inner().generation);
        Ok(validated)
    }

    /// Spawns a renderer whose stdout is drained by a reader thread into a channel, so the
    /// event loop can send requests and poll replies without blocking (ADR-004 phase 3).
    pub fn spawn_with_reader()
    -> Result<(Self, std::sync::mpsc::Receiver<ValidatedResponse>), IpcError> {
        Self::spawn_with_reader_with_network(true, None)
    }

    /// In-process executable variant of [`Self::spawn_with_reader`] with a network policy.
    pub(crate) fn spawn_with_reader_with_network(
        allow_network: bool,
        read_path: Option<&Path>,
    ) -> Result<(Self, std::sync::mpsc::Receiver<ValidatedResponse>), IpcError> {
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
    ) -> Result<(Self, std::sync::mpsc::Receiver<ValidatedResponse>), IpcError> {
        Self::spawn_with_reader_from_args(executable, &["--megi-ipc-worker".to_owned()])
    }

    /// Like [`Self::spawn_with_reader_from`] with explicit argv for isolation tests.
    pub fn spawn_with_reader_from_args(
        executable: &Path,
        args: &[String],
    ) -> Result<(Self, std::sync::mpsc::Receiver<ValidatedResponse>), IpcError> {
        Self::spawn_with_reader_from_args_with_network(executable, args, true, None)
    }

    /// Like [`Self::spawn_with_reader_from_args`] with the slot's sandbox network policy.
    pub(crate) fn spawn_with_reader_from_args_with_network(
        executable: &Path,
        args: &[String],
        allow_network: bool,
        read_path: Option<&Path>,
    ) -> Result<(Self, std::sync::mpsc::Receiver<ValidatedResponse>), IpcError> {
        Self::spawn_with_reader_from_args_with_network_and_token(
            executable,
            args,
            allow_network,
            read_path,
            default_token(),
        )
    }

    /// Core reader-thread spawn: the thread authenticates each envelope against this
    /// renderer's token before publishing a validated response (#2308).
    pub(crate) fn spawn_with_reader_from_args_with_network_and_token(
        executable: &Path,
        args: &[String],
        allow_network: bool,
        read_path: Option<&Path>,
        site_token: [u8; 32],
    ) -> Result<(Self, std::sync::mpsc::Receiver<ValidatedResponse>), IpcError> {
        let mut renderer = Self::spawn_with_args_with_network_and_token(
            executable,
            args,
            allow_network,
            read_path,
            site_token,
        )?;
        let stdout = renderer
            .stdout
            .take()
            .ok_or_else(|| IpcError::Io("no stdout".to_owned()))?;
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut stdout = stdout;
            let mut last_generation: Option<u64> = None;
            loop {
                let incoming = match read_framed_envelope::<_, IpcRenderResponse>(&mut stdout) {
                    Ok(Some(frame)) => frame,
                    Ok(None) => break,
                    Err(error) => {
                        let _ = sender.send(Err(error));
                        break;
                    }
                };
                // The first generation in a slot can be any value: the browser-wide
                // counter keeps advancing as new sites are spawned (#2308). Pin it to
                // the request id, and accept gaps while rejecting stale/repeated frames.
                let response_generation = incoming.1.generation;
                if let Some(last) = last_generation
                    && response_generation <= last
                {
                    let _ = sender.send(Err(IpcError::Validation(format!(
                        "stale generation {response_generation} after {last}"
                    ))));
                    break;
                }
                let last = response_generation.saturating_sub(1);
                match decode_validated_response(&incoming.0, incoming.1, &site_token, last) {
                    Ok(validated) => {
                        last_generation = Some(validated.inner().generation);
                        if sender.send(Ok(validated)).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(Err(error));
                        break;
                    }
                }
            }
        });
        Ok((renderer, receiver))
    }

    /// Writes one render request as a versioned envelope (#2308).
    pub fn write_request(&mut self, request: &IpcRenderRequest) -> Result<(), IpcError> {
        self.write_request_with_token(request, self.site_token)
    }

    /// Envelope write with an explicit slot token, used by the pool so the token chosen at
    /// slot creation remains the authoritative one even after a renderer replacement.
    pub(crate) fn write_request_with_token(
        &mut self,
        request: &IpcRenderRequest,
        site_token: [u8; 32],
    ) -> Result<(), IpcError> {
        write_framed_envelope(
            &mut self.stdin,
            IPC_KIND_REQUEST,
            request.generation,
            site_token,
            request,
        )
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
