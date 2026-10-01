// Element and document members that pages read without feature-testing.
// Evaluated after the Rust-backed DOM wrappers (dom.rs) exist, because it
// extends their prototypes. A name is only defined when the prototype does
// not already have it, so a real implementation always wins.
//
// What is and is not honest here. Content-model members (attributes,
// previousElementSibling, tabIndex, document.scripts/forms/images/links,
// activeElement, ...) are computed from the real tree. Geometry (offset*,
// client*, scroll*, getBoundingClientRect) is NOT: layout does not feed
// script yet, so a view reports zeros and empty lists, the same answer a
// headless DOM like jsdom gives. Pages that read geometry then take their
// "nothing is measurable" path instead of throwing. Real geometry belongs
// with a layout-to-script bridge. attachShadow and animate are deliberately
// absent: faking them would send pages down paths that need the real thing,
// and their absence is detectable.
(function (g) {
    var Element = g.Element, HTMLElement = g.HTMLElement, Document = g.Document, Node = g.Node;
    if (!Element || !Document) return;

    function has(proto, name) {
        for (var p = proto; p; p = Object.getPrototypeOf(p)) {
            if (Object.prototype.hasOwnProperty.call(p, name)) return true;
        }
        return false;
    }
    function getter(proto, name, fn, setter) {
        if (has(proto, name)) return;
        Object.defineProperty(proto, name, { get: fn, set: setter, configurable: true, enumerable: true });
    }
    function method(proto, name, fn) {
        if (has(proto, name)) return;
        Object.defineProperty(proto, name, { value: fn, writable: true, configurable: true, enumerable: true });
    }
    function noop() {}

    // ---- Element: content model
    getter(Element.prototype, 'namespaceURI', function () { return 'http://www.w3.org/1999/xhtml'; });
    getter(Element.prototype, 'previousElementSibling', function () {
        var n = this.previousSibling;
        while (n && n.nodeType !== 1) n = n.previousSibling;
        return n || null;
    });
    getter(Element.prototype, 'shadowRoot', function () { return null; });
    getter(Element.prototype, 'slot', function () { return this.getAttribute('slot') || ''; },
        function (v) { this.setAttribute('slot', v); });
    getter(Element.prototype, 'assignedSlot', function () { return null; });

    // NamedNodeMap-shaped snapshot of the attributes.
    function Attr(name, value, owner) {
        this.name = name; this.localName = name; this.value = value; this.nodeValue = value;
        this.specified = true; this.namespaceURI = null; this.prefix = null; this.ownerElement = owner;
    }
    function NamedNodeMap() {}
    NamedNodeMap.prototype.item = function (i) { return this[i] || null; };
    NamedNodeMap.prototype.getNamedItem = function (name) {
        for (var i = 0; i < this.length; i++) if (this[i].name === name) return this[i];
        return null;
    };
    NamedNodeMap.prototype.getNamedItemNS = function (ns, name) { return this.getNamedItem(name); };
    if (typeof Symbol === 'function') {
        NamedNodeMap.prototype[Symbol.iterator] = function () {
            var self = this, i = 0;
            return { next: function () { return i < self.length ? { value: self[i++], done: false } : { value: undefined, done: true }; } };
        };
    }
    getter(Element.prototype, 'attributes', function () {
        var m = new NamedNodeMap(), names = this.getAttributeNames();
        for (var i = 0; i < names.length; i++) m[i] = new Attr(names[i], this.getAttribute(names[i]), this);
        Object.defineProperty(m, 'length', { value: names.length });
        return m;
    });

    // tabIndex (HTML §6.6.3): the attribute, else 0 for natively focusable.
    var FOCUSABLE = { a: 1, area: 1, button: 1, input: 1, select: 1, textarea: 1, summary: 1, iframe: 1 };
    getter(HTMLElement.prototype, 'tabIndex', function () {
        var a = this.getAttribute('tabindex');
        if (a !== null && !isNaN(parseInt(a, 10))) return parseInt(a, 10);
        var t = String(this.localName || this.tagName || '').toLowerCase();
        if (t === 'a' || t === 'area') return this.hasAttribute('href') ? 0 : -1;
        return FOCUSABLE[t] ? 0 : -1;
    }, function (v) { this.setAttribute('tabindex', String(v)); });
    getter(HTMLElement.prototype, 'contentEditable', function () {
        var a = this.getAttribute('contenteditable');
        return a === null ? 'inherit' : (a === '' ? 'true' : String(a).toLowerCase());
    }, function (v) { this.setAttribute('contenteditable', v); });
    getter(HTMLElement.prototype, 'isContentEditable', function () {
        var a = this.getAttribute('contenteditable');
        return a !== null && String(a).toLowerCase() !== 'false';
    });
    getter(HTMLElement.prototype, 'accessKey', function () { return this.getAttribute('accesskey') || ''; },
        function (v) { this.setAttribute('accesskey', v); });

    // ---- Element: geometry. Zeros: see the header.
    ['offsetWidth', 'offsetHeight', 'offsetTop', 'offsetLeft',
     'clientWidth', 'clientHeight', 'clientTop', 'clientLeft',
     'scrollWidth', 'scrollHeight'].forEach(function (k) {
        getter(Element.prototype, k, function () { return 0; });
    });
    ['scrollTop', 'scrollLeft'].forEach(function (k) {
        getter(Element.prototype, k, function () { return 0; }, noop);
    });
    getter(Element.prototype, 'offsetParent', function () { return null; });
    function zeroRect() {
        return { x: 0, y: 0, width: 0, height: 0, top: 0, right: 0, bottom: 0, left: 0,
                 toJSON: function () { return { x: 0, y: 0, width: 0, height: 0, top: 0, right: 0, bottom: 0, left: 0 }; } };
    }
    method(Element.prototype, 'getBoundingClientRect', zeroRect);
    method(Element.prototype, 'getClientRects', function () { return []; });
    method(Element.prototype, 'checkVisibility', function () { return true; });
    ['scrollIntoView', 'scroll', 'scrollTo', 'scrollBy', 'setPointerCapture', 'releasePointerCapture'].forEach(function (k) {
        method(Element.prototype, k, noop);
    });
    method(Element.prototype, 'hasPointerCapture', function () { return false; });

    // ---- Document
    function locHref() { return g.location && g.location.href; }
    getter(Document.prototype, 'defaultView', function () { return g; });
    getter(Document.prototype, 'activeElement', function () { return this.body || this.documentElement || null; });
    getter(Document.prototype, 'scrollingElement', function () { return this.documentElement || null; });
    getter(Document.prototype, 'compatMode', function () { return 'CSS1Compat'; });
    getter(Document.prototype, 'visibilityState', function () { return 'visible'; });
    getter(Document.prototype, 'hidden', function () { return false; });
    ['characterSet', 'charset', 'inputEncoding'].forEach(function (k) {
        getter(Document.prototype, k, function () { return 'UTF-8'; });
    });
    getter(Document.prototype, 'contentType', function () { return 'text/html'; });
    getter(Document.prototype, 'designMode', function () { return 'off'; }, noop);
    getter(Document.prototype, 'dir', function () { return ''; }, noop);
    getter(Document.prototype, 'documentURI', function () { return locHref(); });
    getter(Document.prototype, 'baseURI', function () { return locHref(); });
    getter(Document.prototype, 'lastModified', function () { return new Date(0).toUTCString(); });
    // Plain tag queries filtered in script, so the answer does not depend on
    // the engine's selector matcher being injected (it is in a real view).
    function collect(doc, tags, attr) {
        var out = [];
        tags.forEach(function (tag) {
            var all = doc.querySelectorAll(tag);
            for (var i = 0; i < all.length; i++) if (!attr || all[i].hasAttribute(attr)) out.push(all[i]);
        });
        return out;
    }
    [['scripts', ['script'], null], ['forms', ['form'], null], ['images', ['img'], null], ['embeds', ['embed'], null],
     ['links', ['a', 'area'], 'href'], ['anchors', ['a'], 'name']].forEach(function (p) {
        getter(Document.prototype, p[0], function () { return collect(this, p[1], p[2]); });
    });
    getter(Document.prototype, 'styleSheets', function () { return []; });
    method(Document.prototype, 'hasFocus', function () { return true; });
    method(Document.prototype, 'elementFromPoint', function () { return null; });
    method(Document.prototype, 'elementsFromPoint', function () { return []; });
    method(Document.prototype, 'getSelection', function () { return g.getSelection ? g.getSelection() : null; });
    method(Document.prototype, 'execCommand', function () { return false; });
    method(Document.prototype, 'queryCommandSupported', function () { return false; });
    method(Document.prototype, 'queryCommandEnabled', function () { return false; });
    method(Document.prototype, 'createElementNS', function (ns, qualifiedName) {
        var q = String(qualifiedName), i = q.indexOf(':');
        return this.createElement(i < 0 ? q : q.slice(i + 1));
    });
    method(Document.prototype, 'importNode', function (node, deep) { return node.cloneNode(!!deep); });
    method(Document.prototype, 'adoptNode', function (node) { return node; });

    // createEvent (DOM §3.5): the legacy factory, with initEvent.
    if (g.Event && !has(g.Event.prototype, 'initEvent')) {
        Object.defineProperty(g.Event.prototype, 'initEvent', {
            value: function (type, bubbles, cancelable) {
                Object.defineProperty(this, 'type', { value: String(type), configurable: true });
                Object.defineProperty(this, 'bubbles', { value: !!bubbles, configurable: true });
                Object.defineProperty(this, 'cancelable', { value: !!cancelable, configurable: true });
            }, writable: true, configurable: true, enumerable: false
        });
    }
    method(Document.prototype, 'createEvent', function (iface) {
        var k = String(iface).toLowerCase().replace(/s$/, '');
        var ok = { event: 'Event', htmlevent: 'Event', customevent: 'CustomEvent', uievent: 'UIEvent',
                   mouseevent: 'MouseEvent', keyboardevent: 'KeyboardEvent', focusevent: 'FocusEvent' };
        var name = ok[k];
        if (!name) {
            var D = g.DOMException;
            throw typeof D === 'function' ? new D("The provided event type ('" + iface + "') is invalid.", 'NotSupportedError') : new Error('NotSupportedError');
        }
        var C = g[name] || g.Event;
        var e = new C('');
        if (name === 'CustomEvent' && !has(C.prototype, 'initCustomEvent')) {
            e.initCustomEvent = function (type, bubbles, cancelable, detail) {
                this.initEvent(type, bubbles, cancelable);
                Object.defineProperty(this, 'detail', { value: detail, configurable: true });
            };
        }
        return e;
    });
})(globalThis);
