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
//! - **[`h2`]**: **HTTP/2 MVP client** (feature `tls`): ALPN `h2` negotiation on
//!   the existing `rustls` transport, HPACK (static/dynamic tables, Huffman),
//!   framing (HEADERS/CONTINUATION, DATA, SETTINGS, WINDOW_UPDATE, PING,
//!   RST_STREAM, GOAWAY), one GET at a time, with a transparent fallback to the
//!   HTTP/1.1 client when the server negotiates `http/1.1` (or for non-GET
//!   methods). Limits: no multiplexing, no server push, no full flow control.
//! - **[`loader`]**: [`ResourceLoader`] with the HTTP cache (freshness, conditional
//!   revalidation, `Vary`) that dispatches to `http`/`https`/`file`/`data`, returning
//!   [`loader::Resource`] values (URL + bytes + resolved content type).
//! - **[`cookies`]**: RFC 6265 subset [`CookieJar`] (domain/path/secure/httpOnly/expiry,
//!   `Set-Cookie` parsing, `Cookie` request header) integrated optionally through
//!   [`FetchOptions::cookie_jar`](http::FetchOptions::cookie_jar). `SameSite` enforcement
//!   and a reduced Public Suffix List are covered in the "Browser features" section below.
//! - **[`cors`]**: CORS subset: [`Origin`], response validation with
//!   `Access-Control-Allow-Origin`/`-Credentials`, preflight (`OPTIONS`) validation and
//!   [`http::preflight`], enabled by [`FetchOptions::origin`](http::FetchOptions::origin).
//!   Same-origin/absent-origin requests keep the previous no-policy behaviour.
//! - **[`compression`]**: `Content-Encoding: gzip`/`deflate` decoding (RFC 1952 /
//!   RFC 1950/1951) on top of the shared, dependency-free inflater from the `image`
//!   crate, with anti-bomb caps on the decompressed size (64 MiB) and compression ratio
//!   (1000:1), typed errors, and `Accept-Encoding: gzip` negotiation enabled by default
//!   (disable with [`AcceptEncoding::Identity`]).
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
//! - HTTP/2 (`h2`) supports multiplexed requests with connection/stream flow control,
//!   SETTINGS, request bodies (DATA/END_STREAM), response trailers and interim
//!   `100 Continue`; server push, priorities, h2c and
//!   request bodies are not implemented (HTTP/1.1 remains the transparent fallback).
//!   CORS is the subset described in [`cors`] (no preflight caching, no
//!   `Access-Control-Expose-Headers`).
//! - Compression covers only single `gzip`/`deflate` codings (no `br`/`zstd`, no stacked
//!   encodings) and buffers the whole body before decoding. The inflater is the `image`
//!   crate's std-only codec; extracting it into a dedicated crate is a documented
//!   follow-up. No IDNA / non-ASCII host names (rejected with
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
//!
//! ## Browser features (R109)
//!
//! - **Cookies**: `SameSite=Strict|Lax|None` (None requires `Secure`), enforced through an
//!   approximate [`RequestSite`] model (same-site / cross-site top-level navigation /
//!   cross-site subresource) using a **reduced PSL** ([`psl`]) that also rejects domain
//!   cookies scoped to public suffixes; missing `SameSite` defaults to `Lax`.
//! - **Referrer policy**: all eight [`ReferrerPolicy`] variants, parsed from the response
//!   header ([`Response::referrer_policy`]) or `<meta name="referrer">`
//!   ([`ReferrerPolicy::from_meta_html`]); [`ReferrerConfig`] on [`FetchOptions::referrer`]
//!   makes every HTTP(S) hop send a policy-computed `Referer` (`https`→`http` downgrades
//!   blocked per policy; render should install it once the document is fetched).
//! - **Ranges**: [`ByteRange`] parsing/serialization and [`ContentRange`] response parsing;
//!   [`FetchOptions::range`] sends `Range: bytes=...` (single range only) and
//!   [`ResourceLoader::load_range`] performs a one-shot partial load (HTTP `206`, plus
//!   local slicing of `file:`/`data:` resources; no `If-Range`/resume/streaming).
//! - **Cache-Control**: `no-cache` vs `must-revalidate` are distinguished,
//!   `s-maxage`/`Age`/`Expires` feed [`CachePolicy::freshness_lifetime`] (Age subtracted),
//!   and conditional revalidation (`ETag`/`Last-Modified`) keeps working unchanged.
//!
//! Limits: the PSL is curated (not the full list), the top-level navigation model is
//! approximate, no HTTP/2 or streaming, and range requests bypass the HTTP cache.

#![forbid(unsafe_code)]

pub mod cache;
pub mod compression;
pub mod cookies;
pub mod cors;
pub mod data;
pub mod error;
pub mod file;
pub mod h2;
pub mod http;
pub mod loader;
pub mod mime;
pub mod psl;
pub mod range;
pub mod referrer;
pub mod url;

pub use cache::{CachePolicy, parse_cache_policy, parse_http_date};
pub use compression::{
    AcceptEncoding, CompressionLimits, ContentEncoding, DEFAULT_MAX_COMPRESSION_RATIO,
    DEFAULT_MAX_DECOMPRESSED_BYTES, decode_body as decode_content_encoding,
};
pub use cookies::{
    Clock, Cookie, CookieJar, ParsedSetCookie, RequestSite, SameSite, SetCookieError,
    default_cookie_path, domain_matches, is_valid_cookie_name, parse_set_cookie, path_matches,
};
pub use cors::{Origin, requires_preflight, validate_preflight_response, validate_response};
pub use data::{DataUrl, base64_decode, base64_encode, decode_data_url};
pub use error::{FetchError, LoadError, UrlError};
pub use file::load_file_url;
pub use http::{FetchOptions, Response, fetch, fetch_with_headers, preflight};
pub use loader::{HEURISTIC_FRESHNESS, Resource, ResourceLoader};
pub use mime::{MimeHint, resolve_content_type, sniff};
pub use psl::{
    is_public_suffix, normalize_host, registrable_domain, same_origin, same_site, site_for_url,
};
pub use range::{ByteRange, ContentRange, RangeError};
pub use referrer::{ReferrerConfig, ReferrerPolicy, referer_value};
pub use url::{Scheme, Url};
