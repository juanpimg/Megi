//! Unit tests for the scripting stage.

use super::*;

#[cfg(feature = "js")]
use std::sync::Arc;

use megi_core::Document;
use megi_net::{ResourceLoader, Url};

#[cfg(test)]
mod unit_tests {
    use super::*;

    fn document_with(html: &str) -> Document {
        megi_html::parse_document(html, false)
    }

    #[cfg(not(feature = "js"))]
    #[test]
    fn scripting_defaults_to_disabled_without_the_feature() {
        assert_eq!(
            crate::ScriptingMode::default(),
            crate::ScriptingMode::Disabled
        );
    }

    #[cfg(feature = "js")]
    #[test]
    fn oversized_import_maps_are_ignored() {
        let document = document_with(
            "<body><script type='importmap'>{\"imports\":{\"a\":\"b\"}}</script></body>",
        );
        let mut warnings = Vec::new();
        assert!(import_map_source(&document, 8, &mut warnings).is_none());
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("import map is")),
            "{warnings:?}"
        );
        let mut warnings = Vec::new();
        assert!(import_map_source(&document, 1024, &mut warnings).is_some());
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn discovers_classic_inline_and_external_scripts_in_order() {
        let doc = document_with(
            "<html><head><script src='a.js'></script><script type='text/javascript'>x()</script></head>\
             <body><script type='application/json'>{}</script><script type='module'>m()</script>\
             <script nomodule>n()</script><script>y()</script></body></html>",
        );
        let mut warnings = Vec::new();
        let candidates = discover_scripts(&doc, &mut warnings, MAX_DISCOVERED_SCRIPTS);
        // Classic scripts + the inline module are candidates; `application/json`,
        // `nomodule` and unsupported types are skipped.
        assert_eq!(candidates.len(), 4);
        assert_eq!(candidates[0].src.as_deref(), Some("a.js"));
        assert_eq!(candidates[1].inline_text.as_deref(), Some("x()"));
        assert!(candidates[2].is_module);
        assert_eq!(candidates[2].inline_text.as_deref(), Some("m()"));
        assert_eq!(candidates[3].inline_text.as_deref(), Some("y()"));
        assert!(
            warnings.is_empty(),
            "module discovery is not a warning: {warnings:?}"
        );
    }

    #[test]
    fn script_discovery_reports_its_cap() {
        let mut html = String::new();
        for _ in 0..1100 {
            html.push_str("<script>x()</script>");
        }
        let doc = document_with(&html);
        let mut warnings = Vec::new();
        let candidates = discover_scripts(&doc, &mut warnings, 1024);
        assert_eq!(candidates.len(), 1024);
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("more than 1024 scripts")),
            "silently dropping later scripts is a defect (#1222): {warnings:?}"
        );
    }

    #[cfg(feature = "js")]
    #[test]
    fn console_warnings_have_no_terminal_control_bytes() {
        // #2309: `console.warn` text comes from the page and is mirrored into
        // `RenderStats::warnings`; ESC/BEL must never reach the operator's stderr.
        let script_text = "console.warn('ready \\u001b[31mBOOM\\u0007 \\u009b1mESC');";
        let document = document_with(&format!(
            "<!DOCTYPE html><body><script>{script_text}</script></body>"
        ));
        let scripts = vec![LoadedScript {
            element: document.root(),
            text: script_text.to_owned(),
            source_name: "inline-script-0".to_owned(),
            bytes: script_text.len(),
            is_module: false,
            module_url: None,
        }];
        let host = ScriptingHostContext::new(
            None,
            megi_css::MediaContext::with_viewport(320.0, 200.0),
            (320, 200),
        );
        let mut warnings = Vec::new();
        let _execution = execute_scripts(
            document,
            &Url::parse("https://example.test/").expect("url"),
            scripts,
            ScriptingLimits::default(),
            &host,
            &mut warnings,
        );
        assert!(
            warnings.iter().any(|w| w.starts_with("console.warn:")),
            "{warnings:?}"
        );
        for warning in &warnings {
            assert!(
                warning.chars().all(|c| !c.is_control()),
                "terminal control byte in warning: {warning:?}"
            );
        }
    }

    #[cfg(feature = "js")]
    #[test]
    fn worker_loader_enforces_the_file_policy() {
        let path = std::env::temp_dir().join(format!("megi-worker-{}.js", std::process::id()));
        std::fs::write(&path, "self.onmessage = function () {};").expect("write worker");
        let url = format!("file://{}", path.display());

        let blocked = load_worker_source(&url, false, 4096).expect_err("blocked");
        assert!(blocked.to_string().contains("blocked"), "{blocked}");
        let source = load_worker_source(&url, true, 4096).expect("allowed");
        assert!(source.contains("onmessage"), "{source}");
        assert!(load_worker_source(&url, true, 4).is_err(), "cap");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_scripts_applies_network_and_duplicate_policy() {
        let doc = document_with(
            "<html><body><script src='/a.js'></script><script src='/a.js'></script>\
             <script src='https://example.com/b.js'></script><script>x()</script></body></html>",
        );
        let base = Url::parse("file:///tmp/index.html").expect("file URL");
        let mut loader = ResourceLoader::with_options(megi_net::FetchOptions::default());
        let mut warnings = Vec::new();
        let candidates = discover_scripts(&doc, &mut warnings, MAX_DISCOVERED_SCRIPTS);
        let report = load_scripts(
            &candidates,
            &base,
            &mut loader,
            ScriptingLimits::default(),
            true,
            &mut warnings,
        );
        // /a.js is skipped (missing file) and the duplicate/newtork script too; only inline runs.
        assert_eq!(report.scripts.len(), 1);
        assert_eq!(report.scripts[0].text, "x()");
        assert_eq!(report.scripts[0].source_name, "inline-script-3");
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("--no-network"))
        );
    }
    #[cfg(feature = "js")]
    #[test]
    fn file_fetch_is_confined_to_the_document_directory() {
        let root = std::env::temp_dir().join(format!("megi-file-fetch-{}", std::process::id()));
        let site = root.join("site");
        std::fs::create_dir_all(&site).expect("mkdir");
        std::fs::write(site.join("index.html"), "x").expect("doc");
        std::fs::write(site.join("data.txt"), "x").expect("inside");
        std::fs::write(root.join("secret.txt"), "x").expect("outside");
        let base = Url::parse(&format!("file://{}/index.html", site.display())).expect("base");
        let inside = Url::parse(&format!("file://{}/data.txt", site.display())).expect("inside");
        let outside =
            Url::parse(&format!("file://{}/secret.txt", root.display())).expect("outside");
        let system = Url::parse("file:///etc/passwd").expect("system");
        assert!(file_fetch_allowed(&base, &inside));
        assert!(!file_fetch_allowed(&base, &outside));
        assert!(!file_fetch_allowed(&base, &system));
        // A `..` traversal that resolves outside the directory is denied too.
        let traversal = Url::parse(&format!("file://{}/site/../secret.txt", root.display()))
            .expect("traversal");
        assert!(!file_fetch_allowed(&base, &traversal));
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(all(test, feature = "js"))]
mod storage_origin_tests {
    use super::*;

    #[test]
    fn storage_namespace_uses_the_tuple_origin() {
        let a = Url::parse("http://example.com:8080/a").expect("url");
        let b = Url::parse("http://example.com:8081/b").expect("url");
        let c = Url::parse("https://example.com/b").expect("url");
        assert_ne!(
            storage_origin_key(&a),
            storage_origin_key(&b),
            "different ports are different origins"
        );
        assert_ne!(
            storage_origin_key(&a),
            storage_origin_key(&c),
            "different schemes are different origins"
        );
        assert_eq!(
            storage_origin_key(&a),
            Some("http://example.com:8080".into())
        );
        assert_eq!(
            storage_origin_key(&Url::parse("file:///tmp/x.html").expect("url")),
            None
        );
    }
}

#[cfg(all(test, feature = "js"))]
mod layout_provider_freshness_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn layout_provider_is_not_frozen_after_the_first_query() {
        // Drive Megi's real geometry bridge (#1470): JS `getBoundingClientRect()` must
        // re-invoke the installed LayoutProvider on every query, so later measurements
        // observe fresh geometry instead of a one-shot cached rect.
        use megi_js::host::LayoutRect;
        let mut runtime = megi_js::JsRuntime::new().expect("runtime");
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&calls);
        let provider: megi_js::host::LayoutProvider = Arc::new(move |_node| {
            let n = counter.fetch_add(1, Ordering::SeqCst);
            let width = 10.0 + n as f32;
            Some(LayoutRect {
                border_box: (0.0, 0.0, width, 10.0),
                content_box: (0.0, 0.0, width, 10.0),
            })
        });
        runtime.set_layout_provider(provider);
        runtime
            .eval(
                "var el = document.createElement('div'); document.body.appendChild(el);",
                "t",
            )
            .expect("setup");
        let first = runtime
            .eval("el.getBoundingClientRect().width", "t")
            .expect("first query");
        let second = runtime
            .eval("el.getBoundingClientRect().width", "t")
            .expect("second query");
        assert_eq!((first.as_str(), second.as_str()), ("10", "11"));
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "the provider must be re-evaluated on every query, not cached"
        );
    }
}

#[cfg(all(test, feature = "js"))]
mod fetch_failure_tests {
    use super::*;

    #[test]
    fn loader_failures_reject_instead_of_resolving_404() {
        // Drive Megi's real fetch bridge (#1456): an Err from the host FetchProvider
        // must reject the fetch() promise (a TypeError), never resolve as a synthetic
        // successful 404 that scripts could inspect with `response.ok`.
        let mut runtime = megi_js::JsRuntime::new().expect("runtime");
        runtime.set_fetch_provider(Arc::new(|_| {
            Err(megi_js::JsError::Runtime("permission denied".to_owned()))
        }));
        runtime
            .eval(
                "window.__state = ''; fetch('/secret').then(function () { __state = 'resolved'; }) \
                 .catch(function (e) { __state = e.name + ':' + String(e); });",
                "t",
            )
            .expect("fetch");
        runtime.run_microtasks().expect("microtasks");
        let state = runtime.eval("__state", "t").expect("state");
        assert!(
            state.starts_with("TypeError:"),
            "a loader failure must reject the fetch promise, got {state}"
        );
        assert!(
            state.contains("fetch failed"),
            "the rejection must surface the loader error, got {state}"
        );
    }
}

#[cfg(all(test, feature = "js"))]
mod websocket_policy_tests {
    use super::*;

    #[test]
    fn https_document_blocks_insecure_ws_before_dialing() {
        let script_text = "var ws = new WebSocket('ws://127.0.0.1:9/insecure');\
            document.body.setAttribute('data-state', String(ws.readyState));\
            ws.onerror = function (e) { document.body.setAttribute('data-msg', e.message || ''); };";
        let html = format!("<!DOCTYPE html><body><script>{script_text}</script></body>");
        let document = megi_html::parse_document(&html, true);
        let https_url = Url::parse("https://secure.example.test/app.html").expect("url");
        let scripts = vec![LoadedScript {
            element: document.root(),
            text: script_text.to_owned(),
            source_name: "inline-script-0".to_owned(),
            bytes: script_text.len(),
            is_module: false,
            module_url: None,
        }];
        let host = ScriptingHostContext::new(
            None,
            megi_css::MediaContext::with_viewport(320.0, 200.0),
            (320, 200),
        );
        let mut warnings = Vec::new();
        let execution = execute_scripts(
            document,
            &https_url,
            scripts,
            ScriptingLimits::default(),
            &host,
            &mut warnings,
        );
        let body = execution.document.body().expect("body");
        let elem = execution
            .document
            .try_node(body)
            .and_then(|node| node.as_element())
            .expect("element");
        assert_eq!(elem.get_attribute("data-state"), Some("3"));
        assert!(
            elem.get_attribute("data-msg")
                .is_some_and(|msg| msg.contains("Mixed content")),
            "expected mixed content block, got {:?}",
            elem.get_attribute("data-msg")
        );
    }

    #[test]
    fn websocket_url_with_surrounding_whitespace_is_rejected() {
        let script_text = "var ws = new WebSocket('  ws://127.0.0.1:9/chat  ');\
            document.body.setAttribute('data-state', String(ws.readyState));\
            ws.onerror = function (e) { document.body.setAttribute('data-msg', e.message || ''); };";
        let html = format!("<!DOCTYPE html><body><script>{script_text}</script></body>");
        let document = megi_html::parse_document(&html, true);
        let http_url = Url::parse("http://example.test/app.html").expect("url");
        let scripts = vec![LoadedScript {
            element: document.root(),
            text: script_text.to_owned(),
            source_name: "inline-script-0".to_owned(),
            bytes: script_text.len(),
            is_module: false,
            module_url: None,
        }];
        let host = ScriptingHostContext::new(
            None,
            megi_css::MediaContext::with_viewport(320.0, 200.0),
            (320, 200),
        );
        let mut warnings = Vec::new();
        let execution = execute_scripts(
            document,
            &http_url,
            scripts,
            ScriptingLimits::default(),
            &host,
            &mut warnings,
        );
        let body = execution.document.body().expect("body");
        let elem = execution
            .document
            .try_node(body)
            .and_then(|node| node.as_element())
            .expect("element");
        assert_eq!(elem.get_attribute("data-state"), Some("3"));
        assert!(
            elem.get_attribute("data-msg")
                .is_some_and(|msg| msg.contains("whitespace")),
            "expected whitespace rejection, got {:?}",
            elem.get_attribute("data-msg")
        );
    }
}

#[cfg(all(test, feature = "js"))]
mod current_script_tests {
    use super::*;

    #[test]
    fn current_script_exposes_the_running_script_element() {
        // #1887: analytics scripts read `document.currentScript.getAttribute(...)`.
        let script_text = "var s = document.currentScript; \
             document.body.setAttribute('data-current', s ? s.getAttribute('data-id') : 'null');";
        let html =
            format!("<!DOCTYPE html><body><script data-id=\"ok\">{script_text}</script></body>");
        let document = megi_html::parse_document(&html, true);
        let script_element = document.get_elements_by_tag_name("script")[0];
        let scripts = vec![LoadedScript {
            element: script_element,
            text: script_text.to_owned(),
            source_name: "inline-script-0".to_owned(),
            bytes: script_text.len(),
            is_module: false,
            module_url: None,
        }];
        let url = Url::parse("https://example.test/").expect("url");
        let host = ScriptingHostContext::new(
            None,
            megi_css::MediaContext::with_viewport(320.0, 200.0),
            (320, 200),
        );
        let mut warnings = Vec::new();
        let execution = execute_scripts(
            document,
            &url,
            scripts,
            ScriptingLimits::default(),
            &host,
            &mut warnings,
        );
        let body = execution.document.body().expect("body");
        assert_eq!(
            execution
                .document
                .node(body)
                .as_element()
                .unwrap()
                .get_attribute("data-current"),
            Some("ok")
        );
    }
}
