//! Classic `<script>` discovery and loading (WHATWG HTML §4.12).
//!
//! Discovery walks the parsed DOM in tree order; loading applies the CSP, network, SRI,
//! deduplication and byte/count caps before handing sources to the JS realm.

use std::collections::HashSet;

use megi_core::Document;
use megi_net::{ResourceLoader, Scheme, Url};

use super::types::{LoadedScript, ScriptCandidate, ScriptLoadReport, ScriptingLimits};

/// Returns `true` when `type_attr` denotes a classic JavaScript script.
fn is_classic_script_type(type_attr: Option<&str>) -> bool {
    let Some(raw) = type_attr else {
        return true;
    };
    // MIME types may carry parameters (`text/javascript; charset=utf-8`).
    let essence = raw
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    essence.is_empty()
        || matches!(
            essence.as_str(),
            "text/javascript"
                | "application/javascript"
                | "application/ecmascript"
                | "text/ecmascript"
                | "application/x-javascript"
        )
}

/// Returns `true` when `type_attr` explicitly requests an ES module.
fn is_module_script(type_attr: Option<&str>) -> bool {
    type_attr.is_some_and(|raw| {
        raw.split(';')
            .next()
            .unwrap_or("")
            .trim()
            .eq_ignore_ascii_case("module")
    })
}

/// Discovers classic and module `<script>` elements in document order.
///
/// Unsupported/ignored script types (JSON data blocks, templates, ...) are skipped
/// silently; `<script type="module">` elements are included and executed later through the
/// ES module subset. Discovery has its own (generous) bound; exceeding it is reported in
/// `warnings` instead of silently dropping later scripts (#1222). Execution caps are
/// applied later by `load_scripts` (`max_scripts`/`max_script_attempts`).
pub(crate) const MAX_DISCOVERED_SCRIPTS: usize = 4096;

pub fn discover_scripts(
    document: &Document,
    warnings: &mut Vec<String>,
    max_discovered: usize,
) -> Vec<ScriptCandidate> {
    let mut candidates = Vec::new();
    for element in document.get_elements_by_tag_name("script") {
        if candidates.len() >= max_discovered {
            warnings.push(format!(
                "page references more than {max_discovered} scripts; later scripts ignored"
            ));
            break;
        }
        let Some(data) = document.node(element).as_element() else {
            continue;
        };
        let type_attr = data.get_attribute("type");
        let is_module = is_module_script(type_attr);
        if !is_module && !is_classic_script_type(type_attr) {
            // JSON data blocks, templates and other non-JS types are ignored silently.
            continue;
        }
        if !is_module && data.has_attribute("nomodule") {
            // `nomodule` only affects classic scripts; module scripts ignore it.
            continue;
        }
        let src = data
            .get_attribute("src")
            .map(str::trim)
            .filter(|src| !src.is_empty())
            .map(str::to_owned);
        let inline_text = if src.is_none() {
            Some(document.text_content(element))
        } else {
            None
        };
        candidates.push(ScriptCandidate {
            element,
            inline_text,
            src,
            deferred: data.has_attribute("defer"),
            is_async: data.has_attribute("async"),
            is_module,
            integrity: data.get_attribute("integrity").map(str::to_owned),
            nonce: data.get_attribute("nonce").map(str::to_owned),
            crossorigin: data.has_attribute("crossorigin"),
        });
    }
    candidates
}

/// Loads the external scripts of `candidates` and applies the byte/count/duplication caps.
///
/// `loader` must be the shared [`megi_net::ResourceLoader`] so script fetches share the HTTP
/// cache with stylesheets/images/backgrounds. `no_network` blocks `http(s)` script sources
/// with a warning (offline rendering). All failures are non-fatal.
/// Boa 0.22 rejects `function using(...)` / `function let(...)` (contextual reserved words)
/// even though ECMA-262 allows them as function-expression names in sloppy code (#1969).
/// Rename only the binding name of such function expressions; jQuery UI relies on this.
fn sanitize_reserved_function_names(source: &str) -> String {
    source
        .replace("function using(", "function __megi_using(")
        .replace("function let(", "function __megi_let(")
}

pub fn load_scripts(
    candidates: &[ScriptCandidate],
    base_url: &Url,
    loader: &mut ResourceLoader,
    limits: ScriptingLimits,
    no_network: bool,
    warnings: &mut Vec<String>,
) -> ScriptLoadReport {
    load_scripts_with_csp(
        candidates, base_url, loader, limits, no_network, warnings, None,
    )
}

/// Like [`load_scripts`] but enforces a Content Security Policy (#1526) on every script.
pub fn load_scripts_with_csp(
    candidates: &[ScriptCandidate],
    base_url: &Url,
    loader: &mut ResourceLoader,
    limits: ScriptingLimits,
    no_network: bool,
    warnings: &mut Vec<String>,
    csp: Option<&megi_net::csp::ContentSecurityPolicy>,
) -> ScriptLoadReport {
    let mut report = ScriptLoadReport::default();
    let mut seen_urls: HashSet<String> = HashSet::new();
    // `max_scripts` bounds the scripts that execute; `max_script_attempts` bounds the
    // fetches independently so failing URLs cannot starve later valid scripts.
    let mut attempts = 0usize;
    for (index, candidate) in candidates.iter().enumerate() {
        if report.scripts.len() >= limits.max_scripts {
            report.skipped += 1;
            warnings.push(format!(
                "page references more than {} scripts; only the first {} execute (cap)",
                limits.max_scripts, limits.max_scripts
            ));
            break;
        }
        let source_name;
        let text;
        let mut module_url = None;
        if let Some(src) = &candidate.src {
            let url = match base_url.join(src) {
                Ok(url) => url,
                Err(error) => {
                    report.skipped += 1;
                    warnings.push(format!("skipping script {src:?}: invalid URL ({error})"));
                    continue;
                }
            };
            if no_network && matches!(url.scheme(), Scheme::Http | Scheme::Https) {
                report.skipped += 1;
                warnings.push(format!("skipping script {url} (--no-network is set)"));
                continue;
            }
            // CSP (#1526): block external scripts not allowed by script-src.
            if let Some(policy) = csp
                && !policy.allows_external(&url, base_url)
            {
                report.skipped += 1;
                warnings.push(format!(
                    "skipping script {url}: blocked by Content-Security-Policy"
                ));
                continue;
            }
            // Only module scripts are deduplicated by URL (the HTML module map). A classic
            // external script executes once per element; the loader cache avoids re-fetching
            // the same URL (#1217).
            if candidate.is_module && !seen_urls.insert(url.cache_key()) {
                report.skipped += 1;
                warnings.push(format!(
                    "skipping duplicate module script {url} (already loaded)"
                ));
                continue;
            }
            if attempts >= limits.max_script_attempts {
                report.skipped += 1;
                warnings.push(format!(
                    "page attempted more than {} script loads; the remaining scripts are not fetched (cap)",
                    limits.max_script_attempts
                ));
                break;
            }
            attempts += 1;
            let use_cors = candidate.is_module || candidate.crossorigin;
            let resource = match loader.load_with_mode_checked(&url, true, use_cors) {
                Ok(resource) => resource,
                Err(error) => {
                    report.skipped += 1;
                    warnings.push(format!("skipping script {url}: {error}"));
                    continue;
                }
            };
            if resource.bytes.len() > limits.max_script_bytes {
                report.skipped += 1;
                warnings.push(format!(
                    "skipping script {url}: {} bytes exceeds max_script_bytes={}",
                    resource.bytes.len(),
                    limits.max_script_bytes
                ));
                continue;
            }
            // Subresource Integrity (#1527): verify the decoded bytes before execution and fail
            // closed on mismatch or malformed metadata. An absent `integrity` is a no-op.
            if let Some(integrity) = &candidate.integrity
                && let Err(error) =
                    megi_net::integrity::verify_integrity(&resource.bytes, integrity)
            {
                report.skipped += 1;
                warnings.push(format!(
                    "skipping script {url}: integrity check failed: {error}"
                ));
                continue;
            }
            source_name = url.as_str();
            if candidate.is_module {
                module_url = Some(url.as_str().to_owned());
            }
            text = match String::from_utf8(resource.bytes.clone()) {
                Ok(text) => text,
                Err(error) => {
                    warnings.push(format!(
                        "script {url} is not valid UTF-8; decoding lossily ({error})"
                    ));
                    String::from_utf8_lossy(&resource.bytes).into_owned()
                }
            };
        } else {
            let Some(inline) = &candidate.inline_text else {
                continue;
            };
            // CSP (#1526): block inline scripts unless 'unsafe-inline'/nonce/hash allows them.
            if let Some(policy) = csp
                && !policy.allows_inline(candidate.nonce.as_deref(), inline)
            {
                report.skipped += 1;
                warnings.push(format!(
                    "skipping inline script {index}: blocked by Content-Security-Policy"
                ));
                continue;
            }
            source_name = format!("inline-script-{index}");
            text = inline.clone();
            if candidate.is_module {
                module_url = Some(base_url.as_str().to_owned());
            }
        }

        if text.trim().is_empty() {
            continue;
        }
        if text.len() > limits.max_script_bytes {
            report.skipped += 1;
            warnings.push(format!(
                "skipping script {source_name}: {} bytes exceeds max_script_bytes={}",
                text.len(),
                limits.max_script_bytes
            ));
            continue;
        }
        if report.script_bytes + text.len() > limits.max_total_bytes {
            report.skipped += 1;
            warnings.push(format!(
                "skipping script {source_name}: total script bytes would exceed max_total_bytes={}",
                limits.max_total_bytes
            ));
            continue;
        }
        let text = sanitize_reserved_function_names(&text);
        report.script_bytes += text.len();
        report.scripts.push(LoadedScript {
            element: candidate.element,
            bytes: text.len(),
            text,
            source_name,
            is_module: candidate.is_module,
            module_url,
        });
    }
    report
}

#[cfg(test)]
mod tests {
    use super::sanitize_reserved_function_names;

    #[test]
    fn reserved_contextual_function_names_are_renamed() {
        // #1969: jQuery UI 1.13.2 uses `{using: function using(t){...}}`.
        let source = "var o = {using: function using(t){return t}, x: function let(t){return t}};";
        let fixed = sanitize_reserved_function_names(source);
        assert!(fixed.contains("function __megi_using(t)"));
        assert!(fixed.contains("function __megi_let(t)"));
        assert!(fixed.contains("using: function"));
    }
}
