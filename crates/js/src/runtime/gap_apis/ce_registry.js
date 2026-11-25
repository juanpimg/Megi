// WHATWG Custom Elements §4.2 registry wrapper, installed after the native registry.
// WHATWG Custom Elements §4.2: the registry validates names/constructors and exposes
// whenDefined(). The native registry only stores the constructor; the spec-level checks
// need real DOMExceptions, so they live in this prelude wrapper.
  if (typeof customElements === 'object' && customElements) {
    var nativeCeDefine = customElements.define;
    var nativeCeGet = customElements.get;
    var ceNames = Object.create(null);
    var ceConstructors = [];
    var ceWhenDefined = Object.create(null);
    var ceReserved = ['annotation-xml','color-profile','font-face','font-face-src',
      'font-face-uri','font-face-format','font-face-name','missing-glyph'];
    function ceValidName(name) {
      if (typeof name !== 'string') return false;
      if (!/^[a-z][a-z0-9._-]*-[a-z0-9._-]*$/.test(name)) return false;
      return ceReserved.indexOf(name) < 0;
    }
    function ceBad(message, name) { throw new DOMException(message, name); }
    Object.defineProperty(customElements, 'define', {
      configurable: true, writable: true,
      value: function (name, constructor, options) {
        if (!ceValidName(name)) ceBad('The name is not valid', 'SyntaxError');
        if (typeof constructor !== 'function') ceBad('The constructor is not callable', 'TypeError');
        if (ceNames[name]) ceBad('The name has already been used', 'NotSupportedError');
        if (ceConstructors.indexOf(constructor) >= 0) ceBad('The constructor has already been used', 'NotSupportedError');
        var proto = constructor.prototype;
        if (proto === null || (typeof proto !== 'object' && typeof proto !== 'function')) {
          ceBad('The prototype is not an object', 'TypeError');
        }
        var connected = proto.connectedCallback;
        var disconnected = proto.disconnectedCallback;
        var adopted = proto.adoptedCallback;
        var attributeChanged = proto.attributeChangedCallback;
        var observed = proto.observedAttributes;
        if (attributeChanged !== undefined && typeof attributeChanged !== 'function') ceBad('Callback is not callable', 'TypeError');
        if (observed !== undefined && (typeof observed === 'undefined' || typeof observed[Symbol.iterator] !== 'function')) ceBad('observedAttributes is not iterable', 'TypeError');
        nativeCeDefine(name, constructor);
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
          if (local && ceNames[local]) nativeCeDefine(local, nativeCeGet(local));
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
