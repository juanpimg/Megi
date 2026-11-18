//! Internal URL parsing helpers (authority, host, path normalization).

use crate::error::UrlError;

use super::Scheme;
use super::encoding::{encode_userinfo, hex_value};

// ---------------------------------------------------------------------------
// Internal parsing helpers
// ---------------------------------------------------------------------------

/// Strips surrounding whitespace/C0 controls and removes embedded tab/LF/CR (§4.4 prelude).
pub(super) fn clean_input(input: &str) -> String {
    input
        .trim_matches(|c: char| c.is_ascii_whitespace() || c.is_ascii_control())
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect()
}

/// Makes a special-scheme reference's path use `/` separators, leaving query/fragment alone.
///
/// The URL parser treats a backslash as a slash only while parsing the authority and path
/// states; `?` and `#` end those, so a backslash after them is literal data (WHATWG URL
/// "path state", "query state" and "fragment state").
pub(super) fn replace_path_backslashes(input: &str) -> String {
    let split = input.find(['?', '#']).unwrap_or(input.len());
    let mut out = String::with_capacity(input.len());
    out.push_str(&input[..split].replace('\\', "/"));
    out.push_str(&input[split..]);
    out
}

/// Splits `scheme:rest` when `input` starts with a syntactically valid scheme.
pub(super) fn split_scheme(input: &str) -> Option<(&str, &str)> {
    let mut chars = input.char_indices();
    let (_, first) = chars.next()?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    for (index, ch) in chars {
        match ch {
            ':' => return Some((&input[..index], &input[index + 1..])),
            'a'..='z' | 'A'..='Z' | '0'..='9' | '+' | '-' | '.' => {}
            _ => return None,
        }
    }
    None
}

/// Splits the authority from the remainder (`/path?query#fragment`).
pub(super) fn split_authority(input: &str) -> (&str, &str) {
    let end = input.find(['/', '?', '#']).unwrap_or(input.len());
    (&input[..end], &input[end..])
}

pub(super) fn split_fragment(input: &str) -> (&str, Option<&str>) {
    match input.split_once('#') {
        Some((before, fragment)) => (before, Some(fragment)),
        None => (input, None),
    }
}

pub(super) fn split_path_query_fragment(input: &str) -> (&str, Option<&str>, Option<&str>) {
    let (before_fragment, fragment) = split_fragment(input);
    let (path, query) = match before_fragment.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (before_fragment, None),
    };
    (path, query, fragment)
}

/// Parses `[userinfo@]host[:port]` into its normalized components.
pub(super) fn parse_authority(
    scheme: Scheme,
    authority: &str,
) -> Result<(String, String, String, Option<u16>), UrlError> {
    let (userinfo, host_port) = match authority.rsplit_once('@') {
        Some((userinfo, host_port)) => (Some(userinfo), host_port),
        None => (None, authority),
    };
    let (username, password) = match userinfo {
        Some(userinfo) => {
            let (username, password) = match userinfo.split_once(':') {
                Some((username, password)) => (username, password),
                None => (userinfo, ""),
            };
            (encode_userinfo(username), encode_userinfo(password))
        }
        None => (String::new(), String::new()),
    };
    if scheme == Scheme::File && (!username.is_empty() || !password.is_empty()) {
        return Err(UrlError::InvalidHost(
            "file: URLs cannot contain credentials".to_owned(),
        ));
    }

    let (raw_host, raw_port) = split_host_port(host_port)?;
    let host = normalize_host(scheme, raw_host)?;
    let port = match raw_port {
        // WHATWG URL "port state": an empty buffer sets the port to null (a validation
        // error, not a failure), so `http://host:/` is valid. `file:` URLs never carry a
        // port, so the stray colon stays a failure there.
        Some("") => {
            if scheme == Scheme::File {
                return Err(UrlError::InvalidPort(String::new()));
            }
            None
        }
        Some(raw_port) => {
            let parsed: u16 = raw_port
                .parse()
                .map_err(|_| UrlError::InvalidPort(raw_port.to_owned()))?;
            if scheme == Scheme::File {
                return Err(UrlError::InvalidPort(raw_port.to_owned()));
            }
            if Some(parsed) == scheme.default_port() {
                None
            } else {
                Some(parsed)
            }
        }
        None => None,
    };
    Ok((username, password, host, port))
}

/// Splits a serialized `host[:port]` string, handling bracketed IPv6 hosts.
pub(super) fn split_host_port(input: &str) -> Result<(&str, Option<&str>), UrlError> {
    if let Some(rest) = input.strip_prefix('[') {
        let Some(close) = rest.find(']') else {
            return Err(UrlError::InvalidHost(input.to_owned()));
        };
        let host = &input[..close + 2];
        let after = &rest[close + 1..];
        if after.is_empty() {
            return Ok((host, None));
        }
        let Some(port) = after.strip_prefix(':') else {
            return Err(UrlError::InvalidHost(input.to_owned()));
        };
        return Ok((host, Some(port)));
    }
    // A bare IPv6 literal is not a valid authority: `::1` and `::1:8080` are ambiguous with
    // `host:port`, so the standard requires brackets. Reject them instead of mis-splitting
    // (#645).
    if input.matches(':').count() > 1 {
        return Err(UrlError::InvalidHost(input.to_owned()));
    }
    match input.rfind(':') {
        Some(index) => Ok((&input[..index], Some(&input[index + 1..]))),
        None => Ok((input, None)),
    }
}

/// Validates and normalizes the host component.
pub(super) fn normalize_host(scheme: Scheme, raw: &str) -> Result<String, UrlError> {
    if raw.is_empty() {
        return if scheme == Scheme::File {
            Ok(String::new())
        } else {
            Err(UrlError::MissingHost)
        };
    }
    if raw.starts_with('[') {
        if !raw.ends_with(']') || raw.len() < 3 {
            return Err(UrlError::InvalidHost(raw.to_owned()));
        }
        let inner = &raw[1..raw.len() - 1];
        // A bracketed host is an IPv6 literal, so validate it as one instead of checking that
        // the characters merely look hexadecimal (#645): `[::::]`, `[12345]` and
        // `[1.2.3.4.5]` used to be accepted and then sent to DNS/SNI/TLS as if valid.
        // `Ipv6Addr` accepts the compressed forms and an embedded IPv4 tail; a link-local
        // zone id (`fe80::1%eth0`) is not part of the host syntax and stays rejected.
        if inner.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(UrlError::InvalidHost(raw.to_owned()));
        }
        return Ok(format!("[{}]", inner.to_ascii_lowercase()));
    }
    if raw
        .chars()
        .any(|ch| !ch.is_ascii() || is_forbidden_host_char(ch))
    {
        return Err(UrlError::InvalidHost(raw.to_owned()));
    }
    // DNS names are at most 255 bytes; cap labels too so PSL/cookie/referrer walks
    // cannot be driven quadratic by a hostile host (#1284).
    if raw.len() > 255 || raw.bytes().filter(|&byte| byte == b'.').count() >= 128 {
        return Err(UrlError::InvalidHost(raw.to_owned()));
    }
    let host = raw.to_ascii_lowercase();
    if scheme == Scheme::File && host == "localhost" {
        return Ok(String::new());
    }
    if host.chars().all(|ch| ch.is_ascii_digit() || ch == '.') {
        return parse_ipv4(&host);
    }
    // Empty labels (e.g. `a..b`) are invalid domains in the full standard; this subset
    // only rejects entirely empty hosts, which was handled above.
    Ok(host)
}

pub(super) fn is_forbidden_host_char(ch: char) -> bool {
    ch.is_ascii_control()
        || matches!(
            ch,
            ' ' | '#' | '/' | ':' | '<' | '>' | '?' | '@' | '[' | '\\' | ']' | '^' | '|' | '%'
        )
}

/// Rejects a dot-separated host unless it is four decimal octets (`0..=255`).
pub(super) fn parse_ipv4(host: &str) -> Result<String, UrlError> {
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() != 4 {
        return Err(UrlError::InvalidHost(host.to_owned()));
    }
    let mut octets = [0u8; 4];
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() || part.len() > 3 || !part.bytes().all(|b| b.is_ascii_digit()) {
            return Err(UrlError::InvalidHost(host.to_owned()));
        }
        octets[index] = part
            .parse()
            .map_err(|_| UrlError::InvalidHost(host.to_owned()))?;
    }
    Ok(format!(
        "{}.{}.{}.{}",
        octets[0], octets[1], octets[2], octets[3]
    ))
}

/// Decodes `%2E` (case-insensitive) to `.` so that dot-segment resolution sees an escaped
/// `..` as traversal. `%2F` is deliberately left encoded: WHATWG keeps it opaque, and
/// decoding it would let `%2f` split a path segment after the fact (#641).
pub(super) fn decode_dot_escapes(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            // WHATWG URL §4.4 decodes only `%2E`; matching the low nibble alone
            // would also turn `%3E`, `%4E`, `%5E`, … into dot segments (#1404).
            && hex_value(bytes[index + 1]) == Some(0x2)
            && hex_value(bytes[index + 2]) == Some(0xE)
        {
            out.push('.');
            index += 3;
            continue;
        }
        let ch = input[index..]
            .chars()
            .next()
            .expect("index on char boundary");
        out.push(ch);
        index += ch.len_utf8();
    }
    out
}

/// Normalizes the path per the WHATWG "path state": resolves `.` and `..` segments and
/// preserves empty segments (repeated slashes). `input` is already percent-encoded.
pub(super) fn normalize_path(input: &str) -> String {
    if !input.starts_with('/') {
        return format!("/{}", input.trim_start_matches('/'));
    }
    let decoded = decode_dot_escapes(input);
    let input = decoded.as_str();
    let trailing_slash =
        input.ends_with('/') || input.ends_with("/.") || input.ends_with("/..") || input == ".";
    let mut segments: Vec<&str> = Vec::new();
    for segment in input.split('/').skip(1) {
        match segment {
            "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    let mut out = String::with_capacity(input.len());
    out.push('/');
    out.push_str(&segments.join("/"));
    if trailing_slash && !out.ends_with('/') {
        out.push('/');
    }
    out
}
