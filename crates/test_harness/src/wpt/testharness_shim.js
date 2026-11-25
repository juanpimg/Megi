// Minimal testharness.js compatible shim for the offline WPT component runner.
// Records real assertion outcomes; unsupported async constructs are reported as NOTRUN
// instead of being silently turned into PASS.
(function () {
  "use strict";

  var results = [];
  var completionCallbacks = [];
  globalThis.__megi_wpt_results__ = results;
  // Vendored WPT resources are injected as parsed JSON by the Rust runner. This
  // local `fetch` is test-harness plumbing only; it never reimplements an engine API.
  globalThis.__megi_wpt_resources__ = {};
  globalThis.fetch = function (url) {
    var name = String(url).split("/").pop().split("?")[0];
    if (Object.prototype.hasOwnProperty.call(globalThis.__megi_wpt_resources__, name)) {
      var value = globalThis.__megi_wpt_resources__[name];
      return Promise.resolve({
        json: function () { return Promise.resolve(value); },
        text: function () { return Promise.resolve(JSON.stringify(value)); }
      });
    }
    return Promise.reject(new Error("fetch is not available in this host"));
  };

  function text(value) {
    try {
      return String(value);
    } catch (error) {
      return "<unprintable>";
    }
  }

  function record(name, status, message) {
    results.push({ name: String(name), status: status, message: message ? text(message) : "" });
  }

  function assertion(message, description) {
    var error = new Error(description ? description + ": " + message : message);
    error.name = "AssertionError";
    throw error;
  }

  function equivalent(left, right) {
    if (left === right) { return true; }
    // WHATWG Web IDL treats every NaN as equal for assertion purposes.
    return typeof left === "number" && typeof right === "number" &&
      left !== left && right !== right;
  }

  var LEGACY_DOM_EXCEPTION_CODES = {
    INDEX_SIZE_ERR: 1, HIERARCHY_REQUEST_ERR: 3, WRONG_DOCUMENT_ERR: 4,
    INVALID_CHARACTER_ERR: 5, NO_MODIFICATION_ALLOWED_ERR: 7, NOT_FOUND_ERR: 8,
    NOT_SUPPORTED_ERR: 9, INVALID_STATE_ERR: 11, SYNTAX_ERR: 12,
    INVALID_MODIFICATION_ERR: 13, NAMESPACE_ERR: 14, INVALID_ACCESS_ERR: 15,
    TYPE_MISMATCH_ERR: 17, SECURITY_ERR: 18, NETWORK_ERR: 19, ABORT_ERR: 20,
    URL_MISMATCH_ERR: 21, QUOTA_EXCEEDED_ERR: 22, TIMEOUT_ERR: 23,
    INVALID_NODE_TYPE_ERR: 24, DATA_CLONE_ERR: 25
  };

  globalThis.format_value = function (value) {
    try { return JSON.stringify(value); } catch (error) { return text(value); }
  };

  globalThis.test = function (fn, name, props) {
    if (typeof fn !== "function") {
      record(name, "harness_error", "test callback is not a function");
      return;
    }
    try {
      fn();
      record(name, "PASS", "");
    } catch (error) {
      record(name, "FAIL", error && error.message ? error.message : error);
    }
  };

  globalThis.setup = function (fn) {
    if (typeof fn !== "function") { return; }
    try { fn(); } catch (error) { record("<setup>", "harness_error", error); }
  };

  globalThis.generate_tests = function (fn, tests, props) {
    for (var index = 0; index < tests.length; index += 1) {
      var args = tests[index];
      globalThis.test(function () {
        fn.apply(this, args.slice(1));
      }, args[0], Array.isArray(props) ? props[index] : props);
    }
  };

  globalThis.add_completion_callback = function (fn) {
    if (typeof fn === "function") { completionCallbacks.push(fn); }
  };

  globalThis.done = function () {};

  globalThis.assert_true = function (value, description) {
    if (value !== true) { assertion("expected true, got " + text(value), description); }
  };

  globalThis.assert_false = function (value, description) {
    if (value !== false) { assertion("expected false, got " + text(value), description); }
  };

  globalThis.assert_equals = function (actual, expected, description) {
    if (!equivalent(actual, expected)) {
      assertion("expected " + text(expected) + ", got " + text(actual), description);
    }
  };

  globalThis.assert_not_equals = function (actual, expected, description) {
    if (equivalent(actual, expected)) {
      assertion("expected a different value than " + text(expected), description);
    }
  };

  globalThis.assert_array_equals = function (actual, expected, description) {
    var left = Array.prototype.slice.call(actual);
    var right = Array.prototype.slice.call(expected);
    if (left.length !== right.length) {
      assertion("expected length " + right.length + ", got " + left.length, description);
    }
    for (var index = 0; index < right.length; index += 1) {
      if (!equivalent(left[index], right[index])) {
        assertion("at index " + index + ": expected " + text(right[index]) +
          ", got " + text(left[index]), description);
      }
    }
  };

  globalThis.assert_object_equals = function (actual, expected, description) {
    if (format_value(actual) !== format_value(expected)) {
      assertion("expected " + format_value(expected) + ", got " + format_value(actual), description);
    }
  };

  globalThis.assert_in_array = function (actual, expected, description) {
    for (var index = 0; index < expected.length; index += 1) {
      if (equivalent(actual, expected[index])) { return; }
    }
    assertion(text(actual) + " is not in " + text(expected), description);
  };

  globalThis.assert_throws = function (expected, fn, description) {
    var caught = null;
    try { fn(); } catch (error) { caught = error; }
    if (caught === null) { assertion("expected an exception to be thrown", description); }
    var matches = false;
    var name = text(caught && caught.name);
    if (typeof expected === "string") {
      matches = name === expected ||
        (LEGACY_DOM_EXCEPTION_CODES[expected] !== undefined &&
          caught.code === LEGACY_DOM_EXCEPTION_CODES[expected]);
    } else if (typeof expected === "function") {
      matches = caught instanceof expected || name === expected.name;
    } else if (expected && typeof expected === "object") {
      matches = name === text(expected.name) ||
        (expected.code !== undefined && caught.code === expected.code);
    }
    if (!matches) {
      assertion("expected " + text(expected) + ", got " + text(caught), description);
    }
  };

  globalThis.assert_throws_dom = globalThis.assert_throws;
  globalThis.assert_throws_js = globalThis.assert_throws;
  globalThis.assert_throws_exception = globalThis.assert_throws;

  globalThis.assert_unreached = function (description) {
    assertion("assert_unreached was called", description);
  };

  globalThis.async_test = function (fn, name) {
    var pending = {
      name: name,
      step: function (fn) {
        try { fn(pending); } catch (error) { record(name, "FAIL", error); }
      },
      step_func: function (fn) {
        return function () { pending.step(function () { fn.apply(pending, arguments); }); };
      },
      done: function () { record(name, "NOTRUN", "async_test is not driven by this runner"); }
    };
    try { fn(pending); } catch (error) { record(name, "FAIL", error); }
    return pending;
  };

  globalThis.promise_test = function (fn, name) {
    var outcome;
    try { outcome = fn(); } catch (error) { record(name, "FAIL", error); return; }
    if (outcome && typeof outcome.then === "function") {
      outcome.then(function () { record(name, "PASS", ""); },
        function (error) { record(name, "FAIL", error); });
    } else {
      record(name, "PASS", "");
    }
  };

  globalThis.__megi_wpt_flush__ = function () {
    for (var index = 0; index < completionCallbacks.length; index += 1) {
      try { completionCallbacks[index]({ status: 0, message: "" }); }
      catch (error) { record("<completion>", "harness_error", error); }
    }
    return results.length;
  };

  if (typeof globalThis.window === "undefined") { globalThis.window = globalThis; }
})();
