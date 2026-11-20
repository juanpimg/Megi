//! Safe IPC framing for per-site renderer processes (ADR-004 phase 3).
//!
//! Messages are length-prefixed JSON (`u32` big-endian length + UTF-8 payload) over a
//! child process's stdin/stdout. No FFI, no unsafe code, no shared memory. The cap on the
//! declared length prevents a hostile peer from forcing an unbounded allocation.

use std::io::{Read, Write};

mod process_pool;
mod regions;
mod session;
pub use process_pool::{MAX_PROCESS_RENDERERS, ProcessRenderPool, request_process_site};
pub use regions::{
    IpcFindMatch, IpcFormControl, IpcImageRegion, IpcLinkRegion, IpcRect, IpcSelectOption,
};
pub use session::{ProcessFrame, ProcessRenderSession};

/// Maximum accepted IPC payload (256 MiB: a 4096×4096 RGBA frame plus metadata).
pub const MAX_IPC_PAYLOAD: usize = 256 * 1024 * 1024;

/// Errors from the IPC framing layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpcError {
    /// The peer closed before a complete header/payload arrived.
    Truncated,
    /// The declared length is larger than [`MAX_IPC_PAYLOAD`].
    TooLarge(u32),
    /// The payload is not valid UTF-8 or not valid JSON.
    InvalidPayload(String),
    /// An underlying I/O error.
    Io(String),
}

impl IpcError {
    /// True when this error means the renderer process is gone (EOF or broken pipe) and the
    /// pool must drop the slot and spawn a fresh process for the next request.
    pub fn is_fatal(&self) -> bool {
        matches!(self, Self::Truncated | Self::Io(_))
    }
}

impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncated => f.write_str("IPC stream truncated"),
            Self::TooLarge(length) => write!(f, "IPC payload too large ({length} bytes)"),
            Self::InvalidPayload(reason) => write!(f, "invalid IPC payload: {reason}"),
            Self::Io(reason) => write!(f, "IPC I/O error: {reason}"),
        }
    }
}

/// Writes one length-prefixed JSON message.
pub fn write_message<W: Write, T: serde::Serialize>(
    writer: &mut W,
    value: &T,
) -> Result<(), IpcError> {
    let payload =
        serde_json::to_vec(value).map_err(|error| IpcError::InvalidPayload(error.to_string()))?;
    let length =
        u32::try_from(payload.len()).map_err(|_| IpcError::TooLarge(payload.len() as u32))?;
    writer
        .write_all(&length.to_be_bytes())
        .and_then(|()| writer.write_all(&payload))
        .and_then(|()| writer.flush())
        .map_err(|error| IpcError::Io(error.to_string()))
}

/// Reads one length-prefixed JSON message.
pub fn read_message<R: Read, T: serde::de::DeserializeOwned>(
    reader: &mut R,
) -> Result<T, IpcError> {
    let mut header = [0u8; 4];
    read_exact_or(reader, &mut header)?;
    let length = u32::from_be_bytes(header) as usize;
    if length > MAX_IPC_PAYLOAD {
        return Err(IpcError::TooLarge(length as u32));
    }
    let mut payload = vec![0u8; length];
    read_exact_or(reader, &mut payload)?;
    serde_json::from_slice(&payload).map_err(|error| IpcError::InvalidPayload(error.to_string()))
}

fn read_exact_or<R: Read>(reader: &mut R, buffer: &mut [u8]) -> Result<(), IpcError> {
    reader
        .read_exact(buffer)
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::UnexpectedEof => IpcError::Truncated,
            _ => IpcError::Io(error.to_string()),
        })
}

/// A render request as it crosses the process boundary (ADR-004 phase 3).
/// `RenderRequest`'s non-serializable host events are intentionally excluded; the child
/// renders the document and its scripts, while DOM patching stays in the parent.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct IpcRenderRequest {
    pub generation: u64,
    pub input: String,
    pub css_width: u32,
    pub css_height: u32,
    pub base_override: Option<String>,
    pub scripting: bool,
    pub find_query: Option<String>,
}

/// The serializable part of a rendered frame. The parent rebuilds documentation-dependent
/// state (form controls, live DOM) locally; process mode is documented as a reduced-realm
/// renderer until ADR-004 phase 3 is completed.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct IpcRenderResponse {
    pub generation: u64,
    pub png_bytes: Vec<u8>,
    pub content_height: u32,
    /// CSS viewport width used for the render (for frame reconstruction).
    pub css_width: u32,
    pub title: Option<String>,
    pub document_url: String,
    pub scripts_executed: usize,
    pub script_timers_pending: usize,
    pub warnings: usize,
    pub script_errors: usize,
    /// Clickable link hit regions with DOM ordinals (ADR-004 hit-region parity).
    pub links: Vec<IpcLinkRegion>,
    pub images: Vec<IpcImageRegion>,
    pub forms: Vec<IpcFormControl>,
    pub find_matches: Vec<IpcFindMatch>,
    pub find_rects: Vec<IpcRect>,
    /// Post-script serialized DOM, so the parent can build further patches (ADR-004 phase 3).
    pub document_html: String,
    /// Render failure text (`Some` means the other fields are defaults).
    pub error: Option<String>,
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
        let mut renderer = Self::spawn()?;
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

/// Renders `input` in a child process and returns the PNG bytes (ADR-004 phase 3).
/// Useful for the headless screenshot path, where no DOM round-trip is needed.
pub fn render_png_in_process(
    input: &str,
    css_width: u32,
    css_height: u32,
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
    };
    let response = renderer.render(request)?;
    renderer.kill();
    if let Some(error) = response.error {
        return Err(IpcError::Io(error));
    }
    Ok(response.png_bytes)
}

/// Entry point for `--megi-ipc-worker`: serves requests until the parent closes stdin.
/// Only the serializable render payload crosses the boundary; DOM-dependent editing stays
/// in the parent renderer (documented ADR-004 phase-3 limitation).
pub fn run_ipc_worker() -> Result<(), IpcError> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();
    loop {
        let request: IpcRenderRequest = match read_message(&mut reader) {
            Ok(request) => request,
            Err(IpcError::Truncated) => return Ok(()),
            Err(error) => return Err(error),
        };
        let render_request = crate::worker::RenderRequest {
            generation: request.generation,
            input: request.input.clone(),
            css_width: request.css_width,
            css_height: request.css_height,
            base_override: request.base_override.clone(),
            scripting: request.scripting,
            find_query: request.find_query.clone(),
            host_events: Vec::new(),
            host_dom_events: Vec::new(),
            form_post: None,
        };
        let options = megi_webview::render::RenderOptions::default();
        let response = match crate::worker::render_document_with_options(&render_request, &options)
        {
            Ok(frame) => IpcRenderResponse {
                generation: frame.generation,
                png_bytes: frame.png_bytes,
                content_height: frame.content_height,
                css_width: request.css_width,
                title: frame.title,
                document_url: frame.document_url,
                scripts_executed: frame.scripts_executed,
                script_timers_pending: frame.script_timers_pending,
                warnings: frame.warnings,
                script_errors: frame.script_errors,
                links: frame
                    .links
                    .iter()
                    .map(|region| IpcLinkRegion::from_region(region, &frame.document))
                    .collect(),
                images: frame
                    .images
                    .iter()
                    .map(|region| IpcImageRegion::from_region(region, &frame.document))
                    .collect(),
                forms: frame
                    .forms
                    .iter()
                    .map(|control| IpcFormControl::from_control(control, &frame.document))
                    .collect(),
                find_matches: frame
                    .find_matches
                    .iter()
                    .map(|m| IpcFindMatch::from_match(m, &frame.document))
                    .collect(),
                find_rects: frame
                    .find_rects
                    .iter()
                    .copied()
                    .map(IpcRect::from_rect)
                    .collect(),
                document_html: crate::download::serialize_document(&frame.document),
                error: None,
            },
            Err(error) => IpcRenderResponse {
                generation: request.generation,
                png_bytes: Vec::new(),
                content_height: 0,
                css_width: 0,
                title: None,
                document_url: request.input,
                scripts_executed: 0,
                script_timers_pending: 0,
                warnings: 0,
                script_errors: 0,
                links: Vec::new(),
                images: Vec::new(),
                forms: Vec::new(),
                find_matches: Vec::new(),
                find_rects: Vec::new(),
                document_html: String::new(),
                error: Some(error),
            },
        };
        write_message(&mut writer, &response)?;
    }
}

/// A DOM patch sent from the parent to a renderer process: the full post-mutation DOM as a
/// `data:` URL plus the real base URL for relative resource resolution (ADR-004 phase 3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomPatch {
    pub input: String,
    pub base_override: Option<String>,
}

impl DomPatch {
    /// Builds a patch from a parent-side document (the same snapshot strategy used by the
    /// in-process form editor).
    pub fn from_document(document: &megi_webview::core::Document, base_url: &str) -> Self {
        let html = crate::download::serialize_document(document);
        Self {
            input: crate::pages::data_url(&html),
            base_override: (!base_url.is_empty()).then(|| base_url.to_owned()),
        }
    }

    /// Reconstructs a local document from the post-script HTML returned by the child, so the
    /// parent can build further patches without shipping the live DOM across the boundary.
    pub fn restore_document(document_html: &str) -> megi_webview::core::Document {
        megi_webview::html::parse_document(document_html, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_round_trips_request_and_response() {
        let mut buffer = Vec::new();
        let request = IpcRenderRequest {
            generation: 7,
            input: "https://example.com/".to_owned(),
            css_width: 640,
            css_height: 480,
            base_override: None,
            scripting: true,
            find_query: Some("café".to_owned()),
        };
        write_message(&mut buffer, &request).expect("write request");
        assert_eq!(
            read_message::<_, IpcRenderRequest>(&mut &buffer[..]).unwrap(),
            request
        );

        buffer.clear();
        let response = IpcRenderResponse {
            generation: 7,
            png_bytes: vec![0x89, b'P', b'N', b'G'],
            content_height: 1200,
            css_width: 640,
            title: Some("Título".to_owned()),
            document_url: "https://example.com/".to_owned(),
            scripts_executed: 3,
            script_timers_pending: 1,
            warnings: 2,
            script_errors: 0,
            links: Vec::new(),
            images: Vec::new(),
            forms: Vec::new(),
            find_matches: Vec::new(),
            find_rects: Vec::new(),
            document_html: "<!DOCTYPE html><html><head></head><body></body></html>".to_owned(),
            error: None,
        };
        write_message(&mut buffer, &response).expect("write response");
        assert_eq!(
            read_message::<_, IpcRenderResponse>(&mut &buffer[..]).unwrap(),
            response
        );
    }

    #[test]
    fn multiple_messages_are_framed_independently() {
        let mut buffer = Vec::new();
        write_message(&mut buffer, &1u32).unwrap();
        write_message(&mut buffer, &22u32).unwrap();
        let mut cursor = &buffer[..];
        assert_eq!(read_message::<_, u32>(&mut cursor).unwrap(), 1);
        assert_eq!(read_message::<_, u32>(&mut cursor).unwrap(), 22);
    }

    #[test]
    fn truncated_stream_is_a_typed_error() {
        let mut buffer = Vec::new();
        write_message(&mut buffer, &42u32).unwrap();
        buffer.truncate(3);
        assert_eq!(
            read_message::<_, u32>(&mut &buffer[..]),
            Err(IpcError::Truncated)
        );
    }

    #[test]
    fn oversized_length_is_rejected_before_allocating() {
        let header = (MAX_IPC_PAYLOAD as u32 + 1).to_be_bytes();
        assert_eq!(
            read_message::<_, u32>(&mut &header[..]),
            Err(IpcError::TooLarge(MAX_IPC_PAYLOAD as u32 + 1))
        );
    }
}

#[cfg(test)]
#[path = "ipc/tests.rs"]
mod protocol_tests;
