//! #2251: crypto.getRandomValues / randomUUID must use an OS CSPRNG.

#![cfg(feature = "js")]

use megi_js::JsRuntime;

#[test]
fn get_random_values_are_not_repeated_between_calls() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "var a = new Uint8Array(32), b = new Uint8Array(32); \
             crypto.getRandomValues(a); crypto.getRandomValues(b); \
             [Array.from(a).join(','), Array.from(b).join(',')].join('|');",
            "crypto.js",
        )
        .expect("getRandomValues");
    let parts: Vec<&str> = out.split('|').collect();
    assert_eq!(parts.len(), 2, "unexpected output: {out}");
    assert_ne!(parts[0], parts[1], "two CSPRNG draws must differ");
    assert!(
        parts[0].split(',').any(|b| b.parse::<u8>().is_ok()),
        "bytes"
    );
}

#[test]
fn random_uuid_uses_csprng_and_is_v4() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "[crypto.randomUUID(), crypto.randomUUID()].join('|')",
            "crypto.js",
        )
        .expect("randomUUID");
    let ids: Vec<&str> = out.split('|').collect();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1], "UUIDs must be unique");
    for id in ids {
        let b = id.as_bytes();
        assert_eq!(b[14], b'4', "version nibble must be 4: {id}");
        assert!(matches!(b[19], b'8' | b'9' | b'a' | b'b'), "variant: {id}");
    }
}
