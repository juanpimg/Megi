//! #1538: FormData, multipart/form-data, HTML5 constraint validation, and HTMLFormElement APIs.

#![cfg(feature = "js")]

use std::sync::{Arc, Mutex};

use js::host::{FetchProvider, FetchRequest, FetchResult};
use js::runtime::JsRuntime;

fn runtime_with_fetch() -> (JsRuntime, Arc<Mutex<Vec<FetchRequest>>>) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&calls);
    let mut runtime = JsRuntime::new().expect("runtime");
    let provider: FetchProvider = Box::new(move |request| {
        seen.lock().expect("lock").push(request);
        Ok(FetchResult {
            status: 200,
            status_text: "OK".into(),
            url: "https://example.test/submit".into(),
            headers: vec![("content-type".into(), "text/plain".into())],
            body: b"ok".to_vec(),
        })
    });
    runtime.set_fetch_provider(provider);
    (runtime, calls)
}

#[test]
fn form_data_basic_mutation_and_iteration() {
    let mut rt = JsRuntime::new().expect("runtime");
    let out = rt
        .eval(
            "var fd = new FormData();
             fd.append('a', '1');
             fd.append('b', '2');
             fd.append('a', '3');
             var getA = fd.get('a');
             var allA = fd.getAll('a').join('|');
             var hasB = fd.has('b');
             fd.set('a', '10');
             var afterSet = fd.getAll('a').join('|');
             fd.delete('b');
             var afterDel = fd.has('b');
             var pairs = [];
             for (var entry of fd) { pairs.push(entry[0] + '=' + entry[1]); }
             var keys = Array.from(fd.keys()).join(',');
             var vals = Array.from(fd.values()).join(',');
             var fe = [];
             fd.forEach(function (v, k) { fe.push(k + ':' + v); });
             String([getA, allA, hasB, afterSet, afterDel, pairs.join(';'), keys, vals, fe.join(';')].join(' / '));",
            "form_data_basic.js",
        )
        .expect("eval");
    assert_eq!(out, "1 / 1|3 / true / 10 / false / a=10 / a / 10 / a:10");
}

#[test]
fn form_data_extracts_successful_controls_from_form() {
    let mut rt = JsRuntime::new().expect("runtime");
    rt.eval(
        "document.body.innerHTML =
           '<form id=\"f\">' +
             '<input type=\"text\" name=\"user\" value=\"alice\">' +
             '<input type=\"hidden\" name=\"csrf\" value=\"tok\">' +
             '<input type=\"checkbox\" name=\"agree\" checked>' +
             '<input type=\"checkbox\" name=\"skip\">' +
             '<input type=\"radio\" name=\"role\" value=\"guest\">' +
             '<input type=\"radio\" name=\"role\" value=\"admin\" checked>' +
             '<input type=\"text\" name=\"ignored_disabled\" value=\"nope\" disabled>' +
             '<fieldset disabled>' +
               '<legend><input type=\"text\" name=\"in_legend\" value=\"ok\"></legend>' +
               '<input type=\"text\" name=\"in_fs\" value=\"blocked\">' +
             '</fieldset>' +
             '<textarea name=\"bio\">hello</textarea>' +
             '<select name=\"country\"><option value=\"es\">ES</option><option value=\"fr\" selected>FR</option></select>' +
             '<select name=\"tags\" multiple><option value=\"x\" selected>X</option><option value=\"y\">Y</option><option value=\"z\" selected>Z</option></select>' +
             '<datalist><option value=\"dl\"><input name=\"dl_in\" value=\"no\"></datalist>' +
             '<button id=\"btn\" type=\"submit\" name=\"action\" value=\"save\">Save</button>' +
           '</form>';",
        "setup.js",
    )
    .expect("setup");

    let out = rt
        .eval(
            "var f = document.getElementById('f');
             var btn = document.getElementById('btn');
             var fd1 = new FormData(f);
             var fd2 = new FormData(f, btn);
             var list1 = Array.from(fd1.entries()).map(function (p) { return p[0] + '=' + p[1]; }).join('&');
             var list2 = Array.from(fd2.entries()).map(function (p) { return p[0] + '=' + p[1]; }).join('&');
             var bad = false;
             try { new FormData(document.body); } catch (e) { bad = e instanceof TypeError; }
             String(bad + ' :: ' + list1 + ' :: ' + fd2.get('action'));",
            "extract.js",
        )
        .expect("extract");

    assert_eq!(
        out,
        "true :: user=alice&csrf=tok&agree=on&role=admin&in_legend=ok&bio=hello&country=fr&tags=x&tags=z :: save"
    );
}

#[test]
fn form_data_and_url_search_params_work_with_fetch_and_xhr() {
    let (mut rt, calls) = runtime_with_fetch();
    rt.eval(
        "var f = document.createElement('form');
         f.innerHTML = '<input name=\"who\" value=\"megi\"><input name=\"mode\" value=\"fast\">';
         document.body.appendChild(f);
         var fd = new FormData(f);
         fetch('/upload', { method: 'POST', body: fd });
         var xhr = new XMLHttpRequest();
         xhr.open('POST', '/xhr-upload');
         xhr.send(fd);
         fetch('/params', { method: 'POST', body: new URLSearchParams('k=1&v=two words') });",
        "send_bodies.js",
    )
    .expect("send");
    rt.run_microtasks().expect("microtasks");

    let seen = calls.lock().expect("lock");
    assert_eq!(seen.len(), 3, "3 POST requests dispatched");

    // XHR dispatches asynchronously via microtasks, so request order is not guaranteed
    // (the URLSearchParams fetch can complete before XHR's send). Match by shape instead.
    let multipart: Vec<_> = seen
        .iter()
        .filter(|req| {
            req.headers.iter().any(|(k, v)| {
                k.eq_ignore_ascii_case("content-type")
                    && v.starts_with("multipart/form-data; boundary=----megiFormBoundary")
            })
        })
        .collect();
    assert_eq!(
        multipart.len(),
        2,
        "fetch and XHR must both send FormData as multipart"
    );
    for req in multipart {
        assert_eq!(req.method, "POST");
        let body = String::from_utf8(req.body.clone().expect("body")).expect("utf8");
        assert!(body.contains("Content-Disposition: form-data; name=\"who\"\r\n\r\nmegi\r\n"));
        assert!(body.contains("Content-Disposition: form-data; name=\"mode\"\r\n\r\nfast\r\n"));
        assert!(body.ends_with("----megiFormBoundary7f3e9a2c--\r\n"));
    }

    let urlencoded: Vec<_> = seen
        .iter()
        .filter(|req| {
            req.headers.iter().any(|(k, v)| {
                k.eq_ignore_ascii_case("content-type")
                    && v.starts_with("application/x-www-form-urlencoded")
            })
        })
        .collect();
    assert_eq!(
        urlencoded.len(),
        1,
        "URLSearchParams must set urlencoded content-type"
    );
    assert_eq!(
        String::from_utf8_lossy(urlencoded[0].body.as_deref().unwrap_or_default()),
        "k=1&v=two%20words"
    );
}

#[test]
fn constraint_validation_covers_all_validity_flags_and_controls() {
    let mut rt = JsRuntime::new().expect("runtime");
    let out = rt
        .eval(
            "document.body.innerHTML =
               '<form id=\"f\">' +
                 '<input id=\"cb\" type=\"checkbox\" required>' +
                 '<input id=\"r1\" type=\"radio\" name=\"grp\" value=\"a\" required>' +
                 '<input id=\"r2\" type=\"radio\" name=\"grp\" value=\"b\">' +
                 '<select id=\"sel\" required><option value=\"\">Choose</option><option value=\"one\">One</option></select>' +
                 '<input id=\"pat\" pattern=\"[A-Z]{3}-[0-9]+\" value=\"bad\">' +
                 '<input id=\"mail\" type=\"email\" value=\"not-an-email\">' +
                 '<input id=\"site\" type=\"url\" value=\"no-scheme\">' +
                 '<input id=\"len\" minlength=\"4\" maxlength=\"6\" value=\"ab\">' +
                 '<input id=\"num\" type=\"number\" min=\"10\" max=\"20\" value=\"5\">' +
               '</form>';
             var cb = document.getElementById('cb');
             var r1 = document.getElementById('r1');
             var r2 = document.getElementById('r2');
             var sel = document.getElementById('sel');
             var pat = document.getElementById('pat');
             var mail = document.getElementById('mail');
             var site = document.getElementById('site');
             var len = document.getElementById('len');
             var num = document.getElementById('num');

             var cbMissing = cb.validity.valueMissing;
             cb.checked = true;
             var cbOk = cb.checkValidity();

             var rMissing = r1.validity.valueMissing;
             r2.checked = true;
             var rOk = r1.checkValidity();

             var selMissing = sel.validity.valueMissing;
             sel.value = 'one';
             var selOk = sel.checkValidity();

             var patBad = pat.validity.patternMismatch;
             pat.value = 'ABC-42';
             var patOk = pat.checkValidity();

             var mailBad = mail.validity.typeMismatch;
             mail.value = 'user@example.com';
             var mailOk = mail.checkValidity();

             var urlBad = site.validity.typeMismatch;
             site.value = 'https://example.org/path';
             var urlOk = site.checkValidity();

             var shortBad = len.validity.tooShort;
             len.value = 'abcdefg';
             var longBad = len.validity.tooLong;
             len.value = 'abcde';
             var lenOk = len.checkValidity();

             var underBad = num.validity.rangeUnderflow;
             num.value = '25';
             var overBad = num.validity.rangeOverflow;
             num.value = '15';
             var numOk = num.checkValidity();

             String([cbMissing, cbOk, rMissing, rOk, selMissing, selOk,
                     patBad, patOk, mailBad, mailOk, urlBad, urlOk,
                     shortBad, longBad, lenOk, underBad, overBad, numOk].every(Boolean));",
            "constraints.js",
        )
        .expect("constraints");
    assert_eq!(out, "true");
}

#[test]
fn report_validity_novalidate_form_properties_and_reset() {
    let mut rt = JsRuntime::new().expect("runtime");
    let out = rt
        .eval(
            "globalThis.__events = [];
             globalThis.__submitters = [];
             document.body.innerHTML =
               '<form id=\"f\" action=\"/save\" method=\"POST\" enctype=\"multipart/form-data\">' +
                 '<input id=\"u\" name=\"u\" value=\"init\" required>' +
                 '<input id=\"c\" type=\"checkbox\" name=\"c\" checked>' +
                 '<textarea id=\"t\" name=\"t\">hello</textarea>' +
                 '<button id=\"sub\" type=\"submit\" formnovalidate>Save Draft</button>' +
               '</form>';
             var f = document.getElementById('f');
             var u = document.getElementById('u');
             var c = document.getElementById('c');
             var t = document.getElementById('t');
             var sub = document.getElementById('sub');

             u.oninvalid = function () { __events.push('invalid'); };
             f.onreset = function () { __events.push('reset'); };
             f.addEventListener('submit', function (e) {
               __events.push('submit');
               __submitters.push(e.submitter ? e.submitter.id : 'none');
             });

             var propsOk = f.action === '/save' && f.method === 'post' &&
               f.enctype === 'multipart/form-data' && f.elements.length === 4 && !f.noValidate;

             u.value = '';
             u.checked = false;
             c.checked = false;
             t.value = 'edited';

             var rep1 = f.reportValidity(); // fires invalid on u, returns false
             var reqBypassed = f.requestSubmit(sub); // formnovalidate allows submit

             f.noValidate = true;
             var reqFormNoVal = f.requestSubmit(); // novalidate allows submit

             f.reset();
             var resetOk = u.value === 'init' && c.checked === true && t.value === 'hello';

             String([propsOk, !rep1, reqBypassed, reqFormNoVal, resetOk,
                     __submitters.join(','), __events.join(',')].join('|'));",
            "form_lifecycle.js",
        )
        .expect("lifecycle");
    assert_eq!(
        out,
        "true|true|true|true|true|sub,none|invalid,submit,submit,reset"
    );
}
