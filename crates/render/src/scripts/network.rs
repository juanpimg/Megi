//! Network-backed JS API providers: `fetch`, `WebSocket` and cooperative Workers.

use std::sync::Arc;

use megi_net::{ResourceLoader, Scheme, Url};

use super::modules::{file_fetch_allowed, load_worker_source};
use super::types::{ScriptingHostContext, ScriptingLimits};

/// Installs the network-backed providers (inside the worker-enabled gate).
pub(crate) fn install_network_providers(
    runtime: &mut megi_js::JsRuntime,
    document_url: &Url,
    host: &ScriptingHostContext,
    limits: ScriptingLimits,
) {
    if host.enable_workers {
        // R203: fetch() provider backed by the render's effective FetchOptions. Scripts run
        // synchronously; the Promise settles when the host drains microtasks. CORS policy is
        // same-origin only (cross-origin http(s) fetch rejects, documented); file:// misses
        // surface as a synthetic 404 so scripts can inspect `response.ok`.
        {
            let base = document_url.clone();
            let mut options = host.fetch_options.clone();
            // Thread the document origin so the loader can run CORS validation (#1530).
            if options.origin.is_none() {
                let scheme_str = match base.scheme() {
                    Scheme::Http => "http",
                    Scheme::Https => "https",
                    _ => "http",
                };
                let origin = if let Some(port) = base.port_or_default() {
                    format!("{}://{}:{}", scheme_str, base.host(), port)
                } else {
                    format!("{}://{}", scheme_str, base.host())
                };
                options.origin = Some(origin);
            }
            if options.mixed_content.is_none() {
                options.mixed_content =
                    Some(megi_net::MixedContentPolicy::new(base.scheme(), true));
            }
            let no_network = host.no_network;
            let fetch_csp = host.csp.clone();
            // `fetch` volume is budgeted per page: the per-call loader caps bound one response,
            // but a script could still issue thousands of calls (#660).
            let fetch_calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let fetch_bytes = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let max_fetch_calls = limits.max_fetch_calls;
            let max_fetch_bytes = limits.max_fetch_bytes;
            runtime.set_fetch_provider(Arc::new(move |request| {
                let calls = fetch_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                if calls > max_fetch_calls {
                    return Err(megi_js::JsError::Runtime(format!(
                        "fetch budget exhausted ({max_fetch_calls} calls per page)"
                    )));
                }
                let charge = |body: &[u8]| -> Result<(), megi_js::JsError> {
                    let total = fetch_bytes
                        .fetch_add(body.len(), std::sync::atomic::Ordering::Relaxed)
                        + body.len();
                    if total > max_fetch_bytes {
                        return Err(megi_js::JsError::Runtime(format!(
                            "fetch budget exhausted ({max_fetch_bytes} bytes per page)"
                        )));
                    }
                    Ok(())
                };
                let url = Url::parse(&request.url)
                    .map_err(|error| megi_js::JsError::Runtime(format!("fetch URL: {error}")))?;
                if let Some(policy) = &fetch_csp
                    && !policy.allows_connect(&url, &base)
                {
                    return Err(megi_js::JsError::Runtime(format!(
                        "Content-Security-Policy: connect-src blocks {url}"
                    )));
                }
                if matches!(url.scheme(), Scheme::Http | Scheme::Https) && no_network {
                    return Err(megi_js::JsError::Runtime(
                        "network disabled (--no-network)".to_string(),
                    ));
                }
                // Cross-origin http(s) requests are no longer rejected up-front; the loader
                // validates them through CORS when `options.origin` is set (#1530).
                if matches!(url.scheme(), Scheme::File) && base.scheme() != url.scheme() {
                    return Err(megi_js::JsError::Runtime(
                        "CORS: cross-origin fetch denied".to_string(),
                    ));
                }
                // `file://` reads are confined to the document's directory. Answer any
                // out-of-directory request with the same synthetic 404 as a missing file,
                // so scripts cannot read (or probe the existence of) arbitrary local files.
                if matches!(url.scheme(), Scheme::File) && !file_fetch_allowed(&base, &url) {
                    return Ok(megi_js::host::FetchResult {
                        status: 404,
                        status_text: "Not Found".to_string(),
                        url: request.url,
                        headers: Vec::new(),
                        body: Vec::new(),
                    });
                }
                if matches!(url.scheme(), Scheme::Data) {
                    // Decode data: URLs like the loader does (#1224).
                    let data = megi_net::decode_data_url(&url)
                        .map_err(|error| megi_js::JsError::Runtime(format!("data url: {error}")))?;
                    charge(&data.bytes)?;
                    let mut headers = vec![("content-type".to_string(), data.mime_type.clone())];
                    headers.truncate(64);
                    return Ok(megi_js::host::FetchResult {
                        status: 200,
                        status_text: "OK".to_string(),
                        url: url.to_string(),
                        headers,
                        body: data.bytes,
                    });
                }
                if matches!(url.scheme(), Scheme::Http | Scheme::Https) {
                    let header_refs: Vec<(&str, &str)> = request
                        .headers
                        .iter()
                        .map(|(name, value)| (name.as_str(), value.as_str()))
                        .collect();
                    let mut req_options = options.clone();
                    match request.redirect.as_deref() {
                        Some("manual") => {
                            req_options.redirect_mode = megi_net::RedirectMode::Manual
                        }
                        Some("error") => req_options.redirect_mode = megi_net::RedirectMode::Error,
                        _ => req_options.redirect_mode = megi_net::RedirectMode::Follow,
                    }
                    let response = megi_net::http::fetch_with_method(
                        &url,
                        &req_options,
                        &request.method,
                        &header_refs,
                        request.body.as_deref(),
                    )
                    .map_err(|error| megi_js::JsError::Runtime(format!("network: {error}")))?;
                    charge(&response.body)?;
                    return Ok(megi_js::host::FetchResult {
                        status: response.status,
                        status_text: response.reason,
                        url: response.final_url.to_string(),
                        headers: response.headers,
                        body: response.body,
                    });
                }
                let mut loader =
                    ResourceLoader::with_options(options.clone()).with_allow_file(base.is_file());
                loader.set_mixed_content_context(Some(base.scheme()));
                match loader.load_str_checked(&request.url, true) {
                    Ok(resource) => {
                        charge(&resource.bytes)?;
                        let mut headers =
                            vec![("content-type".to_string(), resource.content_type.clone())];
                        headers.truncate(64);
                        Ok(megi_js::host::FetchResult {
                            status: 200,
                            status_text: "OK".to_string(),
                            url: resource.url.to_string(),
                            headers,
                            body: resource.bytes.clone(),
                        })
                    }
                    // A real loader failure (permission, oversized response, unsupported
                    // scheme) must reject the fetch promise, not resolve as a 404 (#1456).
                    // Out-of-directory file access is handled above with a synthetic 404.
                    Err(error) => Err(megi_js::JsError::Runtime(format!("fetch failed: {error}"))),
                }
            }));
        }
        // #1534: connect the JS WebSocket class to the real net WebSocket client. A pump
        // thread handles receives; the transport is polled by the runtime's event loop. This
        // is independent of worker support (WebSocket works in ordinary pages).
        let ws_options = host.fetch_options.clone();
        let ws_no_network = host.no_network;
        let ws_csp = host.csp.clone();
        let ws_doc_url = document_url.clone();
        runtime.set_websocket_provider(Arc::new(move |raw_url| {
            if ws_no_network {
                return Err(megi_js::JsError::Runtime(format!(
                    "WebSocket blocked: --no-network prevents opening {raw_url}"
                )));
            }
            let trimmed = raw_url.trim();
            let (secure_ws, policy_url) = if let Some(rest) = trimmed
                .strip_prefix("wss://")
                .or_else(|| trimmed.strip_prefix("WSS://"))
            {
                let parsed = Url::parse(&format!("https://{rest}")).map_err(|error| {
                    megi_js::JsError::Runtime(format!("WebSocket URL {raw_url}: {error}"))
                })?;
                (true, parsed)
            } else if let Some(rest) = trimmed
                .strip_prefix("ws://")
                .or_else(|| trimmed.strip_prefix("WS://"))
            {
                let parsed = Url::parse(&format!("http://{rest}")).map_err(|error| {
                    megi_js::JsError::Runtime(format!("WebSocket URL {raw_url}: {error}"))
                })?;
                (false, parsed)
            } else {
                return Err(megi_js::JsError::Runtime(format!(
                    "WebSocket URL {raw_url}: expected ws:// or wss://"
                )));
            };
            if ws_doc_url.scheme() == Scheme::Https && !secure_ws {
                return Err(megi_js::JsError::Runtime(format!(
                    "Mixed content blocks insecure WebSocket {raw_url} from https document"
                )));
            }
            if let Some(policy) = &ws_csp
                && !policy.allows_websocket(&policy_url, secure_ws, &ws_doc_url)
            {
                return Err(megi_js::JsError::Runtime(format!(
                    "Content-Security-Policy: connect-src blocks WebSocket {raw_url}"
                )));
            }
            let transport =
                crate::ws_bridge::LiveWebSocketTransport::connect(trimmed, &ws_options)?;
            Ok(Box::new(transport) as Box<dyn megi_js::host::WebSocketTransport>)
        }));
        let worker_allow_file = document_url.is_file();
        let max_worker_bytes = limits.max_script_bytes;
        runtime.set_worker_loader(Arc::new(move |url| {
            load_worker_source(url, worker_allow_file, max_worker_bytes)
        }));
    }
}
