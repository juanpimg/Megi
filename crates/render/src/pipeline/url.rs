//! URL entry points and main-document loading for the render pipeline.
//!
//! Moved verbatim from `lib.rs`; public entry points are re-exported at the crate root.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use megi_net::{FetchOptions, ResourceLoader, Scheme, Url};

use super::final_stage::render_parsed_document_inner;
use super::state::ParsedDocumentInput;
use super::svg::render_svg_document;
use crate::{RenderCache, RenderError, RenderOptions, RenderOutput};
use crate::{charset, frames, stylesheets, svg_assets};

/// Loads `input` (an absolute URL: `http(s)://`, `file://` or `data:`) and renders it.
#[cfg(all(feature = "pac", feature = "js"))]
struct JsPacEvaluator;

#[cfg(all(feature = "pac", feature = "js"))]
impl megi_net::PacEvaluator for JsPacEvaluator {
    fn evaluate(&self, script: &str, _url: &str, _host: &str) -> Result<String, String> {
        megi_js::JsRuntime::new()
            .map_err(|e| e.to_string())?
            .eval(script, "proxy.pac")
            .map_err(|e| e.to_string())
    }
}

#[cfg(all(feature = "pac", feature = "js"))]
static INSTALL_PAC: std::sync::Once = std::sync::Once::new();

/// Installs the JS-backed PAC evaluator once. Call before loading any URL when
/// proxy auto-configuration is enabled.
#[cfg(all(feature = "pac", feature = "js"))]
pub fn install_pac_evaluator() {
    INSTALL_PAC.call_once(|| megi_net::set_pac_evaluator(JsPacEvaluator));
}

pub fn render_url(input: &str, options: &RenderOptions) -> Result<RenderOutput, RenderError> {
    #[cfg(all(feature = "pac", feature = "js"))]
    install_pac_evaluator();
    let mut budget = frames::FrameBudget::new(frames::MAX_FRAMES_PER_PAGE);
    render_url_inner(input, options, None, 0, &mut budget)
}

/// Like [`render_url`], but reuses a caller-owned [`RenderCache`] for layout trees.
pub fn render_url_cached(
    input: &str,
    options: &RenderOptions,
    cache: &mut RenderCache,
) -> Result<RenderOutput, RenderError> {
    let mut budget = frames::FrameBudget::new(frames::MAX_FRAMES_PER_PAGE);
    render_url_inner(input, options, Some(cache), 0, &mut budget)
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct FrameRenderContext {
    #[cfg(feature = "js")]
    pub sandbox_flags: Option<megi_js::SandboxFlags>,
    pub skip_scripts: bool,
}

fn render_url_inner(
    input: &str,
    options: &RenderOptions,
    cache: Option<&mut RenderCache>,
    frame_depth: usize,
    budget: &mut frames::FrameBudget,
) -> Result<RenderOutput, RenderError> {
    render_url_with_context(
        input,
        options,
        cache,
        frame_depth,
        FrameRenderContext::default(),
        budget,
    )
}

pub(crate) fn render_url_with_context(
    input: &str,
    options: &RenderOptions,
    cache: Option<&mut RenderCache>,
    frame_depth: usize,
    frame_ctx: FrameRenderContext,
    budget: &mut frames::FrameBudget,
) -> Result<RenderOutput, RenderError> {
    let total_start = Instant::now();
    let url = Url::parse(input)?;
    stylesheets::ensure_supported_scheme(&url)?;
    if options.no_network
        && options.offline_document.is_none()
        && matches!(url.scheme(), Scheme::Http | Scheme::Https)
    {
        return Err(RenderError::NetworkDisabled { url: url.as_str() });
    }

    #[cfg(feature = "js")]
    let opaque_origin = frame_ctx
        .sandbox_flags
        .is_some_and(|flags| !flags.allow_same_origin);
    #[cfg(not(feature = "js"))]
    let opaque_origin = false;

    let fetch_start = Instant::now();
    // One cookie jar per render (or a caller-owned shared one): the document, its
    // subresources and redirect hops all use it. A jar also enables CORS credentials.
    // Sandboxed frames without `allow-same-origin` use an opaque origin and cannot access jars.
    let cookie_jar = if opaque_origin {
        None
    } else {
        options.cookie_jar.clone().or_else(|| {
            options
                .cookies
                .then(|| Arc::new(Mutex::new(megi_net::CookieJar::new())))
        })
    };
    let mut document_fetch_options = options.fetch_options.clone();
    document_fetch_options.cookie_jar = cookie_jar.clone();
    // The top-level navigation request carries no `Origin` header.
    document_fetch_options.origin = None;
    document_fetch_options.accept_push |= options.accept_server_push;
    let mut document_loader = ResourceLoader::with_options(document_fetch_options.clone());
    // M4#6: a service worker can answer a navigation from its cache. The
    // document body is handed to the unchanged HTML/CSS/layout/paint pipeline.
    let mut loaded = match options.offline_document.as_ref() {
        Some(cached) => LoadedDocument {
            bytes: cached.body.clone(),
            warning: None,
            url: url.clone(),
            content_type: cached
                .headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
                .map(|(_, value)| value.clone())
                .unwrap_or_else(|| "text/html; charset=utf-8".to_owned()),
            status: Some(cached.status),
            headers: cached.headers.clone(),
            content_encoding: None,
        },
        None => load_document(
            &url,
            &mut document_loader,
            &document_fetch_options,
            options.stream_responses,
            options.form_post.as_ref(),
        )?,
    };
    // Subresource fetches send the document's origin (CORS) and its cookies.
    let subresource_origin = if opaque_origin {
        None
    } else {
        options
            .origin
            .clone()
            .or_else(|| match loaded.url.scheme() {
                Scheme::Http | Scheme::Https => {
                    megi_net::Origin::from_url(&loaded.url).map(|o| o.as_str())
                }
                Scheme::File | Scheme::Data => None,
            })
    };
    let mut subresource_fetch_options = document_fetch_options.clone();
    subresource_fetch_options.origin = subresource_origin;
    let mut loader = ResourceLoader::with_options(subresource_fetch_options);
    // A remote (or data:) document must not read the local filesystem through a
    // subresource; only file: documents may load file: URLs.
    loader.set_allow_file(loaded.url.is_file());
    // Mixed content (#1528): enforce the document's scheme on every subresource load.
    loader.set_mixed_content_context(Some(loaded.url.scheme()));
    // Push promises received while fetching the document belong to the same load session:
    // transfer them to the subresource loader that serves images/stylesheets/scripts.
    loader.absorb_pushes_from(&mut document_loader);
    let fetch_time = fetch_start.elapsed();
    if loaded.bytes.len() > options.max_document_bytes {
        return Err(RenderError::DocumentTooLarge {
            bytes: loaded.bytes.len(),
            max: options.max_document_bytes,
        });
    }
    // Direct navigation to an SVG document: wrap it in a synthetic `<img>` page so the
    // regular pipeline renders it (intrinsic size, top-left on white), instead of failing
    // with "not HTML". Documented as a simplification (no viewport scaling/centering).
    if svg_assets::looks_like_svg(&loaded.content_type, &loaded.bytes) {
        return render_svg_document(&loaded, options, fetch_time, total_start);
    }

    let decode_start = Instant::now();
    let decoded = charset::decode_html(&loaded.bytes, &loaded.content_type)?;
    if !content_looks_like_html(&loaded.content_type, &loaded.bytes) {
        return Err(RenderError::UnsupportedContentType(
            loaded.content_type.clone(),
        ));
    }
    // Referrer policy for every subresource: HTTP header > `<meta name="referrer">` >
    // browser default. Installing it on the shared loader also enables the automatic
    // cookie `SameSite` classification for subresources (see `megi_net::FetchOptions`).
    let referrer_policy = megi_net::ReferrerPolicy::from_headers(&loaded.headers)
        .or_else(|| megi_net::ReferrerPolicy::from_meta_html(&decoded.text))
        .unwrap_or_default();
    loader.set_referrer(Some(megi_net::ReferrerConfig::with_policy(
        loaded.url.clone(),
        referrer_policy,
    )));
    let mut parse =
        megi_html::parse_document_with_options(&decoded.text, false, &options.parse_options);
    // #2285: expand declarative shadow roots so their content participates in
    // layout/paint (MDN and other modern sites use <template shadowroot>).
    let _ = crate::shadow_dom::expand_declarative_shadow_roots(&mut parse.document);
    let source_mentions_svg = contains_ascii_case_insensitive(decoded.text.as_bytes(), b"<svg");
    let decode_parse_time = decode_start.elapsed();

    // #1632/frame budget: reuse the shared budget threaded through the recursion instead of
    // creating a fresh one per document (which broke the global per-page cap).
    render_parsed_document_inner(ParsedDocumentInput {
        budget,
        document: parse.document,
        document_url: loaded.url,
        document_headers: &loaded.headers,
        options,
        loader,
        cookie_jar,
        referrer_policy,
        source_mentions_svg,
        document_bytes: loaded.bytes.len(),
        charset: decoded.charset,
        status: loaded.status,
        initial_warning: loaded.warning.take(),
        content_encoding: loaded.content_encoding,
        limit_violations: parse.limit_violations,
        cache,
        frame_depth,
        frame_ctx,
        fetch_time,
        decode_parse_time,
        total_start,
    })
}

/// Main document loaded by [`render_url`], before decoding.
/// Detects corporate/CDN error pages served with HTTP 200 (Akamai "Access
/// Denied", Cloudflare "Just a moment...", generic Forbidden). Such bodies are
/// tiny HTML documents whose title or first heading is an error marker, and
/// would otherwise render as a blank page with no warning (#2249).
fn error_page_warning(bytes: &[u8]) -> Option<String> {
    if bytes.len() > 16 * 1024 {
        return None;
    }
    let lower = String::from_utf8_lossy(bytes).to_ascii_lowercase();
    for marker in [
        "access denied",
        "403 forbidden",
        "just a moment...",
        "attention required! | cloudflare",
        "request blocked",
    ] {
        if lower.contains(marker) {
            return Some(format!("possible error page ({marker}) served as HTTP 200"));
        }
    }
    None
}

pub(crate) struct LoadedDocument {
    pub(crate) bytes: Vec<u8>,
    /// Non-fatal diagnostic for a body that looks like a CDN error page (#2249).
    pub(crate) warning: Option<String>,
    pub(crate) url: Url,
    pub(crate) content_type: String,
    pub(crate) status: Option<u16>,
    /// Main-document response headers (empty for `file:`/`data:` navigations).
    pub(crate) headers: Vec<(String, String)>,
    /// Main-document `Content-Encoding`, if any (body already decoded by `net`).
    pub(crate) content_encoding: Option<String>,
}

/// Loads the main document, using `megi_net::fetch` for `http(s)` (to observe the HTTP status)
/// and the cached [`ResourceLoader`] for `file:`/`data:` URLs.
fn load_document(
    url: &Url,
    loader: &mut ResourceLoader,
    fetch_options: &FetchOptions,
    stream: bool,
    form_post: Option<&(String, Vec<u8>)>,
) -> Result<LoadedDocument, RenderError> {
    match url.scheme() {
        Scheme::Http | Scheme::Https => {
            // `RenderOptions::stream_responses` routes the document fetch through the
            // streaming entry point (#1390). The callback receives the decoded body in
            // chunks (the returned `Response::body` is empty on that path), so accumulate
            // it here to keep the deterministic pipeline byte-identical to the buffered
            // path. Status/headers/redirects/pushes come from the same response.
            let mut streamed_body = Vec::new();
            let mut response = if let Some((content_type, body)) = form_post {
                // Form POST navigation (#1538): send the urlencoded body with its content type.
                megi_net::fetch_with_method(
                    url,
                    fetch_options,
                    "POST",
                    &[("Content-Type", content_type.as_str())],
                    Some(body.as_slice()),
                )?
            } else if stream {
                megi_net::fetch_streaming(url, fetch_options, &mut |chunk| {
                    streamed_body.extend_from_slice(chunk);
                    Ok(())
                })?
            } else {
                megi_net::fetch(url, fetch_options)?
            };
            // HTTP/2 server pushes received while fetching the document are part of the
            // same load session: cache them for subresource loads (R177).
            loader.absorb_pushed(std::mem::take(&mut response.pushed));
            if response.status >= 400 {
                return Err(RenderError::HttpStatus {
                    url: response.final_url.as_str(),
                    status: response.status,
                });
            }
            let content_encoding = response.header("content-encoding").map(str::to_owned);
            let bytes = if stream {
                streamed_body
            } else {
                std::mem::take(&mut response.body)
            };
            let content_type = megi_net::resolve_content_type(&response.headers, &bytes);
            let warning = error_page_warning(&bytes);
            Ok(LoadedDocument {
                headers: response.headers,
                warning,
                bytes,
                url: response.final_url,
                content_type,
                status: Some(response.status),
                content_encoding,
            })
        }
        Scheme::File | Scheme::Data => {
            let resource = loader.load(url)?;
            Ok(LoadedDocument {
                warning: error_page_warning(&resource.bytes),
                bytes: resource.bytes.clone(),
                url: resource.url.clone(),
                content_type: resource.content_type.clone(),
                status: None,
                headers: Vec::new(),
                content_encoding: None,
            })
        }
    }
}

/// ASCII case-insensitive substring search (used for the inline-SVG fast path).
fn contains_ascii_case_insensitive(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|window| {
        window
            .iter()
            .zip(needle)
            .all(|(left, right)| left.eq_ignore_ascii_case(right))
    })
}

/// Returns `true` when the resource should be parsed as HTML.
///
/// Accepts a `text/html` / `application/xhtml+xml` content type, or a resource that the
/// MIME sniffer identifies as HTML (browsers sniff `text/plain` navigations the same way;
/// this keeps `file://` and `data:` pages working when the server omits `Content-Type`).
fn content_looks_like_html(content_type: &str, bytes: &[u8]) -> bool {
    let lower = content_type.to_ascii_lowercase();
    if lower.starts_with("text/html") || lower.starts_with("application/xhtml+xml") {
        return true;
    }
    megi_net::sniff(bytes) == megi_net::MimeHint::Html
}
