//! Versioned wire envelope and semantic validation for untrusted IPC peers (F1).
//!
//! Plan: docs/superpowers/plans/2026-11-24-security-isolation-plan.md §Task 4.

use std::io::{Read, Write};

use super::{IpcError, IpcRenderResponse, MAX_IPC_PAYLOAD, read_exact_or};

/// Four-byte frame magic identifying Megi versioned IPC envelopes.
pub const IPC_MAGIC: [u8; 4] = *b"MEGI";

/// Current versioned IPC wire format version.
pub const IPC_VERSION: u16 = 1;

/// Envelope kind for a parent → renderer render request (#2308).
pub const IPC_KIND_REQUEST: u16 = 1;

/// Envelope kind for a renderer → parent render response (#2308).
pub const IPC_KIND_RESPONSE: u16 = 2;

/// Fixed byte length of `magic[4] | version:u16 | kind:u16 | request_id:u64 | site_token:[u8;32] | len:u32`.
pub const IPC_ENVELOPE_HEADER_LEN: usize = 52;

/// Maximum accepted CSS viewport width (and decoded PNG dimension) across IPC.
pub const MAX_IPC_CSS_DIMENSION: u32 = 8192;

/// Maximum serialized `document_html` length accepted from a renderer process (16 MiB).
pub const MAX_IPC_DOCUMENT_HTML_BYTES: usize = 16 * 1024 * 1024;

/// Maximum `document_url` byte length accepted from a renderer process.
pub const MAX_IPC_URL_BYTES: usize = 65_536;

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];
const ALLOWED_SCHEMES: [&str; 5] = ["http://", "https://", "file://", "data:", "about:"];

/// Decoded header of a versioned IPC envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WireHeader {
    pub version: u16,
    pub kind: u16,
    pub request_id: u64,
    pub site_token: [u8; 32],
    pub len: u32,
}

/// Writes one versioned IPC envelope followed by its JSON payload.
pub fn write_framed_envelope<W: Write, T: serde::Serialize>(
    writer: &mut W,
    kind: u16,
    request_id: u64,
    site_token: [u8; 32],
    value: &T,
) -> Result<(), IpcError> {
    let payload =
        serde_json::to_vec(value).map_err(|error| IpcError::InvalidPayload(error.to_string()))?;
    if payload.len() > MAX_IPC_PAYLOAD {
        return Err(IpcError::TooLarge(payload.len()));
    }
    let length = u32::try_from(payload.len()).map_err(|_| IpcError::TooLarge(payload.len()))?;
    let mut header = [0u8; IPC_ENVELOPE_HEADER_LEN];
    header[0..4].copy_from_slice(&IPC_MAGIC);
    header[4..6].copy_from_slice(&IPC_VERSION.to_be_bytes());
    header[6..8].copy_from_slice(&kind.to_be_bytes());
    header[8..16].copy_from_slice(&request_id.to_be_bytes());
    header[16..48].copy_from_slice(&site_token);
    header[48..52].copy_from_slice(&length.to_be_bytes());
    writer
        .write_all(&header)
        .and_then(|()| writer.write_all(&payload))
        .and_then(|()| writer.flush())
        .map_err(|error| IpcError::Io(error.to_string()))
}

/// Reads one versioned IPC envelope, returning `Ok(None)` on clean EOF.
pub fn read_framed_envelope<R: Read, T: serde::de::DeserializeOwned>(
    reader: &mut R,
) -> Result<Option<(WireHeader, T)>, IpcError> {
    let mut header_bytes = [0u8; IPC_ENVELOPE_HEADER_LEN];
    let first = reader
        .read(&mut header_bytes[..1])
        .map_err(|error| IpcError::Io(error.to_string()))?;
    if first == 0 {
        return Ok(None);
    }
    read_exact_or(reader, &mut header_bytes[1..])?;
    if header_bytes[0..4] != IPC_MAGIC {
        return Err(IpcError::InvalidPayload("invalid IPC magic".to_owned()));
    }
    let version = u16::from_be_bytes([header_bytes[4], header_bytes[5]]);
    if version != IPC_VERSION {
        return Err(IpcError::InvalidPayload(format!(
            "unsupported IPC version {version}"
        )));
    }
    let kind = u16::from_be_bytes([header_bytes[6], header_bytes[7]]);
    let mut request_id_bytes = [0u8; 8];
    request_id_bytes.copy_from_slice(&header_bytes[8..16]);
    let request_id = u64::from_be_bytes(request_id_bytes);
    let mut site_token = [0u8; 32];
    site_token.copy_from_slice(&header_bytes[16..48]);
    let len = u32::from_be_bytes([
        header_bytes[48],
        header_bytes[49],
        header_bytes[50],
        header_bytes[51],
    ]);
    let length = len as usize;
    if length > MAX_IPC_PAYLOAD {
        return Err(IpcError::TooLarge(length));
    }
    let mut payload = vec![0u8; length];
    read_exact_or(reader, &mut payload)?;
    let value = serde_json::from_slice(&payload)
        .map_err(|error| IpcError::InvalidPayload(error.to_string()))?;
    let header = WireHeader {
        version,
        kind,
        request_id,
        site_token,
        len,
    };
    Ok(Some((header, value)))
}

/// Semantic rejection reasons when validating an untrusted renderer payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    OversizedDimension,
    StaleGeneration,
    InvalidPng,
    DisallowedScheme,
    PayloadLimitExceeded,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OversizedDimension => f.write_str("IPC dimension exceeds safe limit"),
            Self::StaleGeneration => f.write_str("IPC generation is stale or out of order"),
            Self::InvalidPng => f.write_str("IPC response contains invalid PNG header"),
            Self::DisallowedScheme => f.write_str("IPC document URL uses disallowed scheme"),
            Self::PayloadLimitExceeded => f.write_str("IPC response exceeds payload budget"),
        }
    }
}

/// Newtype proof that an IPC value passed semantic validation at the trust boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validated<T>(T);

impl<T> Validated<T> {
    /// Consumes the proof wrapper and returns the validated value.
    pub fn into_inner(self) -> T {
        self.0
    }

    /// Borrows the validated value.
    pub fn inner(&self) -> &T {
        &self.0
    }
}

/// Validates semantic invariants on a renderer [`IpcRenderResponse`].
pub fn validate_response(
    response: IpcRenderResponse,
    last_generation: u64,
) -> Result<Validated<IpcRenderResponse>, ValidationError> {
    if response.generation <= last_generation
        || response.generation > last_generation.saturating_add(1)
    {
        return Err(ValidationError::StaleGeneration);
    }
    if response.css_width > MAX_IPC_CSS_DIMENSION
        || response.content_height > MAX_IPC_CSS_DIMENSION.saturating_mul(16)
    {
        return Err(ValidationError::OversizedDimension);
    }
    if response.document_html.len() > MAX_IPC_DOCUMENT_HTML_BYTES {
        return Err(ValidationError::PayloadLimitExceeded);
    }
    if !response.png_bytes.is_empty() {
        validate_png_header(&response.png_bytes)?;
    }
    if !response.document_url.is_empty() {
        if response.document_url.len() > MAX_IPC_URL_BYTES {
            return Err(ValidationError::PayloadLimitExceeded);
        }
        let lower = response.document_url.to_ascii_lowercase();
        if !ALLOWED_SCHEMES
            .iter()
            .any(|scheme| lower.starts_with(scheme))
        {
            return Err(ValidationError::DisallowedScheme);
        }
    }
    Ok(Validated(response))
}

fn validate_png_header(bytes: &[u8]) -> Result<(), ValidationError> {
    // ISO/IEC 15948 §5.2–§11.2.2: 8-byte signature + IHDR chunk (4 len + 4 type + 13 data).
    if bytes.len() < 29 || bytes[..8] != PNG_SIGNATURE || &bytes[12..16] != b"IHDR" {
        return Err(ValidationError::InvalidPng);
    }
    let ihdr_len = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    if ihdr_len < 13 {
        return Err(ValidationError::InvalidPng);
    }
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    if width == 0 || height == 0 {
        return Err(ValidationError::InvalidPng);
    }
    if width > MAX_IPC_CSS_DIMENSION || height > MAX_IPC_CSS_DIMENSION {
        return Err(ValidationError::OversizedDimension);
    }
    let rgba_bytes = u64::from(width)
        .saturating_mul(u64::from(height))
        .saturating_mul(4);
    if rgba_bytes > MAX_IPC_PAYLOAD as u64 {
        return Err(ValidationError::PayloadLimitExceeded);
    }
    Ok(())
}
