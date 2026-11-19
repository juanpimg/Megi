#![cfg(feature = "js")]

use std::sync::Arc;

use megi_js::{JsRuntime, SandboxFlags};

#[test]
fn sandbox_attribute_parses_all_tokens() {
    let empty = SandboxFlags::from_attribute("");
    assert!(!empty.allow_scripts);
    assert!(!empty.allow_same_origin);
    assert!(!empty.allow_forms);
    assert!(!empty.allow_popups);
    assert!(!empty.allow_top_navigation);

    let custom = SandboxFlags::from_attribute("allow-scripts ALLOW-SAME-ORIGIN allow-forms");
    assert!(custom.allow_scripts);
    assert!(custom.allow_same_origin);
    assert!(custom.allow_forms);
    assert!(!custom.allow_popups);
    assert!(!custom.allow_top_navigation);
}

#[test]
fn same_origin_iframe_exposes_content_document_and_content_window_identity() {
    let doc = megi_html::parse_document(
        "<!doctype html><html><body>\
         <iframe id='f' srcdoc='<body data-v=\"1\"><p id=\"p\">hi</p></body>'></iframe>\
         </body></html>",
        false,
    );
    let mut rt =
        JsRuntime::with_document(doc, "https://example.com/parent.html".into()).expect("runtime");
    rt.init_connected_frames();
    let res = rt
        .eval(
            "var f = document.getElementById('f');\
             var cw1 = f.contentWindow;\
             var cw2 = f.contentWindow;\
             var cd = f.contentDocument;\
             var p = cd.getElementById('p');\
             p.textContent = 'updated';\
             cd.body.setAttribute('data-edited', 'true');\
             [cw1 === cw2, cd === cw1.document, cd.getElementById('p').textContent].join('|')",
            "test",
        )
        .expect("eval");
    assert_eq!(res, "true|true|updated");

    let snapshots = rt.snapshot_frame_documents();
    assert_eq!(snapshots.len(), 1);
    let child_doc = &snapshots[0].1;
    let body = child_doc.get_elements_by_tag_name("body")[0];
    let body_el = child_doc.node(body).as_element().expect("body");
    assert_eq!(body_el.get_attribute("data-edited"), Some("true"));
}

#[test]
fn sandboxed_iframe_without_allow_same_origin_has_opaque_origin() {
    let doc = megi_html::parse_document(
        "<!doctype html><html><body>\
         <iframe id='box' sandbox='allow-scripts' srcdoc='\
           <script>\
             window.__res = [window.origin, location.origin, document.cookie];\
             document.cookie = \"a=b\";\
             window.__res.push(document.cookie);\
             try { localStorage.setItem(\"k\", \"v\"); window.__res.push(\"ok\"); } catch (e) { window.__res.push(\"blocked\"); }\
             try { var d = window.parent.document; window.__res.push(\"parent-ok\"); } catch (e) { window.__res.push(\"parent-blocked\"); }\
           </script>'></iframe>\
         </body></html>",
        false,
    );
    let mut rt =
        JsRuntime::with_document(doc, "https://example.com/index.html".into()).expect("runtime");
    rt.init_connected_frames();

    let blocked = rt
        .eval(
            "var f = document.getElementById('box');\
             var docNull = f.contentDocument === null;\
             var winBlocked = false;\
             try { var _ = f.contentWindow.document; } catch (e) { winBlocked = String(e).indexOf('SecurityError') >= 0; }\
             docNull + '|' + winBlocked",
            "test",
        )
        .expect("eval");
    assert_eq!(blocked, "true|true");

    // Verify sandbox state when JsRuntime itself runs directly as a sandboxed document.
    let child_doc = megi_html::parse_document(
        "<!doctype html><html><body><form id='fm'></form></body></html>",
        false,
    );
    let mut child_rt = JsRuntime::with_document(child_doc, "https://example.com/child.html".into())
        .expect("child runtime");
    child_rt.set_sandbox_flags(SandboxFlags::from_attribute("allow-scripts"));
    let direct = child_rt
        .eval(
            "var submitted = false;\
             var fm = document.getElementById('fm');\
             fm.addEventListener('submit', function () { submitted = true; });\
             var out = [window.origin, location.origin, document.cookie];\
             document.cookie = 'x=1';\
             out.push(document.cookie);\
             try { localStorage.setItem('a', 'b'); out.push('ok'); } catch (e) { out.push('sec'); }\
             var subOk = fm.submit();\
             out.push(subOk ? 'form-ok' : ('form-blocked:' + submitted));\
             out.join('|')",
            "test-direct",
        )
        .expect("eval direct");
    assert_eq!(direct, "null|null|||sec|form-blocked:false");
}

#[test]
fn cross_origin_iframe_blocks_dom_and_supports_post_message() {
    let doc = megi_html::parse_document(
        "<!doctype html><html><body><iframe id='xo' src='https://other.example/frame.html'></iframe></body></html>",
        false,
    );
    let mut rt =
        JsRuntime::with_document(doc, "https://app.example/index.html".into()).expect("runtime");
    rt.set_frame_loader(Arc::new(|url| {
        if url == "https://other.example/frame.html" {
            Ok((
                url.to_string(),
                "<html><body><script>\
                 window.addEventListener('message', function (ev) {\
                   ev.source.postMessage({ echo: ev.data.msg, seenOrigin: ev.origin }, ev.origin);\
                 });\
                 </script></body></html>"
                    .to_string(),
            ))
        } else {
            Err("not found".to_string())
        }
    }));
    rt.init_connected_frames();
    rt.eval(
        "window.__replies = [];\
         window.addEventListener('message', function (ev) {\
           window.__replies.push(ev.origin + ':' + JSON.stringify(ev.data));\
         });\
         var f = document.getElementById('xo');\
         window.__cdNull = f.contentDocument === null;\
         try { var _ = f.contentWindow.document; window.__threw = false; } catch (e) { window.__threw = String(e).indexOf('SecurityError') >= 0; }\
         f.contentWindow.postMessage({ msg: 'ignored' }, 'https://wrong.example');\
         f.contentWindow.postMessage({ msg: 'hello' }, 'https://other.example');",
        "test",
    )
    .expect("eval");

    rt.run_frame_tasks(16).expect("run frame tasks");
    let check = rt
        .eval(
            "[window.__cdNull, window.__threw, window.__replies.join(';')].join('|')",
            "verify",
        )
        .expect("verify");
    assert_eq!(
        check,
        r#"true|true|https://other.example:{"echo":"hello","seenOrigin":"https://app.example"}"#
    );
}

#[test]
fn same_origin_iframe_supports_bidirectional_post_message_and_srcdoc_reload() {
    let doc = megi_html::parse_document(
        "<!doctype html><html><body>\
         <iframe id='so' srcdoc='<script>\
           window.addEventListener(\"message\", function (e) {\
             window.parent.postMessage({ pong: e.data.ping + 1 }, \"*\");\
           });\
         </script><p id=\"x\">v1</p>'></iframe>\
         </body></html>",
        false,
    );
    let mut rt =
        JsRuntime::with_document(doc, "https://app.example/main.html".into()).expect("runtime");
    rt.init_connected_frames();
    rt.eval(
        "window.__got = '';\
         window.addEventListener('message', function (e) { window.__got = String(e.data.pong); });\
         document.getElementById('so').contentWindow.postMessage({ ping: 41 }, '*');",
        "send",
    )
    .expect("eval");
    rt.run_frame_tasks(16).expect("deliver");
    assert_eq!(rt.eval("window.__got", "read").expect("read"), "42");

    // Changing srcdoc invalidates cached frame state and reloads.
    let updated = rt
        .eval(
            "var f = document.getElementById('so');\
             f.srcdoc = '<p id=\"y\">v2</p>';\
             f.contentDocument.getElementById('y').textContent",
            "reload",
        )
        .expect("reload");
    assert_eq!(updated, "v2");
}
