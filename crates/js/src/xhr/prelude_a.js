
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

  // #1747: HTML §6.1.6.2 registers `on<event>` handler attributes as event
  // listeners at assignment time. Represent each one as a marked entry in
  // `_listeners` so invocation follows registration order and the property
  // handler participates in stopImmediatePropagation like any other listener.
  var XHR_HANDLER_TYPES = [
    'readystatechange', 'loadstart', 'progress', 'abort',
    'error', 'load', 'timeout', 'loadend'
  ];
  function defineHandlerProperty(proto, type) {
    Object.defineProperty(proto, 'on' + type, {
      configurable: true,
      enumerable: true,
      get: function () {
        var entry = this._onHandlerEntries && this._onHandlerEntries[type];
        return entry ? entry.fn : null;
      },
      set: function (fn) {
        if (!this._onHandlerEntries) { this._onHandlerEntries = {}; }
        var prev = this._onHandlerEntries[type];
        if (prev) {
          var old = this._listeners[type];
          if (old) {
            this._listeners[type] = old.filter(function (e) { return e !== prev; });
          }
          delete this._onHandlerEntries[type];
        }
        if (typeof fn === 'function') {
          var entry = { fn: fn, capture: false, once: false, isOnProperty: true };
          var list = this._listeners[type] || (this._listeners[type] = []);
          list.push(entry);
          this._onHandlerEntries[type] = entry;
        }
      }
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

    // #1747: `on<event>` handlers are registered as listeners (see
    // defineHandlerProperty), so they fire here in registration order; do not
    // invoke the property handler ahead of the listener list.
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
        // #2135: encode one byte per char (Latin-1). The native bridge is told
        // via isBinary=true to reinterpret each char code as a byte, avoiding
        // UTF-8 re-encoding that corrupts bytes ≥ 0x80.
        var out = "";
        for (var i = 0; i < view.length; i++) {
          out += String.fromCharCode(view[i]);
        }
        return out;
      }
    }
    return String(body);
  }

  // #2135: report whether `serializeRequestBody` used the byte-preserving
  // Latin-1 path, so the native bridge can decode the payload correctly.
  function isBinaryRequestBody(body) {
    if (body === undefined || body === null || typeof body === "string") { return false; }
    if (typeof FormData !== "undefined" && body instanceof FormData) { return false; }
    return typeof ArrayBuffer !== "undefined"
      && (body instanceof ArrayBuffer || (ArrayBuffer.isView && ArrayBuffer.isView(body)));
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
