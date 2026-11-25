    var s = String(value);
    if (s !== "follow" && s !== "manual" && s !== "error") {
      throw new TypeError("Invalid Request redirect mode: " + s);
    }
    return s;
  }
  define("__megi_test_pattern", function (pattern, value) {
    var re;
    try { re = new RegExp("^(?:" + pattern + ")$", "u"); }
    catch (_e1) {
      try { re = new RegExp("^(?:" + pattern + ")$"); }
      catch (_e2) { return true; }
    }
    return re.test(value);
  });
  function formControlDisabled(el, form) {
    if (el.disabled || (el.hasAttribute && el.hasAttribute("disabled"))) { return true; }
    var cur = el.parentNode;
    var child = el;
    while (cur && cur !== form) {
      if (cur.tagName && cur.tagName.toUpperCase() === "FIELDSET" && (cur.disabled || (cur.hasAttribute && cur.hasAttribute("disabled")))) {
        var firstLegend = null;
        var fsChildren = cur.children || [];
        for (var i = 0; i < fsChildren.length; i++) {
          if (fsChildren[i].tagName && fsChildren[i].tagName.toUpperCase() === "LEGEND") { firstLegend = fsChildren[i]; break; }
        }
        if (child !== firstLegend) { return true; }
      }
      child = cur;
      cur = cur.parentNode;
    }
    return false;
  }
  function collectFormControls(root, form, list) {
    var kids = root.children || [];
    for (var i = 0; i < kids.length; i++) {
      var el = kids[i];
      if (!el || !el.tagName) { continue; }
      var tag = el.tagName.toUpperCase();
      if (tag === "DATALIST") { continue; }
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || tag === "BUTTON") {
        list.push(el);
      }
      collectFormControls(el, form, list);
    }
  }
  function FormData(form, submitter) {
    this._list = [];
    this._boundary = "----megiFormBoundary7f3e9a2c";
    if (form !== undefined && form !== null) {
      if (!form.tagName || form.tagName.toUpperCase() !== "FORM") {
        throw new TypeError("Failed to construct 'FormData': parameter 1 is not of type 'HTMLFormElement'");
      }
      var controls = [];
      collectFormControls(form, form, controls);
      for (var i = 0; i < controls.length; i++) {
        var el = controls[i];
        if (formControlDisabled(el, form)) { continue; }
        var tag = el.tagName.toUpperCase();
        var name = el.getAttribute ? el.getAttribute("name") : null;
        if (!name) { continue; }
        if (tag === "BUTTON") {
          var btnType = (el.getAttribute("type") || "submit").toLowerCase();
          if (el === submitter && btnType !== "reset" && btnType !== "button") {
            this._list.push([name, String(el.value || "")]);
          }
          continue;
        }
        if (tag === "INPUT") {
          var type = (el.getAttribute("type") || "text").toLowerCase();
          if (type === "submit" || type === "reset" || type === "button" || type === "image") {
            if (el === submitter && type !== "reset" && type !== "button") {
              this._list.push([name, String(el.value || "")]);
            }
            continue;
          }
          if (type === "checkbox" || type === "radio") {
            if (!el.checked) { continue; }
            var rawVal = el.getAttribute("value");
            this._list.push([name, rawVal === null ? "on" : String(rawVal)]);
            continue;
          }
          if (type === "file") { continue; }
          this._list.push([name, String(el.value || "")]);
          continue;
        }
        if (tag === "TEXTAREA") {
          this._list.push([name, String(el.value || "")]);
          continue;
        }
        if (tag === "SELECT") {
          var multiple = el.hasAttribute && el.hasAttribute("multiple");
          var opts = el.options || [];
          if (multiple) {
            for (var j = 0; j < opts.length; j++) {
              if (opts[j].selected && !opts[j].disabled) {
                this._list.push([name, String(opts[j].value || "")]);
              }
            }
          } else {
            var picked = null;
            for (var j = 0; j < opts.length; j++) {
              if (opts[j].selected && !opts[j].disabled) { picked = opts[j]; break; }
            }
            if (!picked) {
              for (var j = 0; j < opts.length; j++) {
                if (!opts[j].disabled) { picked = opts[j]; break; }
              }
            }
            if (picked) {
              this._list.push([name, String(picked.value || "")]);
            }
          }
        }
      }
    }
  }
  function makeFormDataIter(list, mapFn) {
    var idx = 0;
    var iter = {
      next: function () {
        if (idx >= list.length) { return { value: undefined, done: true }; }
        return { value: mapFn(list[idx++]), done: false };
      }
    };
    if (typeof Symbol === "function" && Symbol.iterator) {
      iter[Symbol.iterator] = function () { return this; };
    }
    return iter;
  }
  FormData.prototype.append = function (name, value) {
    this._list.push([String(name), String(value)]);
  };
  FormData.prototype["delete"] = function (name) {
    var key = String(name);
    this._list = this._list.filter(function (pair) { return pair[0] !== key; });
  };
  FormData.prototype.get = function (name) {
    var key = String(name);
    for (var i = 0; i < this._list.length; i++) {
      if (this._list[i][0] === key) { return this._list[i][1]; }
    }
    return null;
  };
  FormData.prototype.getAll = function (name) {
    var key = String(name);
    var out = [];
    for (var i = 0; i < this._list.length; i++) {
      if (this._list[i][0] === key) { out.push(this._list[i][1]); }
    }
    return out;
  };
  FormData.prototype.has = function (name) { return this.get(name) !== null; };
  FormData.prototype.set = function (name, value) {
    var key = String(name);
    var val = String(value);
    var found = false;
    var out = [];
    for (var i = 0; i < this._list.length; i++) {
      if (this._list[i][0] === key) {
        if (!found) { out.push([key, val]); found = true; }
      } else {
        out.push(this._list[i]);
      }
    }
    if (!found) { out.push([key, val]); }
    this._list = out;
  };
  FormData.prototype.keys = function () {
    return makeFormDataIter(this._list, function (pair) { return pair[0]; });
  };
  FormData.prototype.values = function () {
    return makeFormDataIter(this._list, function (pair) { return pair[1]; });
  };
  FormData.prototype.entries = function () {
    return makeFormDataIter(this._list, function (pair) { return [pair[0], pair[1]]; });
  };
  FormData.prototype.forEach = function (callback, thisArg) {
    for (var i = 0; i < this._list.length; i++) {
      callback.call(thisArg, this._list[i][1], this._list[i][0], this);
    }
  };
  if (typeof Symbol === "function" && Symbol.iterator) {
    FormData.prototype[Symbol.iterator] = FormData.prototype.entries;
  }
  FormData.prototype._toMultipart = function () {
    var b = this._boundary;
    var out = "";
    for (var i = 0; i < this._list.length; i++) {
      var escapedName = this._list[i][0].replace(/[\r\n"]/g, function (c) {
        return c === '"' ? "%22" : (c === "\r" ? "%0D" : "%0A");
      });
      out += "--" + b + "\r\n";
      out += 'Content-Disposition: form-data; name="' + escapedName + '"\r\n\r\n';
      out += this._list[i][1] + "\r\n";
    }
    out += "--" + b + "--\r\n";
    return out;
  };
  define("FormData", FormData);
  function Request(input, init) {
    init = init || {};
    if (input && typeof input === "object" && typeof input.url === "string") {
      this.url = input.url;
      this.method = String(init.method || input.method || "GET").toUpperCase();
      this.headers = new Headers(init.headers || input.headers);
      this.body = init.body !== undefined ? init.body : (input.body || null);
      if (this.body !== null && (this.method === "GET" || this.method === "HEAD")) { throw new TypeError("Request with GET/HEAD method cannot have body"); }
      this.signal = init.signal || input.signal || null;
      this.redirect = normalizeRedirect(init.redirect !== undefined ? init.redirect : input.redirect);
    } else {
      this.url = new URL(String(input), location.href).href;
      this.method = String(init.method || "GET").toUpperCase();
      this.headers = new Headers(init.headers);
      this.body = init.body !== undefined && init.body !== null ? init.body : null;
      if (this.body !== null && (this.method === "GET" || this.method === "HEAD")) { throw new TypeError("Request with GET/HEAD method cannot have body"); }
      this.signal = init.signal || null;
      this.redirect = normalizeRedirect(init.redirect);
    }
    // #2135: binary bodies must reach the native bridge byte-for-byte. Encode
    // ArrayBuffer / typed-array bodies as a Latin-1 string (one char per byte)
    // and record the flag so the bridge reinterprets each char as a byte rather
    // than UTF-8-encoding it.
    this._isBinaryBody = false;
    if (this.body !== null && typeof ArrayBuffer !== "undefined"
        && (this.body instanceof ArrayBuffer || (ArrayBuffer.isView && ArrayBuffer.isView(this.body)))) {
      var view = this.body instanceof ArrayBuffer
        ? new Uint8Array(this.body)
        : new Uint8Array(this.body.buffer, this.body.byteOffset, this.body.byteLength);
      var latin = "";
      for (var bi = 0; bi < view.length; bi++) { latin += String.fromCharCode(view[bi]); }
      this.body = latin;
      this._isBinaryBody = true;
    }
    if (this.body !== null && typeof this.body === "object") {
      if (this.body instanceof FormData) {
        if (!this.headers.has("Content-Type")) {
          this.headers.set("Content-Type", "multipart/form-data; boundary=" + this.body._boundary);
        }
        this.body = this.body._toMultipart();
      } else if (this.body instanceof SearchParams) {
        if (!this.headers.has("Content-Type")) {
          this.headers.set("Content-Type", "application/x-www-form-urlencoded;charset=UTF-8");
        }
        this.body = this.body.toString();
      }
    }
  }
  define("Request", Request);
  define("fetch", function (input, init) {
    return new Promise(function (resolve, reject) {
      var request;
      try { request = new Request(input, init); } catch (error) { reject(error); return; }
      var signal = request.signal;
      if (signal && signal.aborted) { reject(abortError(signal.reason)); return; }
      var settled = false;
      var onAbort = function () { if (!settled) { settled = true; reject(abortError(signal.reason)); } };
      if (signal) { signal.addEventListener("abort", onAbort); }
      try {
        var headerText = "";
        request.headers.forEach(function (value, name) { headerText += name + ":" + value + "\n"; });
        var raw = __megi_fetch_native(request.url, request.method, headerText, request.body === null ? "" : request.body, request.redirect, request._isBinaryBody === true);
        Promise.resolve().then(function () {
          if (settled) { return; }
          settled = true;
          if (signal) { signal.removeEventListener("abort", onAbort); }
          var response = new Response(raw);
          if (request.redirect === "follow" && response.url && response.url !== request.url) {
            response.redirected = true;
          }
          if (request.method === "HEAD") { response._bytes = []; response._body = ""; response.body = new ReadableStream([]); }
          resolve(response);
        });
      } catch (error) {
        settled = true;
        if (signal) { signal.removeEventListener("abort", onAbort); }
        reject(new TypeError("fetch failed: " + String(error)));
      }
    });
  });


  // Intl.NumberFormat subset (#1523): locale-aware decimal formatting with grouping and
  // configurable fraction digits. Uses small per-locale separator tables (no ICU dependency);
  // unsupported locales fall back to en-US. Enough for the common currency/count display case.
  (function () {
    var separators = {
      'en': [',', '.'], 'en-us': [',', '.'], 'en-gb': [',', '.'],
      'es': ['.', ','], 'es-es': ['.', ','],
      'de': ['.', ','], 'de-de': ['.', ','],
      'fr': ['\u202f', ','], 'fr-fr': ['\u202f', ','],
      'pt': ['.', ','], 'pt-br': ['.', ',']
    };
    function NumberFormat(locale, options) {
      locale = String(locale || 'en-US');
      options = options || {};
      var pair = separators[locale.toLowerCase()] || separators[locale.split('-')[0].toLowerCase()] || separators['en'];
      this._group = pair[0];
      this._decimal = pair[1];
      this._minFrac = options.minimumFractionDigits === undefined ? 0 : Math.max(0, Math.min(20, options.minimumFractionDigits | 0));
      this._maxFrac = options.maximumFractionDigits === undefined
        ? Math.max(this._minFrac, 3)
        : Math.max(this._minFrac, Math.min(20, options.maximumFractionDigits | 0));
      this._grouping = options.useGrouping !== false;
      this._locale = locale;
    }
    NumberFormat.prototype.resolvedOptions = function () {
      return { locale: this._locale, minimumFractionDigits: this._minFrac, maximumFractionDigits: this._maxFrac, useGrouping: this._grouping };
    };
    NumberFormat.prototype.format = function (value) {
      var n = Number(value);
      if (!isFinite(n)) { return String(n); }
      var negative = n < 0;
      var fixed = Math.abs(n).toFixed(this._maxFrac);
      var parts = fixed.split('.');
      var integer = parts[0];
      var frac = parts[1] || '';
      while (frac.length > this._minFrac && frac.charAt(frac.length - 1) === '0') { frac = frac.slice(0, -1); }
      if (this._grouping) { integer = integer.replace(/\B(?=(\d{3})+(?!\d))/g, this._group); }
      return (negative ? '-' : '') + integer + (frac ? this._decimal + frac : '');
    };
    var intl = (typeof Intl === 'object' && Intl) ? Intl : {};
    intl.NumberFormat = NumberFormat;

    // Intl.DateTimeFormat subset (#1523): locale-ordered numeric dates/times. Kept
    // dependency-free (no ICU) and intentionally small: common `toLocale*` display cases.
    var dateOrder = {
      'en': ['m', 'd', 'y'], 'en-us': ['m', 'd', 'y'], 'en-gb': ['d', 'm', 'y'],
      'es': ['d', 'm', 'y'], 'es-es': ['d', 'm', 'y'],
      'de': ['d', 'm', 'y'], 'de-de': ['d', 'm', 'y'],
      'fr': ['d', 'm', 'y'], 'fr-fr': ['d', 'm', 'y'],
      'pt': ['d', 'm', 'y'], 'pt-br': ['d', 'm', 'y']
    };
    function DateTimeFormat(locale, options) {
      locale = String(locale || 'en-US');
      options = options || {};
      var key = locale.toLowerCase();
      this._order = dateOrder[key] || dateOrder[locale.split('-')[0].toLowerCase()] || dateOrder['en'];
      this._locale = locale;
      this._lang = locale.split('-')[0].toLowerCase();
      var hasDate = options.year || options.month || options.day || options.dateStyle;
      var hasTime = options.hour || options.minute || options.second || options.timeStyle;
      if (!hasDate && !hasTime) { hasDate = true; }
      this._date = !!hasDate;
      this._time = !!hasTime;
    }
    DateTimeFormat.prototype.resolvedOptions = function () {
      return { locale: this._locale, date: this._date, time: this._time };
    };
    DateTimeFormat.prototype.format = function (date) {
      var d = new Date(Number(date));
      if (isNaN(d.getTime())) { return 'Invalid Date'; }
      var parts = {
        y: String(d.getFullYear()),
        m: String(d.getMonth() + 1).padStart(2, '0'),
        d: String(d.getDate()).padStart(2, '0')
      };
      var datePart = this._order.map(function (key) { return parts[key]; }).join('/');
      var timePart = String(d.getHours()).padStart(2, '0') + ':' +
        String(d.getMinutes()).padStart(2, '0') + ':' +
        String(d.getSeconds()).padStart(2, '0');
      if (this._date && this._time) {
        return datePart + (this._lang === 'en' ? ', ' : ' ') + timePart;
      }
      if (this._time) { return timePart; }
      return datePart;
    };
    intl.DateTimeFormat = DateTimeFormat;
    define('Intl', intl);
    if (typeof Number === 'function' && Number.prototype) {
      Number.prototype.toLocaleString = function (locale, options) {
        return new NumberFormat(locale, options).format(this);
      };
      if (typeof Date === 'function' && Date.prototype) {
        Date.prototype.toLocaleDateString = function (locale, options) {
          return new DateTimeFormat(
            locale,
            options && (options.year || options.month || options.day || options.dateStyle)
              ? options : { year: 'numeric', month: '2-digit', day: '2-digit' }
          ).format(this);
        };
        Date.prototype.toLocaleTimeString = function (locale, options) {
          return new DateTimeFormat(
            locale,
            options || { hour: '2-digit', minute: '2-digit', second: '2-digit' }
          ).format(this);
        };
        Date.prototype.toLocaleString = function (locale, options) {
          return new DateTimeFormat(
            locale,
            options || {
              year: 'numeric', month: '2-digit', day: '2-digit',
              hour: '2-digit', minute: '2-digit', second: '2-digit'
            }
          ).format(this);
        };
      }
    }
  })();
})();

// ECMA-262 Annex B.1.3 String.prototype.substr (legacy, used by WHATWG site scripts).
// Boa 0.22 does not expose Annex B methods by default; #1923.
if (typeof String.prototype.substr !== 'function') {
  Object.defineProperty(String.prototype, 'substr', {
    configurable: true,
    writable: true,
    value: function substr(start, length) {
      var s = String(this);
      var size = s.length;
      var intStart = Math.trunc(Number(start));
      if (!isFinite(intStart)) intStart = 0;
      var from = intStart < 0 ? Math.max(size + intStart, 0) : Math.min(intStart, size);
      if (length === undefined) return s.slice(from);
      var intLen = Math.trunc(Number(length));
      if (!isFinite(intLen) || intLen < 0) return '';
      return s.slice(from, from + intLen);
    }
  });
}

// Element.prototype navigation/mutation helpers (#1915/#1876). The low-level
// wrappers expose children/querySelector/insertAdjacentHTML; derive the rest.
if (typeof Element !== 'undefined' && Element.prototype) {
  if (typeof Element.prototype.matches !== 'function') {
    Element.prototype.matches = function (selector) {
      var all = (this.ownerDocument || document).querySelectorAll(selector);
      for (var i = 0; i < all.length; i++) if (all[i] === this) return true;
      return false;
    };
  }
  if (typeof Element.prototype.closest !== 'function') {
    Element.prototype.closest = function (selector) {
      var node = this;
      while (node && node.nodeType === 1) {
        if (node.matches && node.matches(selector)) return node;
        node = node.parentElement;
      }
      return null;
    };
  }
  if (typeof Element.prototype.append !== 'function') {
    Element.prototype.append = function () {
      for (var i = 0; i < arguments.length; i++) {
        var v = arguments[i];
        this.insertAdjacentHTML('beforeend', typeof v === 'string' ? v : (v.outerHTML || ''));
      }
    };
  }
  if (typeof Element.prototype.prepend !== 'function') {
    Element.prototype.prepend = function () {
      for (var i = arguments.length - 1; i >= 0; i--) {
        var v = arguments[i];
        this.insertAdjacentHTML('afterbegin', typeof v === 'string' ? v : (v.outerHTML || ''));
      }
    };
  }
  // #1940: ChildNode.remove() is the modern parentNode.removeChild(this).
  if (typeof Element.prototype.remove !== 'function') {
    Element.prototype.remove = function () {
      var parent = this.parentNode;
      if (parent && parent.removeChild) parent.removeChild(this);
    };
  }
  // #1940: Element.replaceChildren(...nodes) empties the node then appends the
  // arguments; strings become text nodes. No arguments means "empty it".
  if (typeof Element.prototype.replaceChildren !== 'function') {
    Element.prototype.replaceChildren = function () {
      while (this.firstChild) this.removeChild(this.firstChild);
      for (var i = 0; i < arguments.length; i++) {
        var v = arguments[i];
        this.appendChild(typeof v === 'string' ? document.createTextNode(v) : v);
      }
    };
  }
}

// ResizeObserver / IntersectionObserver stubs (#1921/#1892). The deterministic
// pipeline has no async layout notifications yet; feature-detection and the
// observer API surface must still work so analytics/lazy-load scripts run.
if (typeof ResizeObserver !== 'function') {
  globalThis.ResizeObserver = class ResizeObserver {
    constructor(callback) { this._callback = callback; }
    observe() {}
    unobserve() {}
    disconnect() {}
    takeRecords() { return []; }
  };
}
if (typeof IntersectionObserver !== 'function') {
  globalThis.IntersectionObserver = class IntersectionObserver {
    constructor(callback, options) {
      this._callback = callback;
      this.root = options && options.root ? options.root : null;
      this.rootMargin = options && options.rootMargin ? options.rootMargin : '0px';
      this.thresholds = options && options.threshold ? [].concat(options.threshold) : [0];
    }
    observe() {}
    unobserve() {}
    disconnect() {}
    takeRecords() { return []; }
  };
  IntersectionObserver.prototype.root = null;
  IntersectionObserver.prototype.rootMargin = '0px';
  IntersectionObserver.prototype.thresholds = [0];
}

// Standard browser globals used for feature-detection (#1924/#1926/#1925/#1941/
// #1946/#1950/#1951/#1954-#1957/#1960/#1966/#1979). Deterministic no-op stubs
// keep real-site scripts running until the full APIs are implemented in Rust.
(function () {
  function define(name, value) {
    try { Object.defineProperty(globalThis, name, { value: value, writable: true, configurable: true }); } catch (e) {}
  }
  function StubCtor(name, parent) {
    function C() {}
    Object.defineProperty(C, 'name', { value: name });
    if (parent && parent.prototype) C.prototype = Object.create(parent.prototype);
    C.prototype.constructor = C;
    return C;
  }
  if (typeof Blob !== 'function') {
    define('Blob', function Blob(parts, options) {
      var chunks = parts || [];
      this.size = 0; this.type = options && options.type ? options.type : '';
      for (var i = 0; i < chunks.length; i++) this.size += String(chunks[i]).length;
      this.arrayBuffer = function () { return Promise.resolve(new ArrayBuffer(0)); };
      this.text = function () { return Promise.resolve(''); };
      this.slice = function () { return new Blob([], { type: this.type }); };
    });
  }
  if (typeof File !== 'function') define('File', function File() { return new Blob(arguments[0] || [], {}); });
  if (typeof FileReader !== 'function') {
    define('FileReader', function FileReader() {
      this.result = null; this.error = null; this.readyState = 0;
      this.onload = null; this.onerror = null;
      this.readAsText = function () { this.readyState = 2; this.result = ''; if (this.onload) this.onload({ target: this }); };
      this.readAsArrayBuffer = function () { this.readyState = 2; this.result = new ArrayBuffer(0); if (this.onload) this.onload({ target: this }); };
      this.abort = function () {};
    });
  }
  if (typeof URL.createObjectURL !== 'function') {
    URL.createObjectURL = function () { return 'blob:megi/' + Math.random().toString(36).slice(2); };
    URL.revokeObjectURL = function () {};
  }
  if (typeof CSS !== 'function') define('CSS', function CSS() {});
  if (typeof CSS === 'function' && typeof CSS.supports !== 'function') {
    CSS.supports = function (prop, value) { return typeof value === 'undefined' ? !!prop : !!prop && !!value; };
  }
  if (typeof screen === 'undefined') {
    define('screen', { width: 1280, height: 720, availWidth: 1280, availHeight: 720, colorDepth: 24, pixelDepth: 24 });
  }
  if (typeof NodeList !== 'function') define('NodeList', StubCtor('NodeList'));
  if (typeof HTMLCollection !== 'function') define('HTMLCollection', StubCtor('HTMLCollection'));
  if (typeof DOMTokenList !== 'function') define('DOMTokenList', StubCtor('DOMTokenList'));
  if (typeof NamedNodeMap !== 'function') define('NamedNodeMap', StubCtor('NamedNodeMap'));
  if (typeof Node !== 'undefined' && typeof Node.prototype === 'undefined') Node.prototype = {};
  if (typeof DOMParser !== 'function') {
    function DOMParser() {}
    DOMParser.prototype.parseFromString = function () { return {}; };
    define('DOMParser', DOMParser);
  }
  if (typeof CharacterData !== 'function') define('CharacterData', StubCtor('CharacterData', typeof Node !== 'undefined' ? Node : null));
  // WHATWG HTML §4.11.2: `<slot>` is exposed as HTMLSlotElement and `name` reflects the attribute.
  if (typeof HTMLSlotElement !== 'function') {
    function HTMLSlotElement() { throw new DOMException('Illegal constructor', 'TypeError'); }
    HTMLSlotElement.prototype = Object.create(typeof HTMLElement === 'function' ? HTMLElement.prototype : Element.prototype);
    Object.defineProperty(HTMLSlotElement.prototype, 'constructor', { value: HTMLSlotElement, configurable: true });
    Object.defineProperty(HTMLSlotElement.prototype, 'name', {
      get: function () { return this.getAttribute ? (this.getAttribute('name') || '') : ''; },
      set: function (value) { this.setAttribute('name', String(value)); },
      configurable: true
    });
    define('HTMLSlotElement', HTMLSlotElement);
  }
  if (typeof HTMLSlotElement === 'function' && typeof globalThis.__megi_slot_assigned_nodes_native === 'function') {
    var nativeAssignedNodes = globalThis.__megi_slot_assigned_nodes_native;
    HTMLSlotElement.prototype.assignedNodes = function (options) {
      return nativeAssignedNodes.call(this, options || {});
    };
    HTMLSlotElement.prototype.assignedElements = function (options) {
      var nodes = nativeAssignedNodes.call(this, options || {});
      return Array.prototype.filter.call(nodes, function (node) { return node.nodeType === 1; });
    };
  }
  if (typeof DocumentFragment !== 'function') define('DocumentFragment', StubCtor('DocumentFragment'));
  // WHATWG DOM §4.2: importNode is cloneNode into the current document. The offline
  // runner's document is already the owner, so delegation preserves node identity.
  if (typeof document !== 'undefined' && typeof document.importNode !== 'function') {
    document.importNode = function (node, deep) { return node.cloneNode(deep !== false); };
  }
  // WHATWG DOM §4.9: `ShadowRoot` inherits from DocumentFragment and has no public constructor.
  if (typeof ShadowRoot !== 'function') {
    function ShadowRoot() { throw new DOMException('Illegal constructor', 'TypeError'); }
    ShadowRoot.prototype = Object.create(typeof DocumentFragment === 'function' ? DocumentFragment.prototype : null);
    Object.defineProperty(ShadowRoot.prototype, 'constructor', { value: ShadowRoot, configurable: true });
    define('ShadowRoot', ShadowRoot);
  }
  // WHATWG DOM §4.9: an open shadow tree without a focused descendant has no
  // activeElement, and an unstyled tree has an empty StyleSheetList.
  if (typeof ShadowRoot === 'function') {
    Object.defineProperty(ShadowRoot.prototype, 'activeElement', { get: function () { return null; }, configurable: true });
    Object.defineProperty(ShadowRoot.prototype, 'styleSheets', { get: function () { return []; }, configurable: true });
  }
  // WHATWG DOM §4.9: attachShadow validates `mode`, rejects a second attach and
  // disallows non-safelisted elements. The native entry point only creates the root.
  if (typeof Element !== 'undefined' && Element.prototype && typeof globalThis.__megi_attach_shadow_native === 'function') {
    var nativeAttachShadow = globalThis.__megi_attach_shadow_native;
    Element.prototype.attachShadow = function (init) {
      if (!init || typeof init !== 'object' || (init.mode !== 'open' && init.mode !== 'closed')) {
        throw new TypeError('attachShadow: mode must be "open" or "closed"');
      }
      if (Object.prototype.hasOwnProperty.call(this, '__megi_shadow_attached__')) {
        throw new DOMException('Element already hosts a shadow tree', 'InvalidStateError');
      }
      var disallowed = globalThis.ATTACHSHADOW_DISALLOWED_ELEMENTS || [];
      if (typeof this.localName === 'string' && disallowed.indexOf(this.localName.toLowerCase()) >= 0) {
        throw new DOMException('attachShadow is not allowed on this element', 'NotSupportedError');
      }
      Object.defineProperty(this, '__megi_shadow_attached__', { value: true, configurable: false });
      var root = nativeAttachShadow.call(this, init);
      // WHATWG DOM §4.9: `ShadowRoot.host` is the element that owns the tree.
      Object.defineProperty(root, '__megi_shadow_host__', { value: this, configurable: false });
      return root;
    };
  }
  if (typeof ShadowRoot === 'function' && ShadowRoot.prototype) {
    Object.defineProperty(ShadowRoot.prototype, 'host', {
      get: function () { return this.__megi_shadow_host__ || null; },
      configurable: true
    });
  }
  if (typeof Element !== 'undefined' && Element.prototype && typeof globalThis.__megi_shadow_root_native === 'function') {
    var nativeShadowRoot = globalThis.__megi_shadow_root_native;
    Object.defineProperty(Element.prototype, 'shadowRoot', {
      get: function () { return nativeShadowRoot.call(this); },
      configurable: true
    });
  }
  if (typeof Text !== 'function') define('Text', StubCtor('Text'));
  // WHATWG DOM §4.9.2: Slotable.assignedSlot is shared by Element and Text wrappers;
  // the native returns null for closed trees and for nodes without a shadow parent.
  if (typeof globalThis.__megi_assigned_slot_native === 'function') {
    var nativeAssignedSlot = globalThis.__megi_assigned_slot_native;
    var slotableGetter = function () { return nativeAssignedSlot.call(this); };
    Object.defineProperty(Element.prototype, 'assignedSlot', {
      get: slotableGetter, configurable: true
    });
    Object.defineProperty(Text.prototype, 'assignedSlot', {
      get: slotableGetter, configurable: true
    });
  }
  if (typeof Comment !== 'function') define('Comment', StubCtor('Comment'));
  if (typeof Attr !== 'function') define('Attr', StubCtor('Attr'));
  if (typeof Document !== 'function') define('Document', StubCtor('Document'));
  if (typeof SVGElement !== 'function') define('SVGElement', StubCtor('SVGElement', typeof Element !== 'undefined' ? Element : null));
  var htmlNames = ['HTMLDivElement','HTMLAnchorElement','HTMLInputElement','HTMLButtonElement',
    'HTMLFormElement','HTMLTableElement','HTMLCanvasElement','HTMLVideoElement','HTMLAudioElement',
    'HTMLScriptElement','HTMLLinkElement','HTMLSpanElement','HTMLImageElement','HTMLUListElement',
    'HTMLOListElement','HTMLLIElement','HTMLSelectElement','HTMLTextAreaElement','HTMLParagraphElement'];
  htmlNames.forEach(function (n) {
    if (typeof globalThis[n] !== 'function') define(n, StubCtor(n, typeof HTMLElement !== 'undefined' ? HTMLElement : Element));
  });
  if (typeof escape !== 'function') {
    define('escape', function (s) { return encodeURI(String(s)).replace(/!/g, '%21').replace(/'/g, '%27').replace(/\(/g, '%28').replace(/\)/g, '%29'); });
  }
  if (typeof unescape !== 'function') define('unescape', function (s) { return decodeURI(String(s).replace(/%([0-9A-Fa-f]{2})/g, '%$1')); });
  if (typeof close !== 'function') define('close', function () {});
  if (typeof scroll !== 'function') define('scroll', typeof scrollTo === 'function' ? scrollTo : function () {});
  if (typeof getSelection !== 'function') define('getSelection', function () { return { toString: function () { return ''; }, rangeCount: 0 }; });
  if (typeof frames === 'undefined') define('frames', globalThis);
  if (typeof name === 'undefined') define('name', '');
  if (typeof opener === 'undefined') define('opener', null);
  if (typeof length === 'undefined') define('length', 0);
  if (typeof console !== 'undefined') {
    if (typeof console.trace !== 'function') console.trace = function () {};
    if (typeof console.group !== 'function') console.group = function () {};
    if (typeof console.groupEnd !== 'function') console.groupEnd = function () {};
  }
  if (typeof document !== 'undefined') {
    if (typeof document.open !== 'function') document.open = function () { return document; };
    if (typeof document.baseURI === 'undefined') Object.defineProperty(document, 'baseURI', { get: function () { return document.URL || location.href; }, configurable: true });
    if (typeof document.referrer === 'undefined') Object.defineProperty(document, 'referrer', { value: '', configurable: true });
  }
  if (typeof history !== 'undefined' && typeof history.scrollRestoration === 'undefined') {
    Object.defineProperty(history, 'scrollRestoration', { value: 'auto', writable: true, configurable: true });
  }
  if (typeof navigator !== 'undefined' && typeof navigator.cookieEnabled === 'undefined') {
    Object.defineProperty(navigator, 'cookieEnabled', { value: true, configurable: true });
  }
  if (typeof Element !== 'undefined' && typeof Element.prototype.getAnimations !== 'function') {
    Element.prototype.getAnimations = function () { return []; };
  }
})();

// Geometry, document collections, cookie access, Intl, fetch members and event
// cancellation from real-site probes (#1944/#1945/#1947/#1952/#1965/#1970/#1971).
(function () {
  if (typeof Element !== 'undefined' && Element.prototype) {
    ['offsetTop','offsetLeft','scrollTop','scrollLeft'].forEach(function (p) {
      if (typeof Object.getOwnPropertyDescriptor(Element.prototype, p) === 'undefined') {
        Object.defineProperty(Element.prototype, p, { get: function () { return 0; }, set: function () {}, configurable: true });
      }
    });
    ['offsetWidth','offsetHeight','scrollWidth','scrollHeight','clientWidth','clientHeight'].forEach(function (p) {
      if (typeof Object.getOwnPropertyDescriptor(Element.prototype, p) === 'undefined') {
        Object.defineProperty(Element.prototype, p, {
          get: function () { var r = this.getBoundingClientRect ? this.getBoundingClientRect() : { width: 0, height: 0 }; return p.indexOf('Width') >= 0 ? r.width : r.height; },
          configurable: true
        });
      }
    });
    if (typeof Element.prototype.getAnimations !== 'function') Element.prototype.getAnimations = function () { return []; };
  }
  if (typeof document !== 'undefined') {
    ['forms','links','images','scripts'].forEach(function (p) {
      if (typeof Object.getOwnPropertyDescriptor(document, p) === 'undefined') {
        Object.defineProperty(document, p, { get: function () { return this.querySelectorAll(p === 'links' ? 'a[href]' : p); }, configurable: true });
      }
    });
    if (typeof document.documentURI === 'undefined') Object.defineProperty(document, 'documentURI', { get: function () { return this.URL || location.href; }, configurable: true });
    if (typeof document.compatMode === 'undefined') Object.defineProperty(document, 'compatMode', { value: 'CSS1Compat', configurable: true });
    if (typeof document.characterSet === 'undefined') Object.defineProperty(document, 'characterSet', { value: 'utf-8', configurable: true });
    if (typeof Object.getOwnPropertyDescriptor(document, 'cookie') === 'undefined') {
      var cookieJar = {};
      Object.defineProperty(document, 'cookie', {
        get: function () { return Object.keys(cookieJar).map(function (k) { return k + '=' + cookieJar[k]; }).join('; '); },
        set: function (v) { var pair = String(v).split(';')[0].split('='); cookieJar[pair[0]] = pair[1]; },
        configurable: true
      });
    }
  }
  if (typeof Intl !== 'undefined') {
    if (typeof Intl.Collator !== 'function') {
      Intl.Collator = function () { return { compare: function (a, b) { return String(a).localeCompare(String(b)); } }; };
    }
    try {
      if (typeof Intl.DateTimeFormat.prototype.formatToParts !== 'function') {
        Intl.DateTimeFormat.prototype.formatToParts = function (d) { return [{ type: 'literal', value: this.format(d) }]; };
      }
    } catch (e) {}
  }
  if (typeof Response !== 'undefined') {
    if (typeof Response.json !== 'function') Response.json = function (data, init) { return new Response(JSON.stringify(data), init); };
  }
  if (typeof Headers !== 'undefined' && Headers.prototype && typeof Headers.prototype[Symbol.iterator] !== 'function') {
    Headers.prototype[Symbol.iterator] = function () {
      var self = this, names = [];
      try { names = self.keys ? Array.from(self.keys()) : []; } catch (e) {}
      var i = 0;
      return { next: function () { return i < names.length ? { value: [names[i], self.get(names[i++])], done: false } : { done: true }; },
               [Symbol.iterator]: function () { return this; } };
    };
  }
  if (typeof AbortSignal !== 'undefined' && typeof AbortSignal.timeout !== 'function') {
    AbortSignal.timeout = function () { return typeof AbortController !== 'undefined' ? new AbortController().signal : {}; };
  }
  if (typeof URL !== 'undefined' && typeof URL.canParse !== 'function') {
    URL.canParse = function (u, base) { try { new URL(u, base); return true; } catch (e) { return false; } };
  }
  if (typeof URLSearchParams !== 'undefined' && URLSearchParams.prototype && typeof URLSearchParams.prototype[Symbol.iterator] !== 'function') {
    URLSearchParams.prototype[Symbol.iterator] = function () {
      var self = this; var entries = [];
      self.forEach(function (v, k) { entries.push([k, v]); });
      return entries[Symbol.iterator]();
    };
  }
})();

// SVG geometry and HTML table/semantic element APIs (#1973/#1974/#1975).
(function () {
  function define(name, value) {
    try { Object.defineProperty(globalThis, name, { value: value, writable: true, configurable: true }); } catch (e) {}
  }
  if (typeof SVGElement === 'function') {
    if (typeof SVGElement.prototype.getBBox !== 'function') {
      SVGElement.prototype.getBBox = function () {
        var r = this.getBoundingClientRect ? this.getBoundingClientRect() : { x: 0, y: 0, width: 0, height: 0 };
        return { x: r.x, y: r.y, width: r.width, height: r.height };
      };
    }
    if (typeof SVGElement.prototype.getCTM !== 'function') {
      SVGElement.prototype.getCTM = function () { return { a: 1, b: 0, c: 0, d: 1, e: 0, f: 0 }; };
    }
    if (typeof SVGElement.prototype.getScreenCTM !== 'function') {
      SVGElement.prototype.getScreenCTM = function () { return { a: 1, b: 0, c: 0, d: 1, e: 0, f: 0 }; };
    }
  }
  if (typeof SVGSVGElement !== 'function') define('SVGSVGElement', function SVGSVGElement() {});
  if (typeof HTMLTableElement !== 'function') define('HTMLTableElement', function HTMLTableElement() {});
  if (typeof HTMLTableRowElement !== 'function') define('HTMLTableRowElement', function HTMLTableRowElement() {});
  if (typeof HTMLTableCellElement !== 'function') define('HTMLTableCellElement', function HTMLTableCellElement() {});
  if (typeof HTMLTemplateElement !== 'function') define('HTMLTemplateElement', function HTMLTemplateElement() {});
  var tableProto = HTMLTableElement.prototype;
  if (typeof tableProto.rows === 'undefined') {
    Object.defineProperty(tableProto, 'rows', { get: function () { return this.querySelectorAll('tr'); }, configurable: true });
  }
  ['tHead', 'tFoot'].forEach(function (n) {
    if (typeof Object.getOwnPropertyDescriptor(tableProto, n) === 'undefined') {
      Object.defineProperty(tableProto, n, { get: function () { return this.querySelector(n.toLowerCase()); }, configurable: true });
    }
  });
  if (typeof tableProto.tBodies === 'undefined') {
    Object.defineProperty(tableProto, 'tBodies', { get: function () { return this.querySelectorAll('tbody'); }, configurable: true });
  }
  if (typeof tableProto.insertRow !== 'function') {
    tableProto.insertRow = function (index) {
      var row = document.createElement('tr');
      var body = this.querySelector('tbody') || this;
      var rows = body.querySelectorAll('tr');
      var at = typeof index === 'number' && index >= 0 ? index : rows.length;
      if (at < rows.length) body.insertBefore(row, rows[at]); else body.appendChild(row);
      return row;
    };
  }
  if (typeof tableProto.deleteRow !== 'function') {
    tableProto.deleteRow = function (index) {
      var rows = this.querySelectorAll('tr');
      var row = rows[index < 0 ? rows.length + index : index];
      if (row && row.parentNode) row.parentNode.removeChild(row);
    };
  }
  var rowProto = HTMLTableRowElement.prototype;
  if (typeof Object.getOwnPropertyDescriptor(rowProto, 'cells') === 'undefined') {
    Object.defineProperty(rowProto, 'cells', { get: function () { return this.querySelectorAll('td,th'); }, configurable: true });
  }
  if (typeof rowProto.insertCell !== 'function') {
    rowProto.insertCell = function (index) {
      var cell = document.createElement('td');
      var cells = this.querySelectorAll('td,th');
      var at = typeof index === 'number' && index >= 0 ? index : cells.length;
      if (at < cells.length) this.insertBefore(cell, cells[at]); else this.appendChild(cell);
      return cell;
    };
  }
  if (typeof Object.getOwnPropertyDescriptor(HTMLTemplateElement.prototype, 'content') === 'undefined') {
    Object.defineProperty(HTMLTemplateElement.prototype, 'content', {
      get: function () { return this; }, configurable: true
    });
  }
  if (typeof HTMLElement !== 'undefined') {
    if (typeof Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'hidden') === 'undefined') {
      Object.defineProperty(HTMLElement.prototype, 'hidden', {
        get: function () { return this.hasAttribute('hidden'); },
        set: function (v) { if (v) this.setAttribute('hidden', ''); else this.removeAttribute('hidden'); },
        configurable: true
      });
    }
  }
  if (typeof HTMLOListElement !== 'function') define('HTMLOListElement', function HTMLOListElement() {});
  if (typeof Object.getOwnPropertyDescriptor(HTMLOListElement.prototype, 'start') === 'undefined') {
    Object.defineProperty(HTMLOListElement.prototype, 'start', {
      get: function () { return parseInt(this.getAttribute('start') || '1', 10); }, configurable: true
    });
    Object.defineProperty(HTMLOListElement.prototype, 'reversed', {
      get: function () { return this.hasAttribute('reversed'); }, configurable: true
    });
  }
})();

// TextEncoderStream / TextDecoderStream (#1902). Minimal deterministic
// pass-through objects exposing the standard properties and stream accessors.
if (typeof TextEncoderStream !== 'function') {
  globalThis.TextEncoderStream = class TextEncoderStream {
    constructor() {
      this.encoding = 'utf-8';
      var reader = { read: function () { return Promise.resolve({ done: true, value: undefined }); }, releaseLock: function () {} };
      var writer = { write: function () { return Promise.resolve(); }, close: function () { return Promise.resolve(); }, releaseLock: function () {} };
      this.readable = { getReader: function () { return reader; }, pipeTo: function () { return Promise.resolve(); } };
      this.writable = { getWriter: function () { return writer; } };
    }
  };
}
if (typeof TextDecoderStream !== 'function') {
  globalThis.TextDecoderStream = class TextDecoderStream {
    constructor(label, options) {
      this.encoding = String(label || 'utf-8').toLowerCase();
      this.fatal = options ? !!options.fatal : false;
      this.ignoreBOM = options ? !!options.ignoreBOM : false;
      var reader = { read: function () { return Promise.resolve({ done: true, value: '' }); }, releaseLock: function () {} };
      var writer = { write: function () { return Promise.resolve(); }, close: function () { return Promise.resolve(); }, releaseLock: function () {} };
      this.readable = { getReader: function () { return reader; }, pipeTo: function () { return Promise.resolve(); } };
      this.writable = { getWriter: function () { return writer; } };
    }
  };
}
