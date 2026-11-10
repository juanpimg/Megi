//! # Megi Network (`net`) — Phase 5 bootstrap
//!
//! First slice of the WHATWG Fetch / resource-loading pipeline: URL parsing, an HTTP/1.1
//! client, `file://` access, MIME type sniffing and an in-memory [`ResourceLoader`] cache.
//!
//! ## Scope (what this crate does today)
//!
//! - **[`url`]**: WHATWG URL *subset* parser: `http`, `https` and `file` schemes;
//!   userinfo, domain / IPv4 / bracketed IPv6 hosts, default ports, path normalization
//!   (`.` / `..`), query and fragment, percent-encoding for the WHATWG encode sets, and
//!   [`Url::join`] relative resolution.
//! - **[`http`]**: blocking HTTP/1.1 `GET` over [`std::net::TcpStream`], connect/read
//!   timeouts, `Content-Length` and `Transfer-Encoding: chunked` bodies, redirects
//!   (301/302/303/307/308) with a limit, response/header size caps. No external crates.
//! - **[`mod@file`]**: `file://` URL → local file bytes (percent-decoded path).
//! - **[`mime`]**: WHATWG MIME Sniffing §5 subset (HTML sniffing, common image/document
//!   signatures, SVG/XML/CSS/JSON heuristics, binary-vs-text fallback) and
//!   [`mime::resolve_content_type`].
//! - **[`loader`]**: [`ResourceLoader`] with an optional in-memory cache keyed by URL,
//!   returning [`loader::Resource`] values (URL + bytes + resolved content type).
//!
//! ## Deliberate limitations / next steps (Phase 5 continuation)
//!
//! - **No TLS**: `https` URLs are parsed and absolutized, but fetching them returns
//!   [`FetchError::TlsUnsupported`]. The next step is `rustls` + `webpki-roots` as
//!   authorized by `docs/ARCHITECTURE.md` §3.1.
//! - No HTTP/2, no cookies, no `Referer`/CORS policy, no compression (no
//!   `Accept-Encoding` is sent and `Content-Encoding` is not decoded).
//! - No IDNA / non-ASCII host names (rejected with [`UrlError::InvalidHost`]), no
//!   percent-encoded hosts, no `data:`/`ftp:`/`blob:` schemes, no WebSocket.
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

pub mod error;
pub mod file;
pub mod http;
pub mod loader;
pub mod mime;
pub mod url;

pub use error::{FetchError, LoadError, UrlError};
pub use file::load_file_url;
pub use http::{FetchOptions, Response, fetch};
pub use loader::{Resource, ResourceLoader};
pub use mime::{MimeHint, resolve_content_type, sniff};
pub use url::{Scheme, Url};
