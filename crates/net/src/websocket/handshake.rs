//! HTTP upgrade handshake, Sec-WebSocket-Key/Accept and ws URL parsing.

use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::FetchError;
use crate::url::{Scheme, Url};

use super::frame::protocol;

/// Validates an HTTP upgrade response for `key` (RFC 6455 §4.1).
///
/// The status code must be exactly 101 (parsed as the second status-line token, so
/// `200 OK ( 101 reasons)` is rejected), and the headers must include
/// `Upgrade: websocket` plus a `Connection` token `Upgrade` (both case-insensitive),
/// alongside the expected `Sec-WebSocket-Accept` value.
pub fn validate_handshake_response(head: &str, key: &str) -> Result<(), FetchError> {
    let mut lines = head.lines();
    let status_line = lines.next().unwrap_or("");
    let mut parts = status_line.split_whitespace();
    let version = parts.next().unwrap_or("");
    let code = parts.next().unwrap_or("");
    let valid_status =
        version.starts_with("HTTP/") && code.parse::<u16>().is_ok_and(|status| status == 101);
    if !valid_status {
        return Err(protocol(&format!(
            "unexpected handshake status: {status_line}"
        )));
    }
    let mut upgrade: Option<String> = None;
    let mut connection: Option<String> = None;
    let mut accept: Option<String> = None;
    for line in lines {
        if line.trim().is_empty() {
            break;
        }
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("upgrade") {
            upgrade = Some(value.trim().to_owned());
        } else if name.trim().eq_ignore_ascii_case("connection") {
            let entry = connection.get_or_insert_with(String::new);
            if !entry.is_empty() {
                entry.push(',');
            }
            entry.push_str(value.trim());
        } else if name.trim().eq_ignore_ascii_case("sec-websocket-accept") {
            accept = Some(value.trim().to_owned());
        }
    }
    let upgrade_ok = upgrade.is_some_and(|value| value.eq_ignore_ascii_case("websocket"));
    if !upgrade_ok {
        return Err(protocol("missing or invalid Upgrade header"));
    }
    let connection_ok = connection.is_some_and(|value| {
        value
            .split(',')
            .any(|token| token.trim().eq_ignore_ascii_case("upgrade"))
    });
    if !connection_ok {
        return Err(protocol("missing or invalid Connection header"));
    }
    let accept = accept.ok_or_else(|| protocol("missing Sec-WebSocket-Accept"))?;
    if !constant_time_eq(&accept, &accept_value(key)) {
        return Err(protocol("Sec-WebSocket-Accept mismatch"));
    }
    Ok(())
}

/// Parses a WebSocket URL, normalizing it to the HTTP(S) URL the handshake is performed against.
/// `ws://host/path` becomes `http://host/path` and `wss://host/path` becomes `https://host/path`
/// (same default port handling), which the handshake uses to resolve the endpoint and `Host`
/// header while still identifying the scheme as WebSocket at the API boundary. `wss://`
/// handshakes are performed over TLS and are never downgraded to plaintext.
pub fn parse_ws_url(input: &str) -> Result<Url, FetchError> {
    // #1641: do not silently trim — whitespace in a pasted URL is a caller error.
    if input.chars().any(char::is_whitespace) {
        return Err(FetchError::InvalidResponse(
            "invalid ws URL: surrounding or embedded whitespace".to_owned(),
        ));
    }
    // #1644: URL schemes are ASCII case-insensitive; the gate in `WebSocket::connect` also
    // lowercases, so both paths must agree.
    let lower = input.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("wss://") {
        return Url::parse(&format!("https://{rest}"))
            .map_err(|error| FetchError::InvalidResponse(format!("invalid wss URL: {error}")));
    }
    if let Some(rest) = lower.strip_prefix("ws://") {
        return Url::parse(&format!("http://{rest}"))
            .map_err(|error| FetchError::InvalidResponse(format!("invalid ws URL: {error}")));
    }
    let url = Url::parse(input)
        .map_err(|error| FetchError::InvalidResponse(format!("invalid ws URL: {error}")))?;
    if url.scheme() != Scheme::Http && url.scheme() != Scheme::Https {
        return Err(FetchError::UnsupportedScheme(format!(
            "{} (expected ws/wss)",
            url.scheme().as_str()
        )));
    }
    Ok(url)
}

/// Generates a random base64 `Sec-WebSocket-Key` for the upgrade handshake.
pub fn handshake_key(bytes: [u8; 16]) -> String {
    crate::data::base64_encode(&bytes)
}

/// Generates an unpredictable `Sec-WebSocket-Key` (RFC 6455 §4.1). Prefer the OS CSPRNG via
/// `/dev/urandom`; the time/pid/counter fallback is only for platforms without it and is never
/// used on the hosts Megi targets (#1633).
pub(super) fn random_key_bytes() -> [u8; 16] {
    let mut bytes = [0u8; 16];
    if let Ok(mut urandom) = std::fs::File::open("/dev/urandom")
        && urandom.read_exact(&mut bytes).is_ok()
    {
        return bytes;
    }
    static NONCE: AtomicU64 = AtomicU64::new(0);
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64)
        ^ (std::process::id() as u64) << 32
        ^ NONCE.fetch_add(1, Ordering::Relaxed);
    bytes[..8].copy_from_slice(&seed.to_le_bytes());
    bytes[8..].copy_from_slice(&seed.swap_bytes().to_le_bytes());
    bytes
}

/// Constant-time byte comparison for the handshake `Sec-WebSocket-Accept` (#1633). Avoids leaking
/// the expected digest prefix through early-exit timing.
pub(super) fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Computes the expected `Sec-WebSocket-Accept` for a handshake key (RFC 6455 §4.2.2).
pub fn accept_value(key: &str) -> String {
    const GUID: &[u8] =
        concat!("258E", "AFA5-E9", "14-47DA-", "95CA-C5", "AB0DC85", "B11").as_bytes();
    let mut input = Vec::with_capacity(key.len() + GUID.len());
    input.extend_from_slice(key.as_bytes());
    input.extend_from_slice(GUID);
    let digest = crate::integrity::sha1(&input);
    crate::data::base64_encode(&digest)
}
