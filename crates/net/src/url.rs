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
//! `blob:`/`ftp:` schemes are not supported. `data:` URLs are parsed and decoded (see
//! [`crate::data`]) but are opaque: they have no host or port, cannot be joined against,
//! and are not CORS- or network-fetchable. All fallible operations return [`UrlError`];
//! the parser never panics.
//!
//! Test vectors are adapted from the WHATWG URL Standard examples and WPT
//! `url/resources/urltestdata.json` cases that fall inside the supported subset.
//!
//! `data:` URLs are parseable and decodable, but opaque (no host, port or relative resolution):
//!
//! ```
//! use megi_net::{Url, decode_data_url};
//!
//! let url = Url::parse("data:text/plain,hello%20world")?;
//! assert_eq!(url.scheme().as_str(), "data");
//! assert_eq!(decode_data_url(&url)?.text(), Some("hello world"));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

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
    /// Returns the lowercase scheme name (`"http"`, `"https"`, `"file"` or `"data"`).
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
            url.query = Some(encode_query(query, self.scheme.is_special()));
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
        // Backslashes are path separators for special schemes, but a `?` or `#` ends the
        // path state: a backslash in the query or fragment is literal data.
        let relative = if self.scheme.is_special() {
            replace_path_backslashes(&cleaned)
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
            query: query.map(|query| encode_query(query, self.scheme.is_special())),
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
        // WHATWG: backslashes are treated as slashes for special schemes, in the path and
        // authority only (`?`/`#` terminate those states).
        let rest = if scheme.is_special() {
            replace_path_backslashes(rest)
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
            query: query.map(|query| encode_query(query, scheme.is_special())),
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
        // Strip credentials so they never leak into cache keys (#1296).
        url.username = String::new();
        url.password = String::new();
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

mod encoding;
mod parsing;

use encoding::{encode_fragment, encode_path, encode_query, hex_value};
use parsing::{
    clean_input, normalize_path, parse_authority, replace_path_backslashes, split_authority,
    split_fragment, split_path_query_fragment, split_scheme,
};

#[cfg(test)]
mod tests;
