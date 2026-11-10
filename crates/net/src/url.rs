//! WHATWG URL subset parser (`http`, `https` and `file` schemes).
//!
//! Implements the parts of the WHATWG URL Standard
//! (<https://url.spec.whatwg.org/>) needed by the Megi resource pipeline:
//!
//! - scheme detection and normalization (§4.4 "scheme");
//! - special-scheme authority parsing: optional `userinfo@`, domain / IPv4 /
//!   bracketed IPv6 host and port (§4.4, §4.5);
//! - default-port elision (`http:80`, `https:443`) and lowercase host normalization;
//! - path normalization (`.` and `..` segments, §4.4 "path state");
//! - query and fragment separation;
//! - percent-encoding with the WHATWG *path*, *query*, *fragment* and *userinfo* encode
//!   sets (percent-escape sequences already present in the input are preserved verbatim);
//! - relative resolution (`Url::join`), following the "relative state" of §4.4.
//!
//! ## Documented differences from the full standard
//!
//! This is deliberately a subset (the crate documents its limitations):
//! `http:/foo`-style legacy forms are rejected, non-ASCII/IDNA hosts and
//! percent-encoded hosts are rejected, octal/hexadecimal IPv4 forms are rejected, and
//! `data:`/`blob:`/`ftp:` schemes are not supported. All fallible operations return
//! [`UrlError`]; the parser never panics.
//!
//! Test vectors are adapted from the WHATWG URL Standard examples and WPT
//! `url/resources/urltestdata.json` cases that fall inside the supported subset.

use std::fmt;

use crate::error::UrlError;

/// URL schemes supported by this bootstrap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scheme {
    /// `http:` (default port 80).
    Http,
    /// `https:` (default port 443; fetching requires the future TLS stack).
    Https,
    /// `file:` (local filesystem, parsed but not fetched over TCP).
    File,
    /// `data:` (inline bytes; opaque, no host or port).
    Data,
}

impl Scheme {
    /// Returns the lowercase scheme name (`"http"`, `"https"` or `"file"`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
            Self::File => "file",
            Self::Data => "data",
        }
    }

    /// Returns the scheme's default port, if any (WHATWG special schemes).
    pub const fn default_port(self) -> Option<u16> {
        match self {
            Self::Http => Some(80),
            Self::Https => Some(443),
            Self::File | Self::Data => None,
        }
    }

    /// Parses a scheme string (ASCII case-insensitive).
    pub fn parse(input: &str) -> Option<Self> {
        if input.eq_ignore_ascii_case("http") {
            Some(Self::Http)
        } else if input.eq_ignore_ascii_case("https") {
            Some(Self::Https)
        } else if input.eq_ignore_ascii_case("file") {
            Some(Self::File)
        } else if input.eq_ignore_ascii_case("data") {
            Some(Self::Data)
        } else {
            None
        }
    }

    /// Whether this is a WHATWG *special* scheme (backslashes behave as slashes).
    pub const fn is_special(self) -> bool {
        matches!(self, Self::Http | Self::Https | Self::File)
    }

    /// Whether this scheme has a network/file authority (`data:` has none).
    pub const fn has_authority(self) -> bool {
        matches!(self, Self::Http | Self::Https | Self::File)
    }
}

/// A parsed absolute URL (WHATWG URL subset).
///
/// All fields are already normalized and percent-encoded for their component, so
/// [`Url::as_str`] is a stable serialization of the URL and [`Url::request_target`] can be
/// written verbatim onto an HTTP request line.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Url {
    scheme: Scheme,
    username: String,
    password: String,
    /// Serialized host: lowercase domain, dotted-decimal IPv4 or bracketed IPv6.
    /// Empty for `file:///...` URLs (empty host).
    host: String,
    port: Option<u16>,
    /// Serialized, normalized path (always starts with `/`).
    path: String,
    query: Option<String>,
    fragment: Option<String>,
}

impl fmt::Display for Url {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_str())
    }
}

impl Url {
    /// Parses an absolute URL string.
    ///
    /// ASCII tab/newline characters anywhere in the input are removed (WHATWG
    /// "URL parser" pre-processing) and surrounding C0 controls/whitespace are trimmed.
    pub fn parse(input: &str) -> Result<Self, UrlError> {
        let cleaned = clean_input(input);
        if cleaned.is_empty() {
            return Err(UrlError::EmptyInput);
        }
        let (scheme_str, rest) = split_scheme(&cleaned).ok_or(UrlError::MissingScheme)?;
        let scheme = Scheme::parse(scheme_str)
            .ok_or_else(|| UrlError::UnsupportedScheme(scheme_str.to_ascii_lowercase()))?;
        Self::parse_after_scheme(scheme, rest)
    }

    /// Resolves `input` (an absolute or relative URL) against `self`.
    ///
    /// Follows the WHATWG "relative state": empty input keeps the base (minus fragment),
    /// `#frag`/`?query` keep the base path, `//authority/...` is scheme-relative,
    /// `/path` is root-relative and anything else is merged with the base directory.
    pub fn join(&self, input: &str) -> Result<Self, UrlError> {
        let cleaned = clean_input(input);
        if cleaned.is_empty() {
            let mut url = self.clone();
            url.fragment = None;
            return Ok(url);
        }
        if let Some(fragment) = cleaned.strip_prefix('#') {
            let mut url = self.clone();
            url.fragment = Some(encode_fragment(fragment));
            return Ok(url);
        }
        if let Some(rest) = cleaned.strip_prefix('?') {
            let mut url = self.clone();
            let (query, fragment) = split_fragment(rest);
            url.query = Some(encode_query(query));
            url.fragment = fragment.map(encode_fragment);
            return Ok(url);
        }
        if let Some((scheme_str, rest)) = split_scheme(&cleaned) {
            let scheme = Scheme::parse(scheme_str)
                .ok_or_else(|| UrlError::UnsupportedScheme(scheme_str.to_ascii_lowercase()))?;
            return Self::parse_after_scheme(scheme, rest);
        }
        // `data:` URLs are opaque and cannot act as a base for relative references.
        if self.scheme == Scheme::Data {
            return Err(UrlError::UnsupportedScheme("data".to_owned()));
        }
        // Backslashes are path separators for special schemes.
        let relative = if self.scheme.is_special() {
            cleaned.replace('\\', "/")
        } else {
            cleaned
        };
        if let Some(scheme_relative) = relative.strip_prefix("//") {
            return Self::parse_after_scheme(self.scheme, &format!("//{scheme_relative}"));
        }

        let (path_raw, query, fragment) = split_path_query_fragment(&relative);
        let encoded_path = if path_raw.starts_with('/') {
            encode_path(path_raw)
        } else {
            let base_dir = match self.path.rfind('/') {
                Some(index) => &self.path[..=index],
                None => "/",
            };
            encode_path(&format!("{base_dir}{path_raw}"))
        };
        let path = normalize_path(&encoded_path);
        Ok(Self {
            scheme: self.scheme,
            username: self.username.clone(),
            password: self.password.clone(),
            host: self.host.clone(),
            port: self.port,
            path,
            query: query.map(encode_query),
            fragment: fragment.map(encode_fragment),
        })
    }

    /// Parses the part after `scheme:` (which may start with `//authority`).
    fn parse_after_scheme(scheme: Scheme, rest: &str) -> Result<Self, UrlError> {
        if scheme == Scheme::Data {
            // WHATWG URL: `data:` is an opaque path. The payload keeps its raw
            // bytes (percent-encoded as written) until `crate::data` decodes it;
            // only the fragment is split off.
            let (payload, fragment) = split_fragment(rest);
            return Ok(Self {
                scheme,
                username: String::new(),
                password: String::new(),
                host: String::new(),
                port: None,
                path: payload.to_owned(),
                query: None,
                fragment: fragment.map(encode_fragment),
            });
        }
        // WHATWG: backslashes are treated as slashes for special schemes.
        let rest = if scheme.is_special() {
            rest.replace('\\', "/")
        } else {
            rest.to_string()
        };
        let (authority, after) = if let Some(stripped) = rest.strip_prefix("//") {
            split_authority(stripped)
        } else if scheme == Scheme::File {
            // `file:/path` is accepted as an empty-authority file URL.
            ("", rest.as_str())
        } else {
            // `http:host/path` is treated like `http://host/path`.
            split_authority(rest.as_str())
        };

        let (username, password, host, port) = parse_authority(scheme, authority)?;
        let (path_raw, query, fragment) = split_path_query_fragment(after);
        let path_input = if path_raw.is_empty() { "/" } else { path_raw };
        let path = normalize_path(&encode_path(path_input));

        Ok(Self {
            scheme,
            username,
            password,
            host,
            port,
            path,
            query: query.map(encode_query),
            fragment: fragment.map(encode_fragment),
        })
    }

    /// Returns the URL's scheme.
    pub const fn scheme(&self) -> Scheme {
        self.scheme
    }

    /// Returns the normalized username (empty when absent).
    pub fn username(&self) -> &str {
        &self.username
    }

    /// Returns the normalized password (empty when absent).
    pub fn password(&self) -> &str {
        &self.password
    }

    /// Returns the serialized host (brackets included for IPv6; empty for `file:///...`).
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Returns the host suitable for DNS resolution: IPv6 brackets are stripped.
    pub fn host_for_dns(&self) -> &str {
        self.host
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
            .unwrap_or(&self.host)
    }

    /// Returns the explicit, non-default port if one was present.
    pub const fn port(&self) -> Option<u16> {
        self.port
    }

    /// Returns the effective port, falling back to the scheme's default.
    pub fn port_or_default(&self) -> Option<u16> {
        self.port.or_else(|| self.scheme.default_port())
    }

    /// Returns the normalized, percent-encoded path (always starts with `/`).
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the percent-encoded query string (without the leading `?`).
    pub fn query(&self) -> Option<&str> {
        self.query.as_deref()
    }

    /// Returns the percent-encoded fragment (without the leading `#`).
    pub fn fragment(&self) -> Option<&str> {
        self.fragment.as_deref()
    }

    /// Returns the HTTP request target: path plus `?query` when present.
    pub fn request_target(&self) -> String {
        match &self.query {
            Some(query) => format!("{}?{query}", self.path),
            None => self.path.clone(),
        }
    }

    /// Returns the URL serialized without its fragment (the cache key / fetch identity).
    pub fn cache_key(&self) -> String {
        let mut url = self.clone();
        url.fragment = None;
        url.as_str()
    }

    /// Returns `true` when the URL denotes the local filesystem (`file:` scheme).
    pub const fn is_file(&self) -> bool {
        matches!(self.scheme, Scheme::File)
    }

    /// Serializes the URL back to a string (WHATWG "URL serializer" subset).
    pub fn as_str(&self) -> String {
        let mut out = String::with_capacity(
            self.scheme.as_str().len()
                + self.host.len()
                + self.path.len()
                + self.username.len()
                + self.password.len()
                + 8,
        );
        if self.scheme == Scheme::Data {
            out.push_str("data:");
            out.push_str(&self.path);
            if let Some(fragment) = &self.fragment {
                out.push('#');
                out.push_str(fragment);
            }
            return out;
        }
        out.push_str(self.scheme.as_str());
        out.push_str("://");
        if !self.username.is_empty() || !self.password.is_empty() {
            out.push_str(&self.username);
            if !self.password.is_empty() {
                out.push(':');
                out.push_str(&self.password);
            }
            out.push('@');
        }
        out.push_str(&self.host);
        if let Some(port) = self.port {
            out.push(':');
            out.push_str(&port.to_string());
        }
        out.push_str(&self.path);
        if let Some(query) = &self.query {
            out.push('?');
            out.push_str(query);
        }
        if let Some(fragment) = &self.fragment {
            out.push('#');
            out.push_str(fragment);
        }
        out
    }
}

/// Decodes `%XX` escape sequences in `input` as UTF-8 (lossily for invalid sequences).
///
/// Invalid escapes are kept verbatim. Used to turn `file:` URL paths into filesystem paths.
pub fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
        {
            out.push(high * 16 + low);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ---------------------------------------------------------------------------
// Internal parsing helpers
// ---------------------------------------------------------------------------

/// Strips surrounding whitespace/C0 controls and removes embedded tab/LF/CR (§4.4 prelude).
fn clean_input(input: &str) -> String {
    input
        .trim_matches(|c: char| c.is_ascii_whitespace() || c.is_ascii_control())
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect()
}

/// Splits `scheme:rest` when `input` starts with a syntactically valid scheme.
fn split_scheme(input: &str) -> Option<(&str, &str)> {
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
fn split_authority(input: &str) -> (&str, &str) {
    let end = input.find(['/', '?', '#']).unwrap_or(input.len());
    (&input[..end], &input[end..])
}

fn split_fragment(input: &str) -> (&str, Option<&str>) {
    match input.split_once('#') {
        Some((before, fragment)) => (before, Some(fragment)),
        None => (input, None),
    }
}

fn split_path_query_fragment(input: &str) -> (&str, Option<&str>, Option<&str>) {
    let (before_fragment, fragment) = split_fragment(input);
    let (path, query) = match before_fragment.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (before_fragment, None),
    };
    (path, query, fragment)
}

/// Parses `[userinfo@]host[:port]` into its normalized components.
fn parse_authority(
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
fn split_host_port(input: &str) -> Result<(&str, Option<&str>), UrlError> {
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
    match input.rfind(':') {
        Some(index) => Ok((&input[..index], Some(&input[index + 1..]))),
        None => Ok((input, None)),
    }
}

/// Validates and normalizes the host component.
fn normalize_host(scheme: Scheme, raw: &str) -> Result<String, UrlError> {
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
        let valid = inner
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() || matches!(ch, ':' | '.'));
        if !valid {
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

fn is_forbidden_host_char(ch: char) -> bool {
    ch.is_ascii_control()
        || matches!(
            ch,
            ' ' | '#' | '/' | ':' | '<' | '>' | '?' | '@' | '[' | '\\' | ']' | '^' | '|' | '%'
        )
}

/// Rejects a dot-separated host unless it is four decimal octets (`0..=255`).
fn parse_ipv4(host: &str) -> Result<String, UrlError> {
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

/// Normalizes the path per the WHATWG "path state": resolves `.` and `..` segments and
/// preserves empty segments (repeated slashes). `input` is already percent-encoded.
fn normalize_path(input: &str) -> String {
    if !input.starts_with('/') {
        return format!("/{}", input.trim_start_matches('/'));
    }
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

// ---------------------------------------------------------------------------
// Percent encoding (WHATWG encode sets)
// ---------------------------------------------------------------------------

const HEX_DIGITS: [u8; 16] = *b"0123456789ABCDEF";

fn push_encoded_byte(out: &mut String, byte: u8) {
    out.push('%');
    out.push(HEX_DIGITS[(byte >> 4) as usize] as char);
    out.push(HEX_DIGITS[(byte & 0x0F) as usize] as char);
}

fn encode_with(input: &str, requires_encoding: fn(u8) -> bool) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii() {
            let byte = ch as u8;
            if requires_encoding(byte) {
                push_encoded_byte(&mut out, byte);
            } else {
                out.push(ch);
            }
        } else {
            let mut buffer = [0u8; 4];
            for byte in ch.encode_utf8(&mut buffer).bytes() {
                push_encoded_byte(&mut out, byte);
            }
        }
    }
    out
}

/// WHATWG *path percent-encode set*: C0 controls, space, `" # < > ? ` { }` and non-ASCII.
fn path_requires_encoding(byte: u8) -> bool {
    !(0x20..=0x7E).contains(&byte)
        || matches!(
            byte,
            b' ' | b'"' | b'#' | b'<' | b'>' | b'?' | b'`' | b'{' | b'}'
        )
}

/// WHATWG *query percent-encode set* (a subset of the special-query set: no `'`).
fn query_requires_encoding(byte: u8) -> bool {
    !(0x20..=0x7E).contains(&byte) || matches!(byte, b' ' | b'"' | b'#' | b'<' | b'>')
}

/// WHATWG *fragment percent-encode set*.
fn fragment_requires_encoding(byte: u8) -> bool {
    !(0x20..=0x7E).contains(&byte) || matches!(byte, b' ' | b'"' | b'<' | b'>' | b'`')
}

/// WHATWG *userinfo percent-encode set*.
fn userinfo_requires_encoding(byte: u8) -> bool {
    path_requires_encoding(byte)
        || matches!(
            byte,
            b'/' | b':' | b';' | b'=' | b'@' | b'[' | b'\\' | b']' | b'^' | b'|'
        )
}

fn encode_path(input: &str) -> String {
    encode_with(input, path_requires_encoding)
}

fn encode_query(input: &str) -> String {
    encode_with(input, query_requires_encoding)
}

fn encode_fragment(input: &str) -> String {
    encode_with(input, fragment_requires_encoding)
}

fn encode_userinfo(input: &str) -> String {
    encode_with(input, userinfo_requires_encoding)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn percent_decode_roundtrip_for_ascii_and_utf8() {
        assert_eq!(percent_decode("/tmp/a%20b"), "/tmp/a b");
        assert_eq!(percent_decode("/caf%C3%A9"), "/café");
        assert_eq!(percent_decode("/bad%2"), "/bad%2");
        assert_eq!(percent_decode("/bad%zz"), "/bad%zz");
    }

    #[test]
    fn normalize_path_resolves_dot_segments() {
        assert_eq!(normalize_path("/a/./b/../c"), "/a/c");
        assert_eq!(normalize_path("/a/b/.."), "/a/");
        assert_eq!(normalize_path("/a/b/."), "/a/b/");
        assert_eq!(normalize_path("/a//b"), "/a//b");
        assert_eq!(normalize_path("/../x"), "/x");
        assert_eq!(normalize_path("/"), "/");
    }
}
