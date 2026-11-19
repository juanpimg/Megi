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
        var raw = __megi_fetch_native(request.url, request.method, headerText, request.body === null ? "" : request.body, request.redirect);
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
