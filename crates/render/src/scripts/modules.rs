//! ES module resolution/loading and local-file policy for the page's JS realm.
//!
//! The host resolver/loader lets `<script type="module">` graphs resolve against the document
//! URL while sharing the render's CSP, offline and byte-budget policy.

use std::sync::{Arc, Mutex};

use megi_net::{ResourceLoader, Scheme, Url};

use super::types::{LoadedScript, ScriptingHostContext, ScriptingLimits};

/// Returns the JSON source of the first `<script type="importmap">`, recording a warning
/// for duplicates or unparsable maps. Import maps must appear before module scripts; this
/// helper scans the document once up front (the executor runs scripts after parsing).
#[cfg(feature = "js")]
pub(crate) fn import_map_source(
    document: &megi_core::Document,
    max_bytes: usize,
    warnings: &mut Vec<String>,
) -> Option<String> {
    let mut maps = Vec::new();
    for node in document.descendants(document.root()) {
        let Some(element) = document.try_node(node).and_then(|node| node.as_element()) else {
            continue;
        };
        if element.local_name != "script"
            || element
                .get_attribute("type")
                .is_none_or(|kind| !kind.eq_ignore_ascii_case("importmap"))
        {
            continue;
        }
        maps.push(document.text_content(node));
    }
    if maps.len() > 1 {
        warnings.push(format!(
            "multiple import maps ({}); only the first one is used",
            maps.len()
        ));
    }
    let source = maps.into_iter().next()?;
    if source.len() > max_bytes {
        // Import maps bypassed the classic/inline script caps: reject an oversized map
        // before parsing it (issue #465).
        warnings.push(format!(
            "import map is {} bytes, larger than the {max_bytes}-byte script cap; ignored",
            source.len()
        ));
        return None;
    }
    match megi_js::ImportMap::from_json(&source) {
        Ok(_) => Some(source),
        Err(error) => {
            warnings.push(format!("invalid import map: {error}"));
            None
        }
    }
}

/// Whether a `file://` fetch stays inside the document's directory.
///
/// Canonicalising both paths also rejects symlinks that escape the directory; a missing
/// document or target path denies the request (the loader then surfaces its usual 404).
#[cfg(feature = "js")]
pub(crate) fn file_fetch_allowed(base: &Url, requested: &Url) -> bool {
    let Ok(base_path) = megi_net::file::path_from_file_url(base) else {
        return false;
    };
    let Ok(requested_path) = megi_net::file::path_from_file_url(requested) else {
        return false;
    };
    let Some(directory) = base_path.parent() else {
        return false;
    };
    let (Ok(directory), Ok(requested_path)) = (
        std::fs::canonicalize(directory),
        std::fs::canonicalize(&requested_path),
    ) else {
        return false;
    };
    requested_path.starts_with(&directory)
}

/// Resolves a Web Worker script for the host under the same local-file policy as every
/// other subresource, bounded by the per-script byte cap.
#[cfg(feature = "js")]
pub(crate) fn load_worker_source(
    url: &str,
    allow_file: bool,
    max_bytes: usize,
) -> Result<String, megi_js::JsError> {
    // #1646: never echo the resolved absolute file:// URL into an error delivered to page
    // scripts — it discloses the local directory layout. Report only a coarse category.
    let parsed = Url::parse(url)
        .map_err(|_| megi_js::JsError::Runtime("worker script URL is invalid".to_owned()))?;
    if parsed.scheme() != Scheme::File {
        return Err(megi_js::JsError::Runtime(
            "worker scripts only support file:// in render".to_owned(),
        ));
    }
    if !allow_file {
        return Err(megi_js::JsError::Runtime(
            "worker script: local file access is blocked for this document".to_owned(),
        ));
    }
    let bytes = megi_net::load_file_url_with_limit(&parsed, max_bytes)
        .map_err(|_| megi_js::JsError::Runtime("worker script: cannot load".to_owned()))?;
    String::from_utf8(bytes)
        .map_err(|_| megi_js::JsError::Runtime("worker script is not UTF-8".to_owned()))
}

/// Installs the host ES module resolver/loader (R162) when the page needs one.
pub(crate) fn install_module_providers(
    runtime: &mut megi_js::JsRuntime,
    document_url: &Url,
    host: &ScriptingHostContext,
    limits: ScriptingLimits,
    import_map_integrity: std::collections::HashMap<String, String>,
    scripts: &[LoadedScript],
) {
    // ES module support (R162): relative/absolute specifiers resolve against the importing
    // module URL (or the document URL for inline modules) with the net `Url`; the loader
    // reads `file://` from disk or `http(s)` through a dedicated `ResourceLoader` configured
    // with the render's FetchOptions (`--no-network` and size caps are enforced). Each
    // `<script type="module">` is an independent root (no shared graph between scripts).
    if scripts
        .iter()
        .any(|script| script.is_module || script.text.contains("import"))
    {
        runtime.set_module_resolver(Arc::new(
            |specifier: &str, base: &str| -> Result<Option<String>, megi_js::JsError> {
                let resolved = match Url::parse(specifier) {
                    Ok(url) => Some(url),
                    Err(_) => Url::parse(base)
                        .ok()
                        .and_then(|base_url| base_url.join(specifier).ok()),
                };
                Ok(resolved
                    .filter(|url| {
                        matches!(
                            url.scheme(),
                            Scheme::File | Scheme::Http | Scheme::Https | Scheme::Data
                        )
                    })
                    .map(|url| url.as_str().to_owned()))
            },
        ));
        let mut module_loader_inner = ResourceLoader::with_options(host.fetch_options.clone())
            .with_allow_file(document_url.is_file());
        module_loader_inner.set_mixed_content_context(Some(document_url.scheme()));
        let module_loader = Arc::new(Mutex::new(module_loader_inner));
        let no_network = host.no_network;
        let module_csp = host.csp.clone();
        let module_doc_url = document_url.clone();
        let max_module_bytes = limits.max_script_bytes;
        let total_module_bytes = Arc::new(Mutex::new(0usize));
        let max_total_module_bytes = limits.max_total_bytes;
        // The module graph is fetched by the JS module loader, outside `load_scripts`: an
        // unbounded graph of tiny/empty modules would otherwise bypass the attempt cap.
        let module_fetches = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let max_module_fetches = limits.max_module_fetches;
        runtime.set_module_loader(Arc::new(
            move |url_str: &str| -> Result<String, megi_js::JsError> {
                if module_fetches.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    >= max_module_fetches
                {
                    return Err(megi_js::JsError::ModuleNotFound(format!(
                        "module graph exceeded max_module_fetches={max_module_fetches}"
                    )));
                }
                let url = Url::parse(url_str).map_err(|error| {
                    megi_js::JsError::ModuleNotFound(format!(
                        "invalid module URL {url_str}: {error}"
                    ))
                })?;
                if let Some(policy) = &module_csp
                    && !policy.allows_external(&url, &module_doc_url)
                {
                    return Err(megi_js::JsError::ModuleNotFound(format!(
                        "Content-Security-Policy blocks module {url}"
                    )));
                }
                if no_network && matches!(url.scheme(), Scheme::Http | Scheme::Https) {
                    return Err(megi_js::JsError::ModuleNotFound(format!(
                        "cannot load module {url}: --no-network is set"
                    )));
                }
                let resource = module_loader
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .load_checked(&url, true)
                    .map_err(|error| {
                        megi_js::JsError::ModuleNotFound(format!(
                            "cannot load module {url}: {error}"
                        ))
                    })?;
                if resource.bytes.len() > max_module_bytes {
                    return Err(megi_js::JsError::ModuleNotFound(format!(
                        "module {url}: {} bytes exceeds max_script_bytes={max_module_bytes}",
                        resource.bytes.len()
                    )));
                }
                // Subresource Integrity (#1527): verify against `<script type="importmap">`
                // `"integrity"` entries (keyed by resolved URL or raw target string).
                if let Some(expected_sri) = import_map_integrity
                    .get(&url.as_str())
                    .or_else(|| import_map_integrity.get(url_str))
                    && let Err(error) =
                        megi_net::integrity::verify_integrity(&resource.bytes, expected_sri)
                {
                    return Err(megi_js::JsError::ModuleNotFound(format!(
                        "module {url}: integrity check failed: {error}"
                    )));
                }
                let text = String::from_utf8(resource.bytes.clone()).map_err(|error| {
                    megi_js::JsError::ModuleNotFound(format!(
                        "module {url} is not valid UTF-8: {error}"
                    ))
                })?;
                let mut total = total_module_bytes
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if total.saturating_add(text.len()) > max_total_module_bytes {
                    return Err(megi_js::JsError::ModuleNotFound(format!(
                        "module graph exceeds max_total_bytes={max_total_module_bytes}"
                    )));
                }
                *total = total.saturating_add(text.len());
                Ok(text)
            },
        ));
    }
}
