//! Safe IPC framing for per-site renderer processes (ADR-004 phase 3).
//!
//! Messages are length-prefixed JSON (`u32` big-endian length + UTF-8 payload) over a
//! child process's stdin/stdout. No FFI, no unsafe code, no shared memory. The cap on the
//! declared length prevents a hostile peer from forcing an unbounded allocation.

use std::io::{Read, Write};

mod cookie_policy;
#[cfg(test)]
#[path = "ipc/envelope_tests.rs"]
mod envelope_tests;
mod file_grant;
mod options;
mod process;
mod process_pool;
pub use cookie_policy::{CookiePolicyError, seeded_cookies_for, validate_cookie_from_renderer};
pub use file_grant::{FileGrant, FileGrantTable, GrantError, MAX_GRANTED_FILE_BYTES};
pub use options::{IpcCookie, IpcRenderOptions, IpcSameSite};
pub use process::{
    ProcessRenderer, render_png_in_process, render_png_in_process_with_options, renderer_command,
    renderer_scratch_dir,
};
mod regions;
mod sandbox;
mod session;
mod validated;
mod worker_stream;
pub use process_pool::{
    MAX_PROCESS_RENDERERS, ProcessRenderPool, request_process_site, request_sandbox_allows_network,
    request_sandbox_read_path,
};
pub use regions::{
    DocumentNodeOrdinals, DocumentOrdinalNodes, IpcFindMatch, IpcFormControl, IpcImageRegion,
    IpcLinkRegion, IpcRect, IpcSelectOption,
};
pub use session::{ProcessFrame, ProcessRenderSession};
pub use validated::{
    IPC_ENVELOPE_HEADER_LEN, IPC_KIND_REQUEST, IPC_KIND_RESPONSE, IPC_MAGIC, IPC_VERSION,
    MAX_IPC_CSS_DIMENSION, Validated, ValidationError, WireHeader, read_framed_envelope,
    validate_response, write_framed_envelope,
};
pub use worker_stream::{run_ipc_worker, run_ipc_worker_stream, run_ipc_worker_stream_with};

/// Maximum accepted IPC payload (32 MiB cap for control and frame messages).
pub const MAX_IPC_PAYLOAD: usize = 32 * 1024 * 1024;

/// A renderer that has not answered within this deadline is killed (plan Task 3 / T5).
pub const RENDER_WATCHDOG_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Errors from the IPC framing layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpcError {
    /// The peer closed before a complete header/payload arrived.
    Truncated,
    /// The declared length is larger than [`MAX_IPC_PAYLOAD`].
    TooLarge(usize),
    /// The payload is not valid UTF-8 or not valid JSON.
    InvalidPayload(String),
    /// An underlying I/O error.
    Io(String),
    /// The watchdog killed a renderer that did not answer in [`RENDER_WATCHDOG_TIMEOUT`].
    Watchdog(u64),
    /// A response failed wire-header or semantic validation (#2308). The peer is
    /// untrustworthy from this point on, so the slot must be retired.
    Validation(String),
}

impl IpcError {
    /// True when this error means the renderer process is gone (EOF, broken pipe or a
    /// watchdog kill) and the pool must drop the slot and spawn a fresh process.
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            Self::Truncated | Self::Io(_) | Self::Watchdog(_) | Self::Validation(_)
        )
    }
}

impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncated => f.write_str("IPC stream truncated"),
            Self::TooLarge(length) => write!(f, "IPC payload too large ({length} bytes)"),
            Self::InvalidPayload(reason) => write!(f, "invalid IPC payload: {reason}"),
            Self::Io(reason) => write!(f, "IPC I/O error: {reason}"),
            Self::Watchdog(generation) => {
                write!(f, "renderer watchdog timeout for generation {generation}")
            }
            Self::Validation(reason) => write!(f, "IPC validation failed: {reason}"),
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
    if payload.len() > MAX_IPC_PAYLOAD {
        return Err(IpcError::TooLarge(payload.len()));
    }
    let length = u32::try_from(payload.len()).map_err(|_| IpcError::TooLarge(payload.len()))?;
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
    read_message_checked(reader)?.ok_or(IpcError::Truncated)
}

/// Reads a message, distinguishing a clean EOF (`Ok(None)`) from a peer that died
/// mid-frame (`Err(Truncated)`). The worker loop needs that distinction to report a
/// crashed renderer instead of treating it as a normal shutdown.
pub fn read_message_checked<R: Read, T: serde::de::DeserializeOwned>(
    reader: &mut R,
) -> Result<Option<T>, IpcError> {
    let mut header = [0u8; 4];
    // Read the first byte separately: zero bytes means the peer closed cleanly, while
    // 1-3 bytes means a frame was cut short by a crash.
    let first = reader
        .read(&mut header[..1])
        .map_err(|error| IpcError::Io(error.to_string()))?;
    if first == 0 {
        return Ok(None);
    }
    read_exact_or(reader, &mut header[1..])?;
    let length = u32::from_be_bytes(header) as usize;
    if length > MAX_IPC_PAYLOAD {
        return Err(IpcError::TooLarge(length));
    }
    let mut payload = vec![0u8; length];
    read_exact_or(reader, &mut payload)?;
    let value = serde_json::from_slice(&payload)
        .map_err(|error| IpcError::InvalidPayload(error.to_string()))?;
    Ok(Some(value))
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
#[serde(deny_unknown_fields)]
pub struct IpcRenderRequest {
    pub generation: u64,
    pub input: String,
    pub css_width: u32,
    pub css_height: u32,
    pub base_override: Option<String>,
    pub scripting: bool,
    pub find_query: Option<String>,
    #[serde(default)]
    pub options: IpcRenderOptions,
}

/// The serializable part of a rendered frame. The parent rebuilds documentation-dependent
/// state (form controls, live DOM) locally; process mode is documented as a reduced-realm
/// renderer until ADR-004 phase 3 is completed.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
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
    /// Enforcement level obtained by this renderer before reading IPC (`M3#4`).
    #[serde(default)]
    pub sandbox_mode: String,
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
            options: IpcRenderOptions::default(),
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
            sandbox_mode: String::new(),
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
            Err(IpcError::TooLarge(MAX_IPC_PAYLOAD + 1))
        );
    }

    #[test]
    fn oversized_write_is_rejected() {
        let payload = "a".repeat(MAX_IPC_PAYLOAD + 1);
        let mut sink = Vec::new();
        assert!(matches!(
            write_message(&mut sink, &payload),
            Err(IpcError::TooLarge(_))
        ));
    }

    #[test]
    fn ipc_worker_stream_catches_panics_and_continues() {
        let req1 = IpcRenderRequest {
            generation: 1,
            input: "data:text/html,<p>panic</p>".to_string(),
            css_width: 320,
            css_height: 240,
            base_override: None,
            scripting: false,
            find_query: None,
            options: IpcRenderOptions::default(),
        };
        let req2 = IpcRenderRequest {
            generation: 2,
            input: "data:text/html,<p>ok</p>".to_string(),
            css_width: 320,
            css_height: 240,
            base_override: None,
            scripting: false,
            find_query: None,
            options: IpcRenderOptions::default(),
        };
        let mut input = Vec::new();
        let token = [0x33u8; 32];
        write_framed_envelope(&mut input, IPC_KIND_REQUEST, req1.generation, token, &req1).unwrap();
        write_framed_envelope(&mut input, IPC_KIND_REQUEST, req2.generation, token, &req2).unwrap();

        let mut output = Vec::new();
        run_ipc_worker_stream_with(&mut &input[..], &mut output, |req, options| {
            if req.generation == 1 {
                panic!("simulated renderer panic");
            }
            crate::worker::render_document_with_options(req, options)
        })
        .expect("worker stream should survive panic");

        let mut cursor = &output[..];
        let (header1, resp1): (WireHeader, IpcRenderResponse) =
            read_framed_envelope(&mut cursor).unwrap().unwrap();
        assert_eq!(header1.kind, IPC_KIND_RESPONSE);
        assert_eq!(header1.site_token, token);
        assert_eq!(resp1.generation, 1);
        assert!(
            resp1
                .error
                .as_deref()
                .is_some_and(|msg| msg.contains("simulated renderer panic")),
            "expected panic error, got {:?}",
            resp1.error
        );
        let (header2, resp2): (WireHeader, IpcRenderResponse) =
            read_framed_envelope(&mut cursor).unwrap().unwrap();
        assert_eq!(header2.request_id, 2);
        assert_eq!(header2.site_token, token);
        assert_eq!(resp2.generation, 2);
        assert!(resp2.error.is_none());
    }
}

#[cfg(test)]
#[path = "ipc/tests.rs"]
mod protocol_tests;
