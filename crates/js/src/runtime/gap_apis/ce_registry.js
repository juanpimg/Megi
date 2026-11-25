// WHATWG Custom Elements §4.2 registry wrapper, installed after the native registry.
// The native registry only stores the constructor; the spec-level checks (validation
// order, definition-running flag, callback conversion and whenDefined()) need real
// DOMExceptions and therefore live in this prelude wrapper.
if (typeof customElements === 'object' && customElements) {
  var nativeCeDefine = customElements.define;
  var nativeCeGet = customElements.get;
  var ceNames = Object.create(null);
  var ceConstructors = [];
  var ceWhenDefined = Object.create(null);
  var ceDefinitionRunning = false;
  var ceReserved = ['annotation-xml','color-profile','font-face','font-face-src',
    'font-face-uri','font-face-format','font-face-name','missing-glyph'];
  function ceValidName(name) {
    if (typeof name !== 'string') return false;
    if (!/^[a-z][a-z0-9._-]*-[a-z0-9._-]*$/.test(name)) return false;
    return ceReserved.indexOf(name) < 0;
  }
  function ceBad(message, name) { throw new DOMException(message, name); }
  function ceCallback(value) {
    // WebIDL §3.11: a callback value must be callable (undefined means "absent").
    if (value !== undefined && typeof value !== 'function') ceBad('Callback is not callable', 'TypeError');
    return value;
  }
  function ceObservedAttributes(value) {
    // WebIDL §3.9: sequence<DOMString> conversion rethrows iterator errors.
    if (value === undefined) return [];
    var method = value[Symbol.iterator];
    if (typeof method !== 'function') ceBad('observedAttributes is not iterable', 'TypeError');
    var result = [];
    var iterator = method.call(value);
    var step = iterator.next();
    while (!step.done) {
      result.push(String(step.value));
      step = iterator.next();
    }
    return result;
  }
  Object.defineProperty(customElements, 'define', {
    configurable: true, writable: true,
    value: function (name, constructor, options) {
      // WHATWG Custom Elements §4.2: IsConstructor, valid name, duplicates, definition flag.
      if (typeof constructor !== 'function') ceBad('The constructor is not callable', 'TypeError');
      if (!ceValidName(name)) ceBad('The name is not valid', 'SyntaxError');
      if (ceNames[name]) ceBad('The name has already been used', 'NotSupportedError');
      if (ceConstructors.indexOf(constructor) >= 0) ceBad('The constructor has already been used', 'NotSupportedError');
      if (ceDefinitionRunning) ceBad('An element definition is already running', 'NotSupportedError');

      var callbacks;
      ceDefinitionRunning = true;
      try {
        var proto = constructor.prototype;
        if (proto === null || (typeof proto !== 'object' && typeof proto !== 'function')) {
          ceBad('The prototype is not an object', 'TypeError');
        }
        // WHATWG Custom Elements §4.2: Get() each lifecycle callback in this order and
        // stop at the first exception or failed callback conversion.
        callbacks = [
          ceCallback(proto.connectedCallback),
          ceCallback(proto.disconnectedCallback),
          ceCallback(proto.adoptedCallback),
          ceCallback(proto.attributeChangedCallback)
        ];
        // observedAttributes is read from the constructor, only when
        // attributeChangedCallback is present (WHATWG Custom Elements §4.2).
        if (callbacks[3] !== undefined) {
          ceObservedAttributes(constructor.observedAttributes);
        }
      } finally {
        ceDefinitionRunning = false;
      }
      // WHATWG Custom Elements §4.2: remember the customized built-in local name.
      var extendsName = options && typeof options.extends === 'string'
        ? options.extends.toLowerCase() : '';
      nativeCeDefine(name, constructor, extendsName);
      ceNames[name] = true;
      ceConstructors.push(constructor);
      var pending = ceWhenDefined[name];
      if (pending) { pending.resolve(undefined); delete ceWhenDefined[name]; }
    }
  });
  Object.defineProperty(customElements, 'get', {
    configurable: true, writable: true,
    value: function (name) { return ceValidName(name) ? nativeCeGet(name) : undefined; }
  });
  Object.defineProperty(customElements, 'whenDefined', {
    configurable: true, writable: true,
    value: function (name) {
      if (!ceValidName(name)) return Promise.reject(new DOMException('The name is not valid', 'SyntaxError'));
      if (ceNames[name]) return Promise.resolve(undefined);
      if (!ceWhenDefined[name]) {
        var resolve;
        var promise = new Promise(function (r) { resolve = r; });
        promise.resolve = resolve;
        ceWhenDefined[name] = promise;
      }
      return ceWhenDefined[name];
    }
  });
  Object.defineProperty(customElements, 'upgrade', {
    configurable: true, writable: true,
    value: function (root) {
      var all = root.querySelectorAll ? root.querySelectorAll('*') : [];
      for (var i = 0; i < all.length; i++) {
        var local = all[i].localName;
        if (local && ceNames[local]) nativeCeDefine(local, nativeCeGet(local), '');
      }
    }
  });
  if (typeof CustomElementRegistry !== 'function') {
    function CustomElementRegistry() { ceBad('Illegal constructor', 'TypeError'); }
    CustomElementRegistry.prototype = {
      define: customElements.define, get: customElements.get,
      whenDefined: customElements.whenDefined, upgrade: customElements.upgrade
    };
    globalThis.CustomElementRegistry = CustomElementRegistry;
  }
}
