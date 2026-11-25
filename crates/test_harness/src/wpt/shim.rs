//! The offline `testharness.js` shim embedded into the component runner.

/// JavaScript prelude defining `test()` and the assertion helpers used by WPT fixtures.
pub const TESHARNESS_SHIM: &str = include_str!("testharness_shim.js");

/// Called after every fixture script: fires completion callbacks before results are read.
pub const FLUSH: &str = "globalThis.__megi_wpt_flush__();";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shim_declares_the_core_testharness_api() {
        for symbol in [
            "__megi_wpt_results__",
            "globalThis.test = function",
            "assert_equals",
            "assert_true",
            "assert_throws",
            "promise_test",
            "__megi_wpt_flush__",
            "__megi_wpt_resources__",
            "globalThis.fetch = function",
            "globalThis.on_event = function",
        ] {
            assert!(TESHARNESS_SHIM.contains(symbol), "shim lacks {symbol}");
        }
        assert!(
            !TESHARNESS_SHIM.contains("PASS\"")
                || TESHARNESS_SHIM.contains("record(name, \"PASS\"")
        );
    }
}
