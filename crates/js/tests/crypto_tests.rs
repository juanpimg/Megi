//! Offline tests for the `crypto` host object (#2251).
//!
//! `crypto.getRandomValues` must be backed by an OS-grade CSPRNG, not a
//! clock-seeded xorshift, and `crypto.subtle.digest('SHA-256', data)` must be
//! present for the common Web Crypto feature-detection pattern.

#![cfg(feature = "js")]

use megi_js::{Budget, JsRuntime};

fn runtime() -> JsRuntime {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_budget(Budget::SMALL);
    runtime
}

#[test]
fn crypto_has_subtle_digest_2251() {
    let mut runtime = runtime();
    // Feature detection: `crypto.subtle` must exist with a digest method.
    assert_eq!(
        runtime
            .eval(
                "typeof crypto.subtle + '|' + typeof crypto.subtle.digest",
                "test.js"
            )
            .expect("eval"),
        "object|function"
    );
}

#[test]
fn subtle_digest_sha256_matches_known_vectors_2251() {
    let mut runtime = runtime();
    // SHA-256 of "" and "abc" (FIPS 180-4 / Web Crypto canonical vectors),
    // compared as lowercase hex built with a DataView-free hex helper.
    let script = r#"
        function hex(buf) {
            var bytes = new Uint8Array(buf);
            var out = '';
            for (var i = 0; i < bytes.length; i++) {
                out += ('0' + bytes[i].toString(16)).slice(-2);
            }
            return out;
        }
        var pending = 2, result = '';
        crypto.subtle.digest('SHA-256', new Uint8Array(0)).then(function (d) {
            result += hex(d) + ';'; if (--pending === 0) window.__done = result;
        });
        crypto.subtle.digest('SHA-256', new Uint8Array([97,98,99])).then(function (d) {
            result += hex(d) + ';'; if (--pending === 0) window.__done = result;
        });
    "#;
    runtime.eval(script, "test.js").expect("eval");
    runtime.run_microtasks().expect("microtasks");
    // Order of resolution is deterministic in Megi's microtask queue (FIFO).
    assert_eq!(
        runtime.eval("__done", "test.js").expect("eval"),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855;".to_owned()
            + "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad;"
    );
}

#[test]
fn get_random_values_sequences_differ_across_calls_2251() {
    let mut runtime = runtime();
    // Two separate calls must not yield the same byte sequence. A clock-seeded
    // xorshift called twice in the same millisecond returns identical bytes.
    let script = r#"
        function fill() {
            var a = new Uint8Array(16);
            crypto.getRandomValues(a);
            return Array.prototype.join.call(a, ',');
        }
        window.__seq1 = fill();
        window.__seq2 = fill();
        window.__same = __seq1 === __seq2;
        window.__allzero = /^(0,)*0$/.test(__seq1);
    "#;
    runtime.eval(script, "test.js").expect("eval");
    assert_eq!(runtime.eval("__same", "test.js").expect("eval"), "false");
    assert_eq!(runtime.eval("__allzero", "test.js").expect("eval"), "false");
}

#[test]
fn get_random_values_rejects_non_typed_array_2251() {
    let mut runtime = runtime();
    let outcome = runtime
        .eval(
            "try { crypto.getRandomValues({}); 'no-throw'; } catch (e) { 'threw:' + e.name; }",
            "test.js",
        )
        .expect("eval");
    assert!(
        outcome.contains("TypeError"),
        "expected a TypeError, got {outcome}"
    );
}
