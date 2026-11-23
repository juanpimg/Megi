//! Browser-facing JS API providers: frames, computed style, navigation, storage and cookies.

use std::sync::{Arc, Mutex};

use megi_net::{ResourceLoader, Scheme, Url};

use crate::{HistoryAction, LocationAction};

use super::modules::file_fetch_allowed;
use super::types::ScriptingHostContext;

/// Shared queue of `location.*` actions requested by page scripts.
pub(crate) type LocationActionQueue = Arc<Mutex<Vec<LocationAction>>>;
/// Shared queue of `history.*` actions requested by page scripts.
pub(crate) type HistoryActionQueue = Arc<Mutex<Vec<HistoryAction>>>;
/// The pair of queues returned by the navigation provider installer.
pub(crate) type NavigationActionQueues = (LocationActionQueue, HistoryActionQueue);

#[cfg(feature = "js")]
/// Serialized tuple origin used as the localStorage namespace key. `None` for
/// non-HTTP(S) documents, which keep the legacy flat storage (#1476).
pub(crate) fn storage_origin_key(url: &Url) -> Option<String> {
    megi_net::cors::Origin::from_url(url)
        .map(|origin| format!("{}://{}:{}", origin.scheme, origin.host, origin.port))
}

/// Installs frame loading, sandbox flags and pre-seeded frame documents.
pub(crate) fn install_frame_providers(
    runtime: &mut megi_js::JsRuntime,
    document_url: &Url,
    host: &ScriptingHostContext,
) {
    if let Some(flags) = host.sandbox_flags {
        runtime.set_sandbox_flags(flags);
    }
    runtime.set_frame_depth(host.frame_depth);
    {
        let frame_base = document_url.clone();
        let frame_no_network = host.no_network;
        let frame_csp = host.csp.clone();
        let frame_options = host.fetch_options.clone();
        runtime.set_frame_loader(Arc::new(move |raw_url| {
            let url = Url::parse(raw_url).map_err(|e| format!("invalid frame URL: {e}"))?;
            if frame_no_network && matches!(url.scheme(), Scheme::Http | Scheme::Https) {
                return Err(format!("frame blocked (--no-network): {url}"));
            }
            if let Some(policy) = &frame_csp
                && !policy.allows_frame(&url, &frame_base)
            {
                return Err(format!("frame blocked by Content-Security-Policy: {url}"));
            }
            if url.scheme() == Scheme::File && !file_fetch_allowed(&frame_base, &url) {
                return Err(format!("frame blocked (file access policy): {url}"));
            }
            let mut loader = ResourceLoader::with_options(frame_options.clone())
                .with_allow_file(frame_base.is_file());
            loader.set_mixed_content_context(Some(frame_base.scheme()));
            let resource = loader
                .load_checked(&url, false)
                .map_err(|e| format!("frame load failed: {e}"))?;
            // WHATWG HTML §13.2.3: decode frame documents with the same BOM,
            // HTTP Content-Type charset, and `<meta charset>` prescan rules as
            // top-level documents (#2162).
            let decoded = crate::charset::decode_html(&resource.bytes, &resource.content_type)
                .map_err(|e| format!("frame decode failed: {e}"))?;
            Ok((resource.url.as_str(), decoded.text))
        }));
    }
    for (iframe, frame_document) in &host.frame_documents {
        runtime.set_frame_document(*iframe, frame_document.clone());
    }
}

/// Installs the import map and returns its SRI entries keyed by raw and resolved URL.
pub(crate) fn install_import_map(
    runtime: &mut megi_js::JsRuntime,
    import_map_json: Option<&str>,
    document_url: &Url,
    csp: Option<&megi_net::csp::ContentSecurityPolicy>,
    warnings: &mut Vec<String>,
) -> std::collections::HashMap<String, String> {
    let mut import_map_integrity: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    if let Some(json) = import_map_json {
        if let Ok(parsed) = megi_js::ImportMap::from_json(json) {
            for (raw_url, sri) in parsed.integrity_entries() {
                import_map_integrity.insert(raw_url.to_owned(), sri.to_owned());
                if let Ok(resolved) = document_url.join(raw_url) {
                    import_map_integrity.insert(resolved.as_str().to_owned(), sri.to_owned());
                }
            }
        }
        if let Err(error) = runtime.set_import_map_json(json) {
            warnings.push(format!("invalid import map: {error}"));
        }
    }
    if let Some(policy) = csp {
        runtime.set_allow_eval(policy.allows_eval());
    }
    import_map_integrity
}

/// Installs the pre-script computed-style, media-query and viewport providers.
pub(crate) fn install_style_providers(
    runtime: &mut megi_js::JsRuntime,
    host: &ScriptingHostContext,
) {
    // Browser-facing host providers (R96): getComputedStyle reads the pre-script snapshot,
    // matchMedia evaluates with the render's MediaContext (same viewport as the cascade),
    // and innerWidth/innerHeight/devicePixelRatio come from the real viewport (DPR 1.0).
    if let Some(styles) = host.styles.clone() {
        let layout = host.layout.clone();
        runtime.set_computed_style_provider(Arc::new(move |document, node_id| {
            crate::browser_host::computed_style_for(
                &styles,
                document,
                node_id,
                layout.as_ref().map(std::sync::Arc::as_ref),
            )
        }));
    }
    let media = host.media.clone();
    runtime.set_media_provider(Arc::new(move |query: &str| {
        crate::browser_host::media_matches(&media, query)
    }));
    let viewport_info = megi_js::host::ViewportInfo {
        width: host.viewport.0,
        height: host.viewport.1,
        device_pixel_ratio: 1.0,
        scroll_x: 0.0,
        scroll_y: 0.0,
    };
    runtime.set_viewport_provider(Arc::new(move || viewport_info));
}

/// Installs the pre-script layout-geometry provider (cached or lazy).
pub(crate) fn install_layout_provider(
    runtime: &mut megi_js::JsRuntime,
    host: &ScriptingHostContext,
) {
    #[cfg(feature = "js")]
    if let Some(layout) = host.layout.clone() {
        runtime.set_layout_provider(Arc::new(move |node_id| layout.get(&node_id).copied()));
    } else if let Some(lookup) = host.layout_lookup.clone() {
        runtime.set_layout_provider(lookup);
    } else if let Some(provider) = host.layout_provider.clone() {
        // No OnceLock: a one-shot cache froze the very first measurement for the whole
        // render (#1470). Re-evaluate the provider on every query so later checkpoints see
        // fresh geometry (the pipeline installs `layout_lookup` for its frozen snapshot).
        runtime.set_layout_provider(Arc::new(move |node_id| provider().get(&node_id).copied()));
    }
}

/// Installs the History/Location queue providers and returns the shared action queues.
pub(crate) fn install_navigation_providers(
    runtime: &mut megi_js::JsRuntime,
) -> NavigationActionQueues {
    // Browser-facing host providers (R155): History/location actions are queued for the
    // host (the shell decides whether to navigate/reload after the render) and Web Storage
    // is persisted for `localStorage` when the caller shares `RenderOptions::storage`;
    // `sessionStorage` is always a fresh per-render in-memory area.
    let location_actions: LocationActionQueue = Arc::new(Mutex::new(Vec::new()));
    {
        let queue = Arc::clone(&location_actions);
        runtime.set_location_provider(Arc::new(move |action| {
            queue
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(match action {
                    megi_js::host::LocationAction::Assign(url) => LocationAction::Assign(url),
                    megi_js::host::LocationAction::Replace(url) => LocationAction::Replace(url),
                    megi_js::host::LocationAction::Reload => LocationAction::Reload,
                });
        }));
    }
    let history_actions: HistoryActionQueue = Arc::new(Mutex::new(Vec::new()));
    {
        let queue = Arc::clone(&history_actions);
        runtime.set_history_provider(Arc::new(move |action| {
            queue
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(match action {
                    megi_js::host::HistoryAction::Push { state, url } => {
                        HistoryAction::Push { state, url }
                    }
                    megi_js::host::HistoryAction::Replace { state, url } => {
                        HistoryAction::Replace { state, url }
                    }
                    megi_js::host::HistoryAction::Back => HistoryAction::Back,
                    megi_js::host::HistoryAction::Forward => HistoryAction::Forward,
                    megi_js::host::HistoryAction::Go(delta) => HistoryAction::Go(delta),
                });
        }));
    }
    (location_actions, history_actions)
}

/// Installs the Web Storage and `document.cookie` providers.
pub(crate) fn install_storage_providers(
    runtime: &mut megi_js::JsRuntime,
    document_url: &Url,
    host: &ScriptingHostContext,
) {
    // A tuple origin scopes localStorage so pages from different origins sharing a
    // RenderStorage cannot read or overwrite each other's entries (#1476).
    let origin_key = storage_origin_key(document_url);
    let per_render_local: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let per_render_session: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
    {
        let shared_for_load = host.storage.clone();
        let shared_for_save = host.storage.clone();
        let origin_key_for_save = origin_key.clone();
        let local_load = Arc::clone(&per_render_local);
        let session_load = Arc::clone(&per_render_session);
        let local_save = Arc::clone(&per_render_local);
        let session_save = Arc::clone(&per_render_session);
        runtime.set_storage_provider(megi_js::host::StorageProvider {
            load: Arc::new(move |kind| match kind {
                megi_js::host::StorageKind::Local => match &shared_for_load {
                    Some(storage) => {
                        let storage = storage
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        match &origin_key {
                            Some(key) => storage.by_origin.get(key).cloned().unwrap_or_default(),
                            None => storage.local.clone(),
                        }
                    }
                    None => local_load
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .clone(),
                },
                megi_js::host::StorageKind::Session => session_load
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .clone(),
            }),
            save: Arc::new(move |kind, entries| match kind {
                megi_js::host::StorageKind::Local => match &shared_for_save {
                    Some(storage) => {
                        let mut storage = storage
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        match &origin_key_for_save {
                            Some(key) => {
                                storage.by_origin.insert(key.clone(), entries.to_vec());
                            }
                            None => storage.local = entries.to_vec(),
                        }
                    }
                    None => {
                        *local_save
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner()) = entries.to_vec();
                    }
                },
                megi_js::host::StorageKind::Session => {
                    *session_save
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) = entries.to_vec();
                }
            }),
        });
    }
    {
        let cookie_jar = host
            .cookie_jar
            .clone()
            .or_else(|| host.fetch_options.cookie_jar.clone())
            .unwrap_or_else(|| Arc::new(Mutex::new(megi_net::CookieJar::new())));
        let jar_get = Arc::clone(&cookie_jar);
        let jar_set = Arc::clone(&cookie_jar);
        runtime.set_cookie_provider(megi_js::host::CookieProvider {
            get: Arc::new(move |url| {
                let Ok(parsed) = Url::parse(url) else {
                    return String::new();
                };
                jar_get
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .document_cookie_string(&parsed)
            }),
            set: Arc::new(move |url, value| {
                let Ok(parsed) = Url::parse(url) else {
                    return;
                };
                let _ = jar_set
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .store_from_document(&parsed, value);
            }),
        });
    }
}
