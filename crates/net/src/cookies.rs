//! HTTP cookies: RFC 6265 subset with a per-jar store, no `document.cookie` surface.
//!
//! ## Supported subset
//!
//! - `Set-Cookie` attributes: `name=value`, `Domain`, `Path`, `Expires`, `Max-Age`,
//!   `Secure`, `HttpOnly` (attribute names are case-insensitive). `Max-Age` takes
//!   precedence over `Expires`; `Max-Age <= 0` or an `Expires` in the past deletes the
//!   cookie.
//! - Domain matching per RFC 6265 §5.1.3 (host-only cookies bind to the exact host;
//!   `Domain` cookies also match subdomains) and path matching per §5.1.4.
//! - Request header construction per §5.4: cookies whose domain/path match the URL,
//!   `Secure` cookies only over `https:`, ordered by longest path first then creation
//!   time.
//! - A jar cap ([`MAX_COOKIES`]) evicts expired cookies first, then the oldest session
//!   cookie, so a hostile server cannot grow the jar without bound.
//!
//! ## Deliberate limitations
//!
//! - `SameSite=Strict|Lax|None` is parsed and enforced against an **approximate**
//!   top-level model: [`RequestSite`] classifies a request as same-site,
//!   cross-site top-level navigation (safe GET) or cross-site subresource. `Strict`
//!   cookies are only sent same-site, `Lax` also on cross-site top-level navigations, and
//!   `None` requires `Secure` (otherwise the cookie is rejected). Missing `SameSite`
//!   defaults to `Lax` (RFC 6265bis browser default), so existing same-site tests keep
//!   passing; [`CookieJar::header_for_url`] keeps the legacy "same-site" behaviour.
//! - A **reduced PSL** ([`crate::psl`]) rejects `Domain` cookies scoped to public
//!   suffixes (e.g. `co.uk`, `github.io`) and is used to compute registrable domains for
//!   the SameSite model. It is a curated subset with a last-label fallback, not the full
//!   list (documented in the PSL module).
//! - No `document.cookie` / JS surface (Phase 6), no `HttpOnly` enforcement beyond the
//!   flag being stored (there is no script engine to hide it from), no cookie prefixes
//!   (`__Host-`/`__Secure-`), no RFC 6265bis `SameSite=Lax-by-default`, no date formats
//!   other than IMF-fixdate (the same parser used for HTTP dates).
//! - The clock is injectable for tests ([`CookieJar::with_clock`]); the default is
//!   `SystemTime::now`. Expired cookies are lazily filtered on read and purged on
//!   write, so an idle jar can still contain stale entries between operations.
//!
//! `fetch` integrates this jar optionally through
//! [`FetchOptions::cookie_jar`](crate::http::FetchOptions::cookie_jar): each response's
//! `Set-Cookie` headers are stored and the matching `Cookie` header is sent on every hop.

mod jar;
mod parse;
mod types;

#[cfg(test)]
mod tests;

pub use jar::CookieJar;
pub use parse::{
    default_cookie_path, domain_matches, is_valid_cookie_name, parse_set_cookie, path_matches,
};
pub use types::{
    Clock, Cookie, MAX_COOKIE_BYTES, MAX_COOKIES, MAX_COOKIES_PER_DOMAIN, ParsedSetCookie,
    RequestSite, SameSite, SetCookieError,
};
