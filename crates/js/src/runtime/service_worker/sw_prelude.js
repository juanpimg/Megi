// ServiceWorkerGlobalScope subset (WHATWG Service Workers §5) and CacheStorage
// (W3C Cache API §5 / §5.4) over the narrow native bridges in prelude.rs.
(function () {
  if (typeof globalThis.self === "undefined") { globalThis.self = globalThis; }
  // W3C Cache API §5.4: quota failures are typed DOMExceptions. Worker realms
  // do not install the XHR prelude that normally defines this constructor.
  if (typeof globalThis.DOMException !== "function") {
    globalThis.DOMException = function DOMException(message, name) {
      var error = new Error(message);
      error.name = name || "DOMException";
      return error;
    };
  }
  var swUrl = __megi_sw_script_url();
  var originMatch = /^([a-z]+:\/\/[^/?#]+)/i.exec(swUrl);
  globalThis.location = { href: swUrl, origin: originMatch ? originMatch[1] : "" };

  // Requests are keyed by their serialized URL, matching the fetch event's
  // `event.request.url` (WHATWG SW §5.2 / Cache API §5.4).
  function cacheKey(request) {
    if (request && typeof request === "object" && typeof request.url === "string") {
      return request.url;
    }
    return new URL(String(request), swUrl).href;
  }

  function headerList(response) {
    var flat = [];
    if (response && response.headers && typeof response.headers.forEach === "function") {
      response.headers.forEach(function (value, name) { flat.push(name); flat.push(value); });
    }
    return flat;
  }

  function responseParts(response, fallbackUrl) {
    return [
      response.status | 0,
      response.statusText || "",
      response.url || fallbackUrl,
      headerList(response),
      typeof response._body === "string" ? response._body : ""
    ];
  }

  function Cache(name) { this.name = name; }
  Cache.prototype.put = function (request, response) {
    var self = this;
    var key = cacheKey(request);
    return Promise.resolve().then(function () {
      if (!response || typeof response.status !== "number") {
        throw new TypeError("Cache.put requires a Response");
      }
      var parts = responseParts(response, key);
      var stored = __megi_cache_put(self.name, key, parts[0], parts[1], parts[2], parts[3], parts[4]);
      if (!stored) { throw new DOMException("QuotaExceededError", "QuotaExceededError"); }
    });
  };
  Cache.prototype.match = function (request) {
    var raw = __megi_cache_match(this.name, cacheKey(request));
    return Promise.resolve(raw ? new Response(raw) : undefined);
  };
  Cache.prototype.add = function (request) {
    var self = this;
    return fetch(request).then(function (response) { return self.put(request, response); });
  };
  Cache.prototype.addAll = function (requests) {
    var self = this;
    return Promise.all(requests.map(function (request) { return self.add(request); }));
  };

  function CacheStorage() {}
  CacheStorage.prototype.open = function (name) {
    return Promise.resolve(new Cache(String(name)));
  };
  CacheStorage.prototype.match = function (request) {
    var raw = __megi_cache_match("", cacheKey(request));
    return Promise.resolve(raw ? new Response(raw) : undefined);
  };
  globalThis.caches = new CacheStorage();

  var listeners = { install: [], activate: [], fetch: [] };
  globalThis.addEventListener = function (type, fn) {
    if (listeners[type] && typeof fn === "function" && listeners[type].length < 64) {
      listeners[type].push(fn);
    }
  };
  globalThis.skipWaiting = function () { return Promise.resolve(); };
  globalThis.clients = { claim: function () { return Promise.resolve(); } };

  // Fired from Rust after registration (install/activate) and on each
  // intercepted same-origin GET (fetch).
  globalThis.__megi_sw_fire = function (type, payload) {
    var list = (listeners[type] || []).slice();
    var event;
    if (type === "fetch") {
      var request = new Request(payload.url, {
        method: payload.method,
        headers: payload.headers
      });
      event = {
        type: "fetch",
        request: request,
        respondWith: function (value) {
          Promise.resolve(value).then(function (response) {
            var parts = responseParts(response, payload.url);
            __megi_sw_capture(parts[0], parts[1], parts[2], parts[3], parts[4]);
          });
        }
      };
    } else {
      event = {
        type: type,
        waitUntil: function (promise) {
          if (promise && typeof promise.then === "function") { promise.catch(function () {}); }
        }
      };
    }
    for (var i = 0; i < list.length; i++) {
      try { list[i](event); } catch (error) {
        if (typeof console !== "undefined") { console.error(error); }
      }
    }
    return list.length;
  };
})();
