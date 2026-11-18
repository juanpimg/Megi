//! `XMLHttpRequest` and `ProgressEvent` built on top of `__megi_fetch_native` (#1532).
//!
//! Covers the standard asynchronous `XMLHttpRequest` lifecycle so bundled libraries and scripts
//! that probe or use XHR before `fetch()` work cleanly against the host `FetchProvider`:
//! - States `UNSENT (0)`, `OPENED (1)`, `HEADERS_RECEIVED (2)`, `LOADING (3)`, `DONE (4)` on both
//!   the constructor and prototype
//! - `open(method, url, async?, user?, password?)` (rejects synchronous requests with
//!   `NotSupportedError` `DOMException`; resolves relative URLs against `location.href`)
//! - `setRequestHeader(name, value)` with RFC 9110 validation and forbidden-header filtering
//! - `overrideMimeType`, `withCredentials`, `timeout`, `upload` EventTarget, `abort()`
//! - `send(body?)` dispatching `loadstart`, `readystatechange` (`HEADERS_RECEIVED`, `LOADING`,
//!   `DONE`), `progress`, `load` / `error` / `abort` / `timeout`, and `loadend` asynchronously via
//!   microtask scheduling so `abort()` right after `send()` works deterministically
//! - `responseType` (`""`, `"text"`, `"json"`, `"arraybuffer"`), `response`, `responseText`,
//!   `status`, `statusText`, `responseURL`, `getResponseHeader`, `getAllResponseHeaders`

use boa_engine::{Context, Source};

use crate::error::JsError;

const XHR_PRELUDE: &str = r####"
(function () {
  var globalObj = typeof window !== "undefined" ? window : globalThis;

  if (typeof globalObj.DOMException !== "function") {
    function DOMException(message, name) {
      var err = new Error(message === undefined ? "" : String(message));
      err.name = name === undefined ? "Error" : String(name);
      err.message = message === undefined ? "" : String(message);
      if (Object.setPrototypeOf) { Object.setPrototypeOf(err, DOMException.prototype); }
      return err;
    }
    DOMException.prototype = Object.create(Error.prototype);
    DOMException.prototype.constructor = DOMException;
    DOMException.prototype.name = "DOMException";
    globalObj.DOMException = DOMException;
  }

  function ProgressEvent(type, eventInitDict) {
    var init = eventInitDict || {};
    var ev;
    if (typeof Event === "function") {
      ev = new Event(String(type), {
        bubbles: Boolean(init.bubbles),
        cancelable: Boolean(init.cancelable)
      });
    } else {
      ev = {
        __megi_event: true,
        type: String(type),
        bubbles: Boolean(init.bubbles),
        cancelable: Boolean(init.cancelable),
        defaultPrevented: false,
        target: null,
        currentTarget: null,
        preventDefault: function () { if (this.cancelable) { this.defaultPrevented = true; } },
        stopPropagation: function () {},
        stopImmediatePropagation: function () {}
      };
    }
    ev.lengthComputable = Boolean(init.lengthComputable);
    ev.loaded = typeof init.loaded === "number" && isFinite(init.loaded) ? init.loaded : 0;
    ev.total = typeof init.total === "number" && isFinite(init.total) ? init.total : 0;
    if (Object.setPrototypeOf) { Object.setPrototypeOf(ev, ProgressEvent.prototype); }
    return ev;
  }
  ProgressEvent.prototype = Object.create(
    (typeof Event === "function" && Event.prototype) ? Event.prototype : Object.prototype
  );
  ProgressEvent.prototype.constructor = ProgressEvent;
  globalObj.ProgressEvent = ProgressEvent;

  var FORBIDDEN_HEADERS = {
    "accept-charset": true,
    "accept-encoding": true,
    "access-control-request-headers": true,
    "access-control-request-method": true,
    "connection": true,
    "content-length": true,
    "cookie": true,
    "cookie2": true,
    "date": true,
    "dnt": true,
    "expect": true,
    "host": true,
    "keep-alive": true,
    "origin": true,
    "proxy-authorization": true,
    "proxy-connection": true,
    "referer": true,
    "te": true,
    "trailer": true,
    "transfer-encoding": true,
    "upgrade": true,
    "user-agent": true,
    "via": true
  };

  function isForbiddenHeader(name) {
    var lower = String(name).toLowerCase();
    return Boolean(FORBIDDEN_HEADERS[lower]) ||
      lower.indexOf("proxy-") === 0 ||
      lower.indexOf("sec-") === 0;
  }

  function validateHeaderName(name) {
    var text = String(name);
    if (!text || !/^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/.test(text)) {
      throw new DOMException("Invalid header name: " + text, "SyntaxError");
    }
    return text;
  }

  function validateHeaderValue(value) {
    var text = String(value).replace(/^[\t ]+|[\t ]+$/g, "");
    if (/[\r\n\0]/.test(text)) {
      throw new DOMException("Invalid header value", "SyntaxError");
    }
    return text;
  }

  function resolveUrl(input) {
    var base = (typeof location !== "undefined" && location && typeof location.href === "string" && location.href)
      ? location.href
      : "about:blank";
    return new URL(String(input), base).href;
  }

  function makeSimpleEvent(type) {
    if (typeof Event === "function") {
      return new Event(type, { bubbles: false, cancelable: false });
    }
    return {
      __megi_event: true,
      type: type,
      bubbles: false,
      cancelable: false,
      defaultPrevented: false,
      target: null,
      currentTarget: null,
      preventDefault: function () {},
      stopPropagation: function () {},
      stopImmediatePropagation: function () {}
    };
  }

  function addTargetListener(target, type, callback, options) {
    if (typeof callback !== "function") { return; }
    var capture = typeof options === "boolean" ? options : Boolean(options && options.capture);
    var once = typeof options === "object" && options !== null ? Boolean(options.once) : false;
    var key = String(type);
    var list = target._listeners[key] || (target._listeners[key] = []);
    for (var i = 0; i < list.length; i++) {
      if (list[i].fn === callback && list[i].capture === capture) {
        return;
      }
    }
    list.push({ fn: callback, capture: capture, once: once });
  }

  function removeTargetListener(target, type, callback, options) {
    var capture = typeof options === "boolean" ? options : Boolean(options && options.capture);
    var key = String(type);
    var list = target._listeners[key];
    if (!list) { return; }
    target._listeners[key] = list.filter(function (entry) {
      return !(entry.fn === callback && entry.capture === capture);
    });
  }

  function dispatchTargetEvent(target, event) {
    if (!event || typeof event.type !== "string") {
      throw new TypeError("Failed to execute 'dispatchEvent': parameter 1 is not of type 'Event'.");
    }
    try { event.target = target; } catch (_e) {}
    try { event.currentTarget = target; } catch (_e) {}
    try { event.eventPhase = 2; } catch (_e) {}
    var stopped = false;
    var origStopImm = event.stopImmediatePropagation;
    event.stopImmediatePropagation = function () {
      stopped = true;
      if (typeof origStopImm === "function") { origStopImm.call(this); }
    };

    var propHandler = target["on" + event.type];
    if (typeof propHandler === "function") {
      propHandler.call(target, event);
    }

    var list = target._listeners[event.type];
    if (list && list.length > 0 && !stopped) {
      var snapshot = list.slice();
      for (var i = 0; i < snapshot.length; i++) {
        if (stopped) { break; }
        var entry = snapshot[i];
        if (entry.once) {
          removeTargetListener(target, event.type, entry.fn, entry.capture);
        }
        entry.fn.call(target, event);
      }
    }
    try { event.currentTarget = null; } catch (_e) {}
    try { event.eventPhase = 0; } catch (_e) {}
    return !event.defaultPrevented;
  }

  function serializeRequestBody(body) {
    if (body === undefined || body === null) {
      return "";
    }
    if (typeof FormData !== "undefined" && body instanceof FormData) {
      return body._toMultipart();
    }
    if (typeof body === "string") {
      return body;
    }
    if (typeof ArrayBuffer !== "undefined") {
      var view = null;
      if (body instanceof ArrayBuffer) {
        view = new Uint8Array(body);
      } else if (ArrayBuffer.isView && ArrayBuffer.isView(body)) {
        view = new Uint8Array(body.buffer, body.byteOffset, body.byteLength);
      }
      if (view) {
        var out = "";
        for (var i = 0; i < view.length; i++) {
          out += String.fromCharCode(view[i]);
        }
        return out;
      }
    }
    return String(body);
  }

  function XMLHttpRequestUpload() {
    this._listeners = {};
    this.onloadstart = null;
    this.onprogress = null;
    this.onabort = null;
    this.onerror = null;
    this.onload = null;
    this.ontimeout = null;
    this.onloadend = null;
  }
  XMLHttpRequestUpload.prototype.addEventListener = function (type, cb, opts) {
    addTargetListener(this, type, cb, opts);
  };
  XMLHttpRequestUpload.prototype.removeEventListener = function (type, cb, opts) {
    removeTargetListener(this, type, cb, opts);
  };
  XMLHttpRequestUpload.prototype.dispatchEvent = function (event) {
    return dispatchTargetEvent(this, event);
  };
  globalObj.XMLHttpRequestUpload = XMLHttpRequestUpload;

  function XMLHttpRequest() {
    this._readyState = 0;
    this._status = 0;
    this._statusText = "";
    this._responseURL = "";
    this._responseText = "";
    this._responseBytes = null;
    this._responseBuffer = null;
    this._cachedJson = undefined;
    this._hasCachedJson = false;
    this._responseType = "";
    this._responseHeaders = [];
    this._requestHeaders = [];
    this._method = null;
    this._url = null;
    this._sendFlag = false;
    this._errorFlag = false;
    this._aborted = false;
    this._requestSeq = 0;
    this._timeout = 0;
    this._withCredentials = false;
    this._overrideMime = null;
    this._listeners = {};
    this._upload = new XMLHttpRequestUpload();

    this.onreadystatechange = null;
    this.onloadstart = null;
    this.onprogress = null;
    this.onabort = null;
    this.onerror = null;
    this.onload = null;
    this.ontimeout = null;
    this.onloadend = null;
  }

  var STATES = [
    ["UNSENT", 0],
    ["OPENED", 1],
    ["HEADERS_RECEIVED", 2],
    ["LOADING", 3],
    ["DONE", 4]
  ];
  for (var s = 0; s < STATES.length; s++) {
    Object.defineProperty(XMLHttpRequest, STATES[s][0], {
      value: STATES[s][1], writable: false, enumerable: true, configurable: false
    });
    Object.defineProperty(XMLHttpRequest.prototype, STATES[s][0], {
      value: STATES[s][1], writable: false, enumerable: true, configurable: false
    });
  }

  Object.defineProperty(XMLHttpRequest.prototype, "readyState", {
    get: function () { return this._readyState; },
    enumerable: true
  });
  Object.defineProperty(XMLHttpRequest.prototype, "status", {
    get: function () { return this._status; },
    enumerable: true
  });
  Object.defineProperty(XMLHttpRequest.prototype, "statusText", {
    get: function () { return this._statusText; },
    enumerable: true
  });
  Object.defineProperty(XMLHttpRequest.prototype, "responseURL", {
    get: function () { return this._responseURL; },
    enumerable: true
  });
  Object.defineProperty(XMLHttpRequest.prototype, "responseXML", {
    get: function () { return null; },
    enumerable: true
  });
  Object.defineProperty(XMLHttpRequest.prototype, "upload", {
    get: function () { return this._upload; },
    enumerable: true
  });
  Object.defineProperty(XMLHttpRequest.prototype, "timeout", {
    get: function () { return this._timeout; },
    set: function (val) {
      var n = Number(val);
      this._timeout = (isFinite(n) && n >= 0) ? Math.floor(n) : 0;
    },
    enumerable: true
  });
  Object.defineProperty(XMLHttpRequest.prototype, "withCredentials", {
    get: function () { return this._withCredentials; },
    set: function (val) { this._withCredentials = Boolean(val); },
    enumerable: true
  });
  Object.defineProperty(XMLHttpRequest.prototype, "responseType", {
    get: function () { return this._responseType; },
    set: function (val) {
      if (this._readyState === 3 || this._readyState === 4) {
        throw new DOMException(
          "Cannot set 'responseType' when readyState is LOADING or DONE",
          "InvalidStateError"
        );
      }
      var next = String(val);
      if (next === "" || next === "text" || next === "json" || next === "arraybuffer") {
        this._responseType = next;
      }
    },
    enumerable: true
  });
  Object.defineProperty(XMLHttpRequest.prototype, "responseText", {
    get: function () {
      if (this._responseType !== "" && this._responseType !== "text") {
        throw new DOMException(
          "responseText is only accessible when responseType is '' or 'text'",
          "InvalidStateError"
        );
      }
      return this._errorFlag || this._readyState < 3 ? "" : this._responseText;
    },
    enumerable: true
  });
  Object.defineProperty(XMLHttpRequest.prototype, "response", {
    get: function () {
      var rt = this._responseType;
      if (rt === "" || rt === "text") {
        return this._errorFlag || this._readyState < 3 ? "" : this._responseText;
      }
      if (this._errorFlag || this._readyState !== 4) {
        return null;
      }
      if (rt === "json") {
        if (!this._hasCachedJson) {
          this._hasCachedJson = true;
          try {
            this._cachedJson = JSON.parse(this._responseText);
          } catch (_e) {
            this._cachedJson = null;
          }
        }
        return this._cachedJson;
      }
      if (rt === "arraybuffer") {
        if (!this._responseBuffer) {
          this._responseBuffer = this._responseBytes
            ? this._responseBytes.slice().buffer
            : new Uint8Array(0).buffer;
        }
        return this._responseBuffer;
      }
      return null;
    },
    enumerable: true
  });

  XMLHttpRequest.prototype.addEventListener = function (type, cb, opts) {
    addTargetListener(this, type, cb, opts);
  };
  XMLHttpRequest.prototype.removeEventListener = function (type, cb, opts) {
    removeTargetListener(this, type, cb, opts);
  };
  XMLHttpRequest.prototype.dispatchEvent = function (event) {
    return dispatchTargetEvent(this, event);
  };

  XMLHttpRequest.prototype._resetResponse = function () {
    this._status = 0;
    this._statusText = "";
    this._responseURL = "";
    this._responseText = "";
    this._responseBytes = null;
    this._responseBuffer = null;
    this._cachedJson = undefined;
    this._hasCachedJson = false;
    this._responseHeaders = [];
    this._errorFlag = false;
  };

  XMLHttpRequest.prototype.open = function (method, url, async) {
    if (arguments.length < 2) {
      throw new TypeError("Failed to execute 'open': 2 arguments required.");
    }
    if (arguments.length >= 3 && async !== undefined && !async) {
      throw new DOMException(
        "Synchronous XMLHttpRequest is not supported",
        "NotSupportedError"
      );
    }
    var m = String(method).trim().toUpperCase();
    var resolved = resolveUrl(url);

    this._requestSeq++;
    this._sendFlag = false;
    this._aborted = false;
    this._method = m;
    this._url = resolved;
    this._requestHeaders = [];
    this._overrideMime = null;
    this._resetResponse();

    var shouldFire = this._readyState !== 1;
    this._readyState = 1;
    if (shouldFire) {
      dispatchTargetEvent(this, makeSimpleEvent("readystatechange"));
    }
  };

  XMLHttpRequest.prototype.setRequestHeader = function (name, value) {
    if (this._readyState !== 1 || this._sendFlag) {
      throw new DOMException(
        "setRequestHeader requires readyState === OPENED and send() not yet called",
        "InvalidStateError"
      );
    }
    var validName = validateHeaderName(name);
    var validValue = validateHeaderValue(value);
    if (isForbiddenHeader(validName)) {
      return;
    }
    var lower = validName.toLowerCase();
    for (var i = 0; i < this._requestHeaders.length; i++) {
      if (this._requestHeaders[i][0].toLowerCase() === lower) {
        this._requestHeaders[i][1] += ", " + validValue;
        return;
      }
    }
    this._requestHeaders.push([validName, validValue]);
  };

  XMLHttpRequest.prototype.overrideMimeType = function (mime) {
    if (this._readyState === 3 || this._readyState === 4) {
      throw new DOMException(
        "Cannot call overrideMimeType when readyState is LOADING or DONE",
        "InvalidStateError"
      );
    }
    this._overrideMime = String(mime);
  };

  XMLHttpRequest.prototype.getResponseHeader = function (name) {
    if (this._readyState < 2 || this._errorFlag) {
      return null;
    }
    var lower = String(name).toLowerCase();
    var values = [];
    for (var i = 0; i < this._responseHeaders.length; i++) {
      if (this._responseHeaders[i][0].toLowerCase() === lower) {
        values.push(this._responseHeaders[i][1]);
      }
    }
    return values.length === 0 ? null : values.join(", ");
  };

  XMLHttpRequest.prototype.getAllResponseHeaders = function () {
    if (this._readyState < 2 || this._errorFlag) {
      return "";
    }
    var combined = {};
    var keys = [];
    for (var i = 0; i < this._responseHeaders.length; i++) {
      var k = String(this._responseHeaders[i][0]).toLowerCase();
      var v = String(this._responseHeaders[i][1]);
      if (!Object.prototype.hasOwnProperty.call(combined, k)) {
        combined[k] = v;
        keys.push(k);
      } else {
        combined[k] += ", " + v;
      }
    }
    keys.sort();
    var out = "";
    for (var j = 0; j < keys.length; j++) {
      out += keys[j] + ": " + combined[keys[j]] + "\r\n";
    }
    return out;
  };

  XMLHttpRequest.prototype._failRequest = function (eventName) {
    this._sendFlag = false;
    this._errorFlag = true;
    this._status = 0;
    this._statusText = "";
    this._responseText = "";
    this._responseBytes = null;
    this._responseBuffer = null;
    this._responseHeaders = [];
    this._readyState = 4;
    dispatchTargetEvent(this, makeSimpleEvent("readystatechange"));
    var pe = new ProgressEvent(eventName, { lengthComputable: false, loaded: 0, total: 0 });
    dispatchTargetEvent(this, pe);
    var le = new ProgressEvent("loadend", { lengthComputable: false, loaded: 0, total: 0 });
    dispatchTargetEvent(this, le);
  };

  XMLHttpRequest.prototype.abort = function () {
    this._requestSeq++;
    this._aborted = true;
    var active = (this._readyState === 1 && this._sendFlag) ||
      this._readyState === 2 ||
      this._readyState === 3;
    if (active) {
      this._failRequest("abort");
    } else {
      this._sendFlag = false;
    }
    this._readyState = 0;
  };

  XMLHttpRequest.prototype.send = function (body) {
    if (this._readyState !== 1 || this._sendFlag) {
      throw new DOMException(
        "Failed to execute 'send': object state must be OPENED and not already sending.",
        "InvalidStateError"
      );
    }
    this._sendFlag = true;
    this._aborted = false;
    this._resetResponse();

    var seq = ++this._requestSeq;
    var method = this._method || "GET";
    var url = this._url;
    if (typeof FormData !== "undefined" && body instanceof FormData) {
      var hasType = false;
      for (var hi = 0; hi < this._requestHeaders.length; hi++) {
        if (String(this._requestHeaders[hi][0]).toLowerCase() === "content-type") { hasType = true; }
      }
      if (!hasType) {
        this._requestHeaders.push(["Content-Type", "multipart/form-data; boundary=" + body._boundary]);
      }
    }
    var headerText = "";
    for (var i = 0; i < this._requestHeaders.length; i++) {
      headerText += this._requestHeaders[i][0] + ":" + this._requestHeaders[i][1] + "\n";
    }
    var payload = (method === "GET" || method === "HEAD") ? "" : serializeRequestBody(body);
    var timeoutLimit = this._timeout;
    var hasUploadPayload = payload.length > 0 && method !== "GET" && method !== "HEAD";

    var self = this;
    dispatchTargetEvent(self, new ProgressEvent("loadstart", { lengthComputable: false, loaded: 0, total: 0 }));
    if (hasUploadPayload) {
      dispatchTargetEvent(self._upload, new ProgressEvent("loadstart", {
        lengthComputable: true, loaded: 0, total: payload.length
      }));
    }

    Promise.resolve().then(function () {
      if (self._aborted || self._requestSeq !== seq) {
        return;
      }
      var startedAt = Date.now();
      var raw;
      try {
        raw = __megi_fetch_native(url, method, headerText, payload);
      } catch (err) {
        if (self._aborted || self._requestSeq !== seq) {
          return;
        }
        var msg = String(err || "").toLowerCase();
        var isTimeout = timeoutLimit > 0 && (msg.indexOf("timed out") >= 0 || msg.indexOf("timeout") >= 0);
        if (hasUploadPayload) {
          var ue = isTimeout ? "timeout" : "error";
          dispatchTargetEvent(self._upload, new ProgressEvent(ue, { lengthComputable: false, loaded: 0, total: 0 }));
          dispatchTargetEvent(self._upload, new ProgressEvent("loadend", { lengthComputable: false, loaded: 0, total: 0 }));
        }
        self._failRequest(isTimeout ? "timeout" : "error");
        return;
      }
      if (self._aborted || self._requestSeq !== seq) {
        return;
      }
      var elapsed = Date.now() - startedAt;
      if (timeoutLimit > 0 && elapsed > timeoutLimit) {
        self._failRequest("timeout");
        return;
      }

      if (hasUploadPayload) {
        dispatchTargetEvent(self._upload, new ProgressEvent("progress", {
          lengthComputable: true, loaded: payload.length, total: payload.length
        }));
        dispatchTargetEvent(self._upload, new ProgressEvent("load", {
          lengthComputable: true, loaded: payload.length, total: payload.length
        }));
        dispatchTargetEvent(self._upload, new ProgressEvent("loadend", {
          lengthComputable: true, loaded: payload.length, total: payload.length
        }));
      }

      self._status = typeof raw.status === "number" ? raw.status : 0;
      self._statusText = raw.statusText || "";
      self._responseURL = raw.url || url || "";
      self._responseHeaders = Array.isArray(raw.headers) ? raw.headers : [];
      self._responseBytes = raw.bodyBytes || new Uint8Array(0);
      self._responseText = method === "HEAD" ? "" : (raw.bodyText || "");
      var totalBytes = method === "HEAD" ? 0 : (self._responseBytes ? self._responseBytes.length : self._responseText.length);

      self._readyState = 2;
      dispatchTargetEvent(self, makeSimpleEvent("readystatechange"));
      if (self._aborted || self._requestSeq !== seq) { return; }

      self._readyState = 3;
      dispatchTargetEvent(self, makeSimpleEvent("readystatechange"));
      if (self._aborted || self._requestSeq !== seq) { return; }
      dispatchTargetEvent(self, new ProgressEvent("progress", {
        lengthComputable: totalBytes > 0, loaded: totalBytes, total: totalBytes
      }));
      if (self._aborted || self._requestSeq !== seq) { return; }

      self._sendFlag = false;
      self._readyState = 4;
      dispatchTargetEvent(self, makeSimpleEvent("readystatechange"));
      dispatchTargetEvent(self, new ProgressEvent("load", {
        lengthComputable: totalBytes > 0, loaded: totalBytes, total: totalBytes
      }));
      dispatchTargetEvent(self, new ProgressEvent("loadend", {
        lengthComputable: totalBytes > 0, loaded: totalBytes, total: totalBytes
      }));
    });
  };

  globalObj.XMLHttpRequest = XMLHttpRequest;
})();
"####;

/// Installs `XMLHttpRequest`, `XMLHttpRequestUpload`, `ProgressEvent`, and a fallback
/// `DOMException` in the JavaScript global object (#1532).
pub(crate) fn install_xhr(context: &mut Context) -> Result<(), JsError> {
    context
        .eval(Source::from_bytes(XHR_PRELUDE))
        .map(|_| ())
        .map_err(|error| JsError::Internal(error.to_string()))
}
