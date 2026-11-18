
(function () {
  function define(name, value) { window[name] = value; }
  function urlNormalizePath(path) {
    var segments = String(path).split("/");
    var out = [];
    for (var i = 0; i < segments.length; i++) {
      var segment = segments[i];
      var last = i === segments.length - 1;
      if (segment === ".") { if (last) { out.push(""); } continue; }
      if (segment === "..") {
        if (out.length > 1 || (out.length === 1 && out[0] !== "")) { out.pop(); }
        if (last) { out.push(""); }
        continue;
      }
      out.push(segment);
    }
    var result = out.join("/");
    if (result === "") { return "/"; }
    if (result.charAt(0) !== "/") { result = "/" + result; }
    return result;
  }
  function urlSplitAuthority(authority) {
    var out = { userinfo: "", host: "", hostname: "", port: "" };
    if (!authority) { return out; }
    var hostport = authority;
    var at = authority.lastIndexOf("@");
    if (at >= 0) { out.userinfo = authority.substring(0, at); hostport = authority.substring(at + 1); }
    var hostname = hostport;
    var port = "";
    if (hostport.charAt(0) === "[") {
      var close = hostport.indexOf("]");
      if (close > 0) {
        hostname = hostport.substring(0, close + 1);
        var rest = hostport.substring(close + 1);
        if (rest.charAt(0) === ":") { port = rest.substring(1); }
      }
    } else {
      var colon = hostport.indexOf(":");
      if (colon >= 0) { hostname = hostport.substring(0, colon); port = hostport.substring(colon + 1); }
    }
    out.hostname = hostname;
    out.port = port;
    out.host = port ? hostname + ":" + port : hostname;
    return out;
  }
  function urlBuild(protocol, userinfo, hostname, port, path, search, hash) {
    var hasAuthority = hostname !== "";
    var pathname = path === "" ? (hasAuthority ? "/" : "") : path;
    if (hasAuthority || pathname.charAt(0) === "/") { pathname = urlNormalizePath(pathname); }
    var searchText = search || "";
    if (searchText !== "" && searchText.charAt(0) !== "?") { searchText = "?" + searchText; }
    var hashText = hash || "";
    if (hashText !== "" && hashText.charAt(0) !== "#") { hashText = "#" + hashText; }
    var authority = hasAuthority
      ? "//" + (userinfo ? userinfo + "@" : "") + hostname + (port ? ":" + port : "")
      : "";
    var host = hasAuthority ? (port ? hostname + ":" + port : hostname) : "";
    var credentials = userinfo ? userinfo.split(":") : [];
    var result = {
      href: protocol + authority + pathname + searchText + hashText,
      protocol: protocol,
      username: credentials.length > 0 ? credentials[0] : "",
      password: credentials.length > 1 ? credentials.slice(1).join(":") : "",
      host: host,
      hostname: hostname,
      port: port,
      pathname: pathname,
      search: searchText,
      hash: hashText,
      origin: hasAuthority ? protocol + "//" + host : "null"
    };
    result.searchParams = null;
    return result;
  }
  function urlParse(input, base) {
    var value = String(input);
    var baseString = base === undefined || base === null ? "" : String(base);
    var scheme = /^([A-Za-z][A-Za-z0-9+.-]*:)/.exec(value);
    var protocolRelative = false;
    if (!scheme && value.substring(0, 2) === "//") {
      // Protocol-relative: inherit the base scheme but not its authority.
      var baseScheme = /^([A-Za-z][A-Za-z0-9+.-]*:)/.exec(baseString);
      if (!baseScheme) { throw new TypeError("Invalid URL: " + value); }
      scheme = baseScheme;
      protocolRelative = true;
    }
    if (!scheme) {
      if (!baseString) { throw new TypeError("Invalid URL: " + value); }
      var resolvedBase = urlParse(baseString, null);
      var path = resolvedBase.pathname;
      var search = resolvedBase.search;
      var hash = "";
      var local = value;
      var hashIndex = local.indexOf("#");
      if (hashIndex >= 0) { hash = local.substring(hashIndex); local = local.substring(0, hashIndex); }
      var localQuery = "";
      var queryIndex = local.indexOf("?");
      if (queryIndex >= 0) { localQuery = local.substring(queryIndex); local = local.substring(0, queryIndex); }
      if (local === "") {
        if (queryIndex >= 0) { search = localQuery; } else if (hashIndex < 0) { search = resolvedBase.search; }
      } else if (local.charAt(0) === "/") {
        path = local;
        search = localQuery;
      } else {
        var slash = path.lastIndexOf("/");
        path = (slash >= 0 ? path.substring(0, slash + 1) : "/") + local;
        search = localQuery;
      }
      var baseUserinfo = resolvedBase.username
        ? (resolvedBase.password ? resolvedBase.username + ":" + resolvedBase.password : resolvedBase.username)
        : "";
      return urlBuild(
        resolvedBase.protocol, baseUserinfo, resolvedBase.hostname, resolvedBase.port, path, search, hash
      );
    }
    var protocol = scheme[1];
    var rest = protocolRelative ? value : value.substring(protocol.length);
    var hashIndex2 = rest.indexOf("#");
    var hash2 = "";
    if (hashIndex2 >= 0) { hash2 = rest.substring(hashIndex2); rest = rest.substring(0, hashIndex2); }
    var search2 = "";
    var queryIndex2 = rest.indexOf("?");
    if (queryIndex2 >= 0) { search2 = rest.substring(queryIndex2); rest = rest.substring(0, queryIndex2); }
    var authority2 = "";
    var path2 = rest;
    if (rest.substring(0, 2) === "//") {
      var afterSlashes = rest.substring(2);
      var end = afterSlashes.length;
      for (var i = 0; i < afterSlashes.length; i++) {
        if (afterSlashes.charAt(i) === "/") { end = i; break; }
      }
      authority2 = afterSlashes.substring(0, end);
      path2 = afterSlashes.substring(end);
    }
    var parts = urlSplitAuthority(authority2);
    return urlBuild(protocol, parts.userinfo, parts.hostname, parts.port, path2, search2, hash2);
  }
  function SearchParams(init) {
    this._list = [];
    if (typeof init === "string") {
      init.replace(/^\?/, "").split("&").forEach(function (pair) {
        if (!pair) { return; }
        var index = pair.indexOf("=");
        var key = index < 0 ? pair : pair.substring(0, index);
        var value = index < 0 ? "" : pair.substring(index + 1);
        this._list.push([decodeURIComponent(key.replace(/\+/g, " ")), decodeURIComponent(value.replace(/\+/g, " "))]);
      }, this);
    } else if (init && init._list) {
      this._list = init._list.map(function (pair) { return [pair[0], pair[1]]; });
    }
  }
  SearchParams.prototype.append = function (key, value) { this._list.push([String(key), String(value)]); };
  SearchParams.prototype["delete"] = function (key) { this._list = this._list.filter(function (pair) { return pair[0] !== String(key); }); };
  SearchParams.prototype.get = function (key) { for (var i = 0; i < this._list.length; i++) { if (this._list[i][0] === String(key)) { return this._list[i][1]; } } return null; };
  SearchParams.prototype.getAll = function (key) { return this._list.filter(function (pair) { return pair[0] === String(key); }).map(function (pair) { return pair[1]; }); };
  SearchParams.prototype.has = function (key) { return this.get(key) !== null; };
  SearchParams.prototype.set = function (key, value) { var found = false; var out = []; this._list.forEach(function (pair) { if (pair[0] === String(key)) { if (!found) { out.push([String(key), String(value)]); found = true; } } else { out.push(pair); } }); if (!found) { out.push([String(key), String(value)]); } this._list = out; };
  SearchParams.prototype.toString = function () { return this._list.map(function (pair) { return encodeURIComponent(pair[0]) + "=" + encodeURIComponent(pair[1]); }).join("&"); };
  SearchParams.prototype.sort = function () { this._list.sort(function (a, b) { return a[0] < b[0] ? -1 : (a[0] > b[0] ? 1 : 0); }); };
  SearchParams.prototype.forEach = function (callback, thisArg) { this._list.forEach(function (pair) { callback.call(thisArg, pair[1], pair[0], this); }, this); };
  define("URLSearchParams", SearchParams);
  function URL(input, base) {
    var parsed = urlParse(input, base);
    for (var key in parsed) { if (Object.prototype.hasOwnProperty.call(parsed, key)) { this[key] = parsed[key]; } }
    this.searchParams = new SearchParams(parsed.search);
  }
  URL.prototype.toString = function () { return this.href; };
  URL.prototype.toJSON = function () { return this.href; };
  define("URL", URL);
  var BASE64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  define("btoa", function (input) {
    var value = String(input); var out = "";
    for (var i = 0; i < value.length; i++) {
      if (value.charCodeAt(i) > 255) { throw new DOMException("InvalidCharacterError", "InvalidCharacterError"); }
    }
    for (var i = 0; i < value.length; i += 3) {
      var a = value.charCodeAt(i), b = value.charCodeAt(i + 1), c = value.charCodeAt(i + 2);
      var triple = (a << 16) | ((isNaN(b) ? 0 : b) << 8) | (isNaN(c) ? 0 : c);
      out += BASE64.charAt((triple >> 18) & 63) + BASE64.charAt((triple >> 12) & 63) + (isNaN(b) ? "=" : BASE64.charAt((triple >> 6) & 63)) + (isNaN(c) ? "=" : BASE64.charAt(triple & 63));
    }
    return out;
  });
  define("atob", function (input) {
    var value = String(input).replace(/\s/g, ""); var out = "";
    for (var i = 0; i < value.length; i++) {
      if (BASE64.indexOf(value.charAt(i)) < 0 && value.charAt(i) !== "=") { throw new DOMException("InvalidCharacterError", "InvalidCharacterError"); }
    }
    for (var i = 0; i < value.length; i += 4) {
      var n = (BASE64.indexOf(value.charAt(i)) << 18) | (BASE64.indexOf(value.charAt(i + 1)) << 12) | ((value.charAt(i + 2) === "=" ? 0 : BASE64.indexOf(value.charAt(i + 2))) << 6) | (value.charAt(i + 3) === "=" ? 0 : BASE64.indexOf(value.charAt(i + 3)));
      out += String.fromCharCode((n >> 16) & 255);
      if (value.charAt(i + 2) !== "=") { out += String.fromCharCode((n >> 8) & 255); }
      if (value.charAt(i + 3) !== "=") { out += String.fromCharCode(n & 255); }
    }
    return out;
  });
  define("alert", function (message) {
    if (typeof console !== "undefined" && console.log) { console.log(String(message)); }
  });
  define("confirm", function (message) {
    if (typeof console !== "undefined" && console.log) { console.log("confirm: " + String(message)); }
    return true;
  });
  define("prompt", function (message, defaultValue) {
    if (typeof console !== "undefined" && console.log) { console.log("prompt: " + String(message)); }
    return (defaultValue === undefined ? null : String(defaultValue));
  });
  function clone(value, seen) {
    if (value === null || typeof value !== "object") { return value; }
    if (seen.has(value)) { throw new DOMException("DataCloneError: cyclic object value", "DataCloneError"); }
    if (value instanceof Date) { return new Date(value.getTime()); }
    if (value instanceof Map) { var map = new Map(); seen.add(value); value.forEach(function (v, k) { map.set(clone(k, seen), clone(v, seen)); }); seen.delete(value); return map; }
    if (value instanceof Set) { var set = new Set(); seen.add(value); value.forEach(function (v) { set.add(clone(v, seen)); }); seen.delete(value); return set; }
    if (Array.isArray(value)) { seen.add(value); var arr = value.map(function (item) { return clone(item, seen); }); seen.delete(value); return arr; }
    seen.add(value); var out = {}; for (var key in value) { if (Object.prototype.hasOwnProperty.call(value, key)) { out[key] = clone(value[key], seen); } } seen.delete(value); return out;
  }
  define("structuredClone", function (value) { return clone(value, new Set()); });
  if (typeof Promise !== "undefined") { define("queueMicrotask", function (callback) { Promise.resolve().then(callback); }); }
  else { define("queueMicrotask", function (callback) { setTimeout(callback, 0); }); }
  define("requestIdleCallback", function (callback) { return setTimeout(function () { callback({ didTimeout: false, timeRemaining: function () { return 50; } }); }, 0); });
  define("cancelIdleCallback", function (id) { clearTimeout(id); });
  function makeMouseEvent(type, init) {
    init = init || {};
    this.__megi_event = true;
    this.type = String(type); this.bubbles = !!init.bubbles; this.cancelable = !!init.cancelable;
    this.relatedTarget = init.relatedTarget || null;
    this.clientX = init.clientX || 0; this.clientY = init.clientY || 0;
    this.button = init.button || 0; this.defaultPrevented = false;
  }
  makeMouseEvent.prototype.preventDefault = function () { this.defaultPrevented = true; };
  makeMouseEvent.prototype.stopPropagation = function () {};
  makeMouseEvent.prototype.stopImmediatePropagation = function () {};
  define("MouseEvent", makeMouseEvent);
  define("PointerEvent", function (type, init) { makeMouseEvent.call(this, type, init); this.pointerId = (init && init.pointerId) || 0; });
  if (typeof Event === "function" && Event.prototype) { define("PointerEvent", (function (Base) { function PointerEvent(type, init) { Base.call(this, type, init || {}); this.pointerId = (init && init.pointerId) || 0; } PointerEvent.prototype = Object.create(Base.prototype); PointerEvent.prototype.constructor = PointerEvent; return PointerEvent; })(Event)); }
  // KeyboardEvent / InputEvent / FocusEvent (#1504). Each derives from `Event` when available so
  // `type`, `bubbles`, `cancelable`, `preventDefault` and `stopPropagation` behave uniformly.
  function makeEvent(ctorName, Base, initFields) {
    function EventCtor(type, init) {
      init = init || {};
      if (Base) { Base.call(this, type, init); }
      else { this.__megi_event = true; this.type = String(type); this.bubbles = !!init.bubbles; this.cancelable = !!init.cancelable; this.defaultPrevented = false; }
      for (var key in initFields) { if (Object.prototype.hasOwnProperty.call(initFields, key)) { this[key] = init[key] === undefined ? initFields[key] : init[key]; } }
    }
    if (Base && Base.prototype) { EventCtor.prototype = Object.create(Base.prototype); }
    else { EventCtor.prototype = { preventDefault: function () { this.defaultPrevented = true; }, stopPropagation: function () {}, stopImmediatePropagation: function () {} }; }
    EventCtor.prototype.constructor = EventCtor;
    return EventCtor;
  }
  var EventBase = (typeof Event === "function" && Event.prototype) ? Event : null;
  define("KeyboardEvent", makeEvent("KeyboardEvent", EventBase, { key: '', code: '', ctrlKey: false, shiftKey: false, altKey: false, metaKey: false, repeat: false }));
  define("InputEvent", makeEvent("InputEvent", EventBase, { data: null, inputType: '' }));
  define("FocusEvent", makeEvent("FocusEvent", EventBase, { relatedTarget: null }));
  define("Node", { DOCUMENT_POSITION_DISCONNECTED: 1, DOCUMENT_POSITION_PRECEDING: 2, DOCUMENT_POSITION_FOLLOWING: 4, DOCUMENT_POSITION_CONTAINS: 8, DOCUMENT_POSITION_CONTAINED_BY: 16, DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC: 32 });
  function Headers(init) {
    this._h = [];
    if (init) {
      if (init instanceof Headers) { this._h = init._h.map(function (p) { return [p[0], p[1]]; }); }
      else if (Array.isArray(init)) { for (var i = 0; i < init.length; i++) { this.append(init[i][0], init[i][1]); } }
      else { for (var key in init) { if (Object.prototype.hasOwnProperty.call(init, key)) { this.append(key, init[key]); } } }
    }
  }
  // Header names/values are serialized as `name:value\n` lines for the native side, so a CR,
  // LF or NUL inside either field would forge extra headers (or a colon inside the name would
  // re-split it). Validate at the boundary, like the fetch spec's header-name/header-value
  // grammar (#664).
  function headerName(value) {
    var name = String(value);
    if (/[\r\n\0:]/.test(name)) { throw new TypeError("invalid header name"); }
    return name;
  }
  function headerValue(value) {
    var text = String(value);
    if (/[\r\n\0]/.test(text)) { throw new TypeError("invalid header value"); }
    return text;
  }
  Headers.prototype._find = function (name) { var lower = String(name).toLowerCase(); for (var i = 0; i < this._h.length; i++) { if (this._h[i][0].toLowerCase() === lower) { return i; } } return -1; };
  Headers.prototype.get = function (name) { var lower = String(name).toLowerCase(); var values = []; for (var i = 0; i < this._h.length; i++) { if (this._h[i][0].toLowerCase() === lower) { values.push(this._h[i][1]); } } return values.length === 0 ? null : values.join(", "); };
  Headers.prototype.has = function (name) { return this._find(name) >= 0; };
  Headers.prototype.append = function (name, value) { this._h.push([headerName(name), headerValue(value)]); };
  Headers.prototype.set = function (name, value) { var lower = headerName(name).toLowerCase(); var out = []; var found = false; for (var i = 0; i < this._h.length; i++) { if (this._h[i][0].toLowerCase() === lower) { if (!found) { out.push([this._h[i][0], headerValue(value)]); found = true; } } else { out.push(this._h[i]); } } if (!found) { out.push([headerName(name), headerValue(value)]); } this._h = out; };
  Headers.prototype["delete"] = function (name) { var lower = String(name).toLowerCase(); this._h = this._h.filter(function (pair) { return pair[0].toLowerCase() !== lower; }); };
  Headers.prototype.forEach = function (callback, thisArg) { for (var i = 0; i < this._h.length; i++) { callback.call(thisArg, this._h[i][1], this._h[i][0], this); } };
  Headers.prototype.entries = function () { return this._h.slice(); };
  define("Headers", Headers);
  function AbortSignal() {
    this.aborted = false;
    this.reason = undefined;
    this._listeners = [];
    this.onabort = null;
  }
  AbortSignal.prototype.addEventListener = function (type, callback) {
    if (type === "abort" && typeof callback === "function") { this._listeners.push(callback); }
  };
  AbortSignal.prototype.removeEventListener = function (type, callback) {
    if (type === "abort") { this._listeners = this._listeners.filter(function (item) { return item !== callback; }); }
  };
  AbortSignal.prototype.throwIfAborted = function () { if (this.aborted) { throw abortError(this.reason); } };
  AbortSignal.prototype._abort = function (reason) {
    if (this.aborted) { return; }
    this.aborted = true;
    this.reason = reason;
    var event = { type: "abort", target: this };
    if (typeof this.onabort === "function") { this.onabort(event); }
    for (var i = 0; i < this._listeners.length; i++) { this._listeners[i](event); }
  };
  define("AbortSignal", AbortSignal);
  function AbortController() { this.signal = new AbortSignal(); }
  AbortController.prototype.abort = function (reason) { this.signal._abort(reason); };
  define("AbortController", AbortController);
  function abortError(reason) {
    var error = new Error("The operation was aborted");
    error.name = "AbortError";
    if (reason !== undefined) { error.reason = reason; }
    return error;
  }
  function ReadableStream(chunks) {
    this._chunks = chunks || [];
    this._index = 0;
    this._cancelled = false;
    this.locked = false;
    this._onRead = null;
  }
  ReadableStream.prototype.getReader = function () {
    var stream = this;
    this.locked = true;
    return {
      read: function () {
        if (stream._onRead) { stream._onRead(); }
        if (stream._cancelled || stream._index >= stream._chunks.length) {
          return Promise.resolve({ value: undefined, done: true });
        }
        var chunk = stream._chunks[stream._index++];
        return Promise.resolve({ value: chunk, done: false });
      },
      cancel: function (reason) { stream.cancel(reason); return Promise.resolve(); },
      releaseLock: function () { stream.locked = false; },
      closed: Promise.resolve()
    };
  };
  ReadableStream.prototype.cancel = function (reason) {
    this._cancelled = true;
    this._chunks = [];
    this._index = 0;
    return Promise.resolve(reason);
  };
  define("ReadableStream", ReadableStream);
  function byteChunks(bytes) {
    var chunks = [];
    if (!bytes || bytes.length === 0) { return chunks; }
    var parts = Math.min(3, bytes.length);
    var size = Math.ceil(bytes.length / parts);
    for (var start = 0; start < bytes.length; start += size) {
      chunks.push(new Uint8Array(bytes.slice(start, start + size)));
    }
    return chunks;
  }
  function Response(data) {
    this.status = data.status;
    this.statusText = data.statusText || "";
    this.ok = data.status >= 200 && data.status < 300;
    this.url = data.url || "";
    this.redirected = Boolean(data.redirected);
    this.headers = new Headers(data.headers || []);
    this._body = data.bodyText || "";
    this._bytes = data.bodyBytes || [];
    this._bodyUsed = false;
    var self = this;
    this.body = new ReadableStream(byteChunks(this._bytes));
    this.body._onRead = function () { self._bodyUsed = true; };
  }
  Response.prototype._consume = function () { if (this._bodyUsed) { return null; } this._bodyUsed = true; return this._body; };
  Response.prototype.text = function () { var body = this._consume(); return body === null ? Promise.reject(new TypeError("body already used")) : Promise.resolve(body); };
  Response.prototype.json = function () { var body = this._consume(); if (body === null) { return Promise.reject(new TypeError("body already used")); } try { return Promise.resolve(JSON.parse(body)); } catch (e) { return Promise.reject(e); } };
  Response.prototype.arrayBuffer = function () {
    var body = this._consume();
    if (body === null) { return Promise.reject(new TypeError("body already used")); }
    return Promise.resolve(new Uint8Array(this._bytes.slice()));
  };
  Response.prototype.clone = function () {
    return new Response({ status: this.status, statusText: this.statusText, url: this.url, redirected: this.redirected, headers: this.headers.entries(), bodyText: this._body, bodyBytes: this._bytes.slice() });
  };
  define("Response", Response);
  function normalizeRedirect(value) {
    if (value === undefined) { return "follow"; }
