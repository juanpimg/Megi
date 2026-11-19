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
    // #1854: WHATWG XHR §3.5.1 steps 2-3. The method must be an HTTP token
    // (RFC 9110 §5.6.2) with no surrounding whitespace, and CONNECT/TRACE/TRACK
    // are forbidden. Fail here rather than surfacing a generic error in send().
    var m = String(method);
    if (!/^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/.test(m)) {
      throw new DOMException("Invalid method: " + m, "SyntaxError");
    }
    if (/^(CONNECT|TRACE|TRACK)$/i.test(m)) {
      throw new DOMException("Forbidden method: " + m, "SecurityError");
    }
    m = m.toUpperCase();
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
