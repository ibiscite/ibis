(() => {
  "use strict";
  const locale = (locales) => (Array.isArray(locales) ? locales[0] : locales) ?? "en-US";
  Object.defineProperty(String.prototype, "localeCompare", { configurable: true, writable: true, value: function(other, locales, options) {
    const o = options ?? {};
    return __ibisCompare(String(this), String(other), String(locale(locales)), String(o.sensitivity ?? "variant"), !!o.ignorePunctuation, o.numeric === undefined ? -1 : (o.numeric ? 1 : 0));
  }});
  Object.defineProperty(String.prototype, "toLocaleLowerCase", { configurable: true, writable: true, value: function(locales) {
    return __ibisCase(String(this), String(locale(locales)), false);
  }});
  Object.defineProperty(String.prototype, "toLocaleUpperCase", { configurable: true, writable: true, value: function(locales) {
    return __ibisCase(String(this), String(locale(locales)), true);
  }});
})();
