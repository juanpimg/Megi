//! # Megi Network (`net`) — Phase 5 bootstrap
//!
//! WHATWG Fetch / resource-loading pipeline slice: URL parsing, HTTP/1.1 with TLS
//! 1.2/1.3 (`https:`), a real HTTP cache, `data:` URLs, `file://` access, MIME type
//! sniffing and a [`ResourceLoader`].
//!
//! ## Scope (what this crate does today)
//!
//! - **[`url`]**: WHATWG URL *subset* parser: `http`, `https` and `file` schemes;
//!   userinfo, domain / IPv4 / bracketed IPv6 hosts, default ports, path normalization
//!   (`.` / `..`), query and fragment, percent-encoding for the WHATWG encode sets, and
//!   [`Url::join`] relative resolution.
//! - **[`http`]**: blocking HTTP/1.1 `GET` over [`std::net::TcpStream`], **TLS 1.2/1.3**
//!   for `https:` with `rustls` + `webpki-roots` (feature `tls`, default on; SNI and
//!   certificate/hostname validation, [`FetchError::Tls`] on failure), connect/read
//!   timeouts, `Content-Length` and `Transfer-Encoding: chunked` bodies, redirects
//!   (301/302/303/307/308) with a limit, response/header size caps.
//! - **[`data`]**: RFC 2397 `data:` URL decoding (mediatype defaults, `;base64` via the
//!   crate's own codec, percent-decoding of raw payloads).
//! - **[`cache`]**: RFC 9111-subset cache policy (`Cache-Control` `max-age`/`no-store`/
//!   `no-cache`, `Expires`, `ETag`/`Last-Modified`, `Vary`, `304` revalidation).
//! - **[`mod@file`]**: `file://` URL → local file bytes (percent-decoded path).
//! - **[`mime`]**: WHATWG MIME Sniffing §5 subset (HTML sniffing, common image/document
//!   signatures, SVG/XML/CSS/JSON heuristics, binary-vs-text fallback) and
//!   [`mime::resolve_content_type`].
//! - **[`loader`]**: [`ResourceLoader`] with the HTTP cache (freshness, conditional
//!   revalidation, `Vary`) that dispatches to `http`/`https`/`file`/`data`, returning
//!   [`loader::Resource`] values (URL + bytes + resolved content type).
//!
//! ## TLS policy (feature `tls`, default on)
//!
//! The trust anchors are the **webpki-roots** Mozilla bundle (compiled in, no system
//! store, no network at startup). Servers are validated against the URL hostname and
//! chain; failures map to [`FetchError::Tls`]. Applications with a private CA inject a
//! `rustls::ClientConfig` through [`http::FetchOptions::tls_config`]. With
//! `--no-default-features` the `tls` feature is off and `https:` yields
//! [`FetchError::TlsUnsupported`].
//!
//! ## Deliberate limitations / next steps (Phase 5 continuation)
//!
//! - No HTTP/2, no cookies, no `Referer`/CORS policy.
//! - **No compression**: `Accept-Encoding` is never sent and `Content-Encoding` is not
//!   decoded (a gzip response is returned raw with its header preserved; see
//!   [`http`] docs). No IDNA / non-ASCII host names (rejected with
//!   [`UrlError::InvalidHost`]), no `ftp:`/`blob:` schemes, no WebSocket.
//! - The URL parser is a *subset*: `` and whitespace handling follow the spec, but odd
//!   legacy forms (e.g. `http:/foo`, Windows `file:` drive letters, octal/hex IPv4) are
//!   rejected or simplified. Existing percent-escape sequences are preserved verbatim
//!   rather than re-encoded.
//! - `file://` paths are treated as UTF-8 Unix paths.
//! - HTTP response bodies are buffered fully in memory (capped by
//!   [`http::FetchOptions::max_response_bytes`]); streaming will arrive with the tile/
//!   image pipeline.
//!
//! ## Tests and network policy
//!
//! The test suite is **fully offline**: every HTTP test starts its own
//! `TcpListener` on `127.0.0.1:0` and serves hand-written HTTP responses, and `file://`
//! tests use temporary files. There are no sleeps (beyond short socket timeouts used as a
//! safety net) and no external connections, so `cargo test -p net` passes in CI without
//! network access.

#![forbid(unsafe_code)]

pub mod cache;
pub mod data;
pub mod error;
pub mod file;
pub mod http;
pub mod loader;
pub mod mime;
pub mod url;

pub use cache::{CachePolicy, parse_cache_policy, parse_http_date};
pub use data::{DataUrl, base64_decode, base64_encode, decode_data_url};
pub use error::{FetchError, LoadError, UrlError};
pub use file::load_file_url;
pub use http::{FetchOptions, Response, fetch, fetch_with_headers};
pub use loader::{HEURISTIC_FRESHNESS, Resource, ResourceLoader};
pub use mime::{MimeHint, resolve_content_type, sniff};
pub use url::{Scheme, Url};
