// Custom Elements (HTML §4.13): customElements.define / get / getName /
// whenDefined / upgrade, a constructible HTMLElement, and the lifecycle
// reactions connectedCallback, disconnectedCallback and
// attributeChangedCallback.
//
// Pure JS over the DOM bindings: it wraps the tree-mutation and attribute
// methods of the node wrappers and does nothing (one length check) until
// a page defines its first element. Stated limits: autonomous custom
// elements only (`define(name, ctor, { extends })` throws NotSupportedError,
// as in Safari); adoptedCallback never fires (one document); ElementInternals /
// form-associated elements are not provided. `:defined` follows the names
// defined here: each define tells the DOM (`__rkShadowInternals`, 'define'),
// and the style engine reads that set.
// Shadow DOM is not here (see the shadow root work).
(function (g) {
    if (g.customElements) return;
    var document = g.document;
    if (!document || !g.HTMLElement) return;

    var OldHTMLElement = g.HTMLElement;
    var names = [];                 // defined names, in definition order
    var byName = {};                // name -> definition
    var byCtor = typeof Map === 'function' ? new Map() : null;
    var pending = {};               // name -> [resolve] for whenDefined
    var state = typeof WeakMap === 'function' ? new WeakMap() : new Map();
    var stack = [];                 // upgrade/construct in progress
    var scopes = [];                // [CEReactions] element queues, innermost last
    var backup = [];                // reactions enqueued with the stack empty
    var backupScheduled = false;
    var processingBackup = false;
    var reactionQueues = typeof WeakMap === 'function' ? new WeakMap() : new Map();
    var rawCreateElement = null;    // the DOM's own createElement, below
    var selector = '';              // joined defined names, for subtree scans
    var shadowHost = g.__rkShadowInternals || null;  // tells the DOM a name is defined

    function domError(message, name) {
        return typeof g.DOMException === 'function' ? new g.DOMException(message, name) : (function () { var e = new Error(message); e.name = name; return e; })();
    }
    function report(e) {
        try { __rk.reportError(e); } catch (_) {}
    }
    function call(fn, thisArg, args) {
        try { fn.apply(thisArg, args); } catch (e) { report(e); }
    }

    // PotentialCustomElementName: a lowercase letter, a hyphen somewhere
    // after it, no uppercase, and not one of the reserved names.
    var RESERVED = ['annotation-xml', 'color-profile', 'font-face', 'font-face-src', 'font-face-uri',
                    'font-face-format', 'font-face-name', 'missing-glyph'];
    function validName(name) {
        return /^[a-z][-._0-9a-z·À-ÖØ-öø-ͽͿ-῿‌‍‿⁀⁰-↏Ⰰ-⿯、-퟿豈-﷏ﷰ-�]*-[-._0-9a-z·À-ÖØ-öø-ͽͿ-῿‌‍‿⁀⁰-↏Ⰰ-⿯、-퟿豈-﷏ﷰ-�]*$/.test(name) &&
            RESERVED.indexOf(name) < 0;
    }

    function stateOf(el) { return state.get(el); }

    // ---- reactions
    // HTML §4.13.6. Each [CEReactions] member pushes an element queue and
    // invokes it on the way out, after its own steps finish. Reactions
    // therefore see the completed mutation and have run before the call
    // returns to script. cloneNode and importNode are both annotated, so
    // an upgrade enqueued while cloning (create an element with the
    // synchronous custom elements flag unset) runs when that call returns.
    // That is the ordinary rule, the same one innerHTML and append use.
    // With the stack empty, the reaction waits on the backup microtask.
    function connectedNow(el) { return !!el.isConnected; }
    function reactionQueue(el) {
        var q = reactionQueues.get(el);
        if (!q) reactionQueues.set(el, q = []);
        return q;
    }
    function scheduleMicrotask(fn) {
        if (typeof g.queueMicrotask === 'function') g.queueMicrotask(fn);
        else Promise.resolve().then(fn);
    }
    function scheduleBackup() {
        if (backupScheduled || processingBackup) return;
        backupScheduled = true;
        scheduleMicrotask(function () {
            backupScheduled = false;
            processingBackup = true;
            try { invoke(backup); }
            finally { processingBackup = false; }
        });
    }
    function enqueueElement(el) {
        var queue = scopes.length ? scopes[scopes.length - 1] : backup;
        if (queue.indexOf(el) < 0) queue.push(el);
        if (!scopes.length) scheduleBackup();
    }
    function enqueueCallback(el, fn, args) {
        if (!fn) return;
        reactionQueue(el).push(function () { call(fn, el, args); });
        enqueueElement(el);
    }
    function enqueueUpgrade(el, def) {
        reactionQueue(el).push(function () { upgrade(el, def); });
        enqueueElement(el);
    }
    function invoke(queue) {
        while (queue.length) {
            var el = queue.shift();
            var rq = reactionQueue(el);
            while (rq.length) {
                var reaction = rq.shift();
                try { reaction(); } catch (e) { report(e); }
            }
        }
    }
    // Push a queue, run fn, pop, invoke. An exception from fn is rethrown
    // after the queue runs (HTML §4.13.6, [CEReactions] steps).
    function withReactions(fn) {
        scopes.push([]);
        var result, exception, threw = false;
        try { result = fn(); }
        catch (e) { threw = true; exception = e; }
        var queue = scopes.pop();
        invoke(queue);
        if (threw) throw exception;
        return result;
    }
    function enqueueConnected(el, st) {
        if (!st || st.failed || st.connected || !connectedNow(el)) return;
        st.connected = true;
        enqueueCallback(el, st.def.connected, []);
    }
    function enqueueDisconnected(el, st) {
        if (!st || st.failed || !st.connected || connectedNow(el)) return;
        st.connected = false;
        if (st.def.disconnected) enqueueCallback(el, st.def.disconnected, []);
    }
    // A move stays connected, so the isConnected check above would skip it.
    // The node was connected before the call; disconnectedCallback still runs.
    function enqueueMoved(el) {
        var st = stateOf(el);
        if (!st || st.failed || !st.connected) return;
        st.connected = false;
        if (st.def.disconnected) enqueueCallback(el, st.def.disconnected, []);
    }

    // Template contents belong to the inert template document (no browsing
    // context: its defaultView is null). Reactions are for this document.
    // importNode adopts into this document before the wrapper below runs;
    // cloneNode does not, so a copy of template content stays inert.
    function inInertDocument(node) {
        if (!node || typeof node.nodeType !== 'number') return false;
        var doc = node.nodeType === 9 ? node : node.ownerDocument;
        return !!(doc && doc !== document && doc.defaultView == null);
    }

    // Upgrade: run the definition's constructor on an element that already
    // exists (HTML "upgrade an element"). Attribute and connected reactions
    // are enqueued before the constructor, then run when the current queue
    // is next drained — after the constructor when nothing nested pops a
    // queue first. A [CEReactions] call inside the constructor drains them
    // before that call returns, which is the spec's re-entrancy rule.
    function upgrade(el, def) {
        if (inInertDocument(el)) return;
        var existing = stateOf(el);
        if (existing && (existing.def === def || existing.failed)) return;
        var st = { def: def, connected: false, failed: true };
        Object.setPrototypeOf(el, def.ctor.prototype);
        state.set(el, st);
        var observed = def.observed;
        if (observed.length && def.attributeChanged && typeof el.getAttributeNames === 'function') {
            el.getAttributeNames().forEach(function (n) {
                if (observed.indexOf(n) >= 0) enqueueCallback(el, def.attributeChanged, [n, null, el.getAttribute(n)]);
            });
        }
        // Captured now, invoked with the queue. A clone is still
        // disconnected here, so connectedCallback stays queued for the
        // later insertion. Attributes present at upgrade are reported from
        // null before that callback, which is what the first connected
        // layout sees together with property bindings committed in between.
        if (connectedNow(el)) {
            st.connected = true;
            enqueueCallback(el, def.connected, []);
        }
        var record = { element: el, constructed: false };
        stack.push(record);
        var result, threw = false;
        try {
            result = new def.ctor();
        } catch (e) {
            threw = true;
            report(e);
        } finally {
            stack.pop();
        }
        if (threw || result !== el) {
            if (!threw) report(domError('The custom element constructor did not produce the element being upgraded.', 'InvalidStateError'));
            st.failed = true;
            reactionQueue(el).length = 0;
            return;
        }
        st.failed = false;
    }

    // Everything in `root` (and root itself) whose name is defined.
    function candidates(root) {
        if (!selector || !root) return [];
        var out = [];
        try {
            if (root.nodeType === 1 && root.matches(selector)) out.push(root);
            if (typeof root.querySelectorAll === 'function') {
                var found = root.querySelectorAll(selector);
                for (var i = 0; i < found.length; i++) out.push(found[i]);
            }
        } catch (e) { report(e); }
        return out;
    }
    function definitionOf(el) {
        if (el.nodeType !== 1) return null;
        return byName[String(el.localName).toLowerCase()] || null;
    }
    // After a tree change: enqueue an upgrade where the synchronous custom
    // elements flag was unset, or a connected reaction for an element that
    // is already custom. Does not walk a <template>'s contents. Those nodes
    // belong to the inert template-contents document, which has no browsing
    // context; importNode of the content is what upgrades them.
    function reconcile(root) {
        if (inInertDocument(root)) return;
        candidates(root).forEach(function (el) {
            if (inInertDocument(el)) return;
            var def = definitionOf(el);
            if (!def) return;
            var st = stateOf(el);
            if (!st) { enqueueUpgrade(el, def); return; }
            if (!st.failed) enqueueConnected(el, st);
        });
    }
    // Before a removal: the connected custom elements that may leave the document.
    function connectedCustom(root) {
        return candidates(root).filter(function (el) {
            var st = stateOf(el);
            return st && !st.failed && st.connected;
        });
    }
    function afterRemoval(list) {
        list.forEach(function (el) { var st = stateOf(el); if (st) enqueueDisconnected(el, st); });
    }

    // ---- the constructible HTMLElement
    function HTMLElement() {
        var target = new.target;
        if (!target) throw new TypeError("Failed to construct 'HTMLElement': Please use the 'new' operator, this DOM object constructor cannot be called as a function.");
        var def = byCtor ? byCtor.get(target) : null;
        if (!def) throw new TypeError('Illegal constructor');
        var top = stack[stack.length - 1];
        if (top && !top.constructed) {
            // Under an upgrade: `super()` hands back the element being upgraded.
            top.constructed = true;
            return top.element;
        }
        // `new MyElement()`: a fresh element with the defined name.
        var el = rawCreateElement.call(document, def.name);
        Object.setPrototypeOf(el, target.prototype);
        state.set(el, { def: def, connected: false, failed: false });
        return el;
    }
    HTMLElement.prototype = OldHTMLElement.prototype;
    Object.defineProperty(HTMLElement.prototype, 'constructor', { value: HTMLElement, writable: true, configurable: true });
    Object.defineProperty(HTMLElement, 'name', { value: 'HTMLElement' });
    Object.setPrototypeOf(HTMLElement, Object.getPrototypeOf(OldHTMLElement));
    Object.defineProperty(g, 'HTMLElement', { value: HTMLElement, writable: true, configurable: true, enumerable: false });

    // document.createElement builds a defined element synchronously.
    var DocumentProto = Object.getPrototypeOf(document);
    var originalCreate = DocumentProto.createElement;
    rawCreateElement = originalCreate;
    // createElement is [CEReactions] and sets the synchronous custom elements
    // flag, so the constructor runs inside the call and the queue pops as
    // it returns. createElementNS does the same for the HTML namespace.
    // The inert template document has no browsing context, so a name
    // created there stays a plain element.
    function constructDefined(el) {
        if (inInertDocument(el)) return el;
        var def = definitionOf(el);
        if (!def) return el;
        try {
            return new def.ctor();
        } catch (e) {
            report(e);
            return el; // an element that failed to construct stays a plain one
        }
    }
    DocumentProto.createElement = function () {
        var self = this, args = arguments;
        if (!names.length) return originalCreate.apply(self, args);
        return withReactions(function () { return constructDefined(originalCreate.apply(self, args)); });
    };
    if (typeof DocumentProto.createElementNS === 'function') {
        var originalCreateNS = DocumentProto.createElementNS;
        DocumentProto.createElementNS = function (ns) {
            var self = this, args = arguments;
            if (!names.length) return originalCreateNS.apply(self, args);
            return withReactions(function () {
                var el = originalCreateNS.apply(self, args);
                var html = ns == null || ns === '' || ns === 'http://www.w3.org/1999/xhtml';
                return html ? constructDefined(el) : el;
            });
        };
    }

    // The connected elements named `name`, in shadow-including tree order
    // (HTML §4.13.4 define, step "upgrade candidates"): a host's shadow
    // tree, closed ones too, comes before the host's children.
    function upgradeCandidates(name) {
        var light = Array.prototype.slice.call(document.getElementsByTagName(name));
        if (!shadowHost || !shadowHost.call('any', document)) return light;
        var out = [];
        (function walk(node) {
            var kids = node.children;
            for (var i = 0; i < kids.length; i++) {
                var el = kids[i];
                if (String(el.localName).toLowerCase() === name) out.push(el);
                var root = shadowHost.node('root', el);
                if (root) walk(root);
                walk(el);
            }
        })(document);
        return out;
    }

    // ---- the registry
    function CustomElementRegistry() { throw new TypeError('Illegal constructor'); }
    CustomElementRegistry.prototype.define = function (name, ctor, options) {
        name = String(name);
        if (typeof ctor !== 'function') throw new TypeError("Failed to execute 'define' on 'CustomElementRegistry': The callback provided as parameter 2 is not a constructor.");
        if (!validName(name)) throw domError("Failed to execute 'define' on 'CustomElementRegistry': \"" + name + '" is not a valid custom element name', 'SyntaxError');
        if (byName[name]) throw domError("Failed to execute 'define' on 'CustomElementRegistry': the name \"" + name + '" has already been used with this registry', 'NotSupportedError');
        if (byCtor && byCtor.has(ctor)) throw domError("Failed to execute 'define' on 'CustomElementRegistry': this constructor has already been used with this registry", 'NotSupportedError');
        if (options && options.extends !== undefined && options.extends !== null) {
            __rk.unsupported('CustomElementRegistry', 'define(extends)');
            throw domError("Failed to execute 'define' on 'CustomElementRegistry': customized built-in elements are not supported", 'NotSupportedError');
        }
        var proto = ctor.prototype;
        if (proto === null || (typeof proto !== 'object' && typeof proto !== 'function')) {
            throw new TypeError("Failed to execute 'define' on 'CustomElementRegistry': The constructor's 'prototype' is not an object.");
        }
        function callback(key) {
            var fn = proto[key];
            if (fn !== undefined && fn !== null && typeof fn !== 'function') {
                throw new TypeError("Failed to execute 'define' on 'CustomElementRegistry': The '" + key + "' property is not callable.");
            }
            return fn || null;
        }
        var def = {
            name: name, ctor: ctor,
            connected: callback('connectedCallback'), disconnected: callback('disconnectedCallback'),
            attributeChanged: callback('attributeChangedCallback'), observed: []
        };
        if (def.attributeChanged) {
            var list = ctor.observedAttributes;
            if (list !== undefined && list !== null) def.observed = Array.from(list).map(String);
        }
        byName[name] = def;
        if (byCtor) byCtor.set(ctor, def);
        names.push(name);
        selector = names.join(',');
        if (shadowHost) shadowHost.call('define', document, name);
        // define is [CEReactions]. Every connected element of that name
        // upgrades when the call returns, shadow trees included, in
        // shadow-including tree order. One element's reactions finish
        // before the next element's upgrade starts.
        withReactions(function () {
            upgradeCandidates(name).forEach(function (el) { enqueueUpgrade(el, def); });
        });
        if (pending[name]) {
            pending[name].forEach(function (resolve) { resolve(ctor); });
            delete pending[name];
        }
    };
    CustomElementRegistry.prototype.get = function (name) {
        var def = byName[String(name)];
        return def ? def.ctor : undefined;
    };
    CustomElementRegistry.prototype.getName = function (ctor) {
        var def = byCtor ? byCtor.get(ctor) : null;
        return def ? def.name : null;
    };
    CustomElementRegistry.prototype.whenDefined = function (name) {
        name = String(name);
        if (!validName(name)) {
            return Promise.reject(domError("Failed to execute 'whenDefined' on 'CustomElementRegistry': \"" + name + '" is not a valid custom element name', 'SyntaxError'));
        }
        if (byName[name]) return Promise.resolve(byName[name].ctor);
        return new Promise(function (resolve) { (pending[name] || (pending[name] = [])).push(resolve); });
    };
    // upgrade is [CEReactions]: constructors run before it returns.
    CustomElementRegistry.prototype.upgrade = function (root) {
        if (!names.length || !root) return;
        withReactions(function () { reconcile(root); });
    };
    Object.defineProperty(CustomElementRegistry.prototype, Symbol.toStringTag, { value: 'CustomElementRegistry', configurable: true });
    var registry = Object.create(CustomElementRegistry.prototype);
    Object.defineProperty(g, 'CustomElementRegistry', { value: CustomElementRegistry, writable: true, configurable: true, enumerable: false });
    Object.defineProperty(g, 'customElements', { value: registry, writable: true, configurable: true, enumerable: true });

    // ---- wrapping the tree and attribute mutators
    // adoptNode is installed later (web_document.js) and calls this.
    Object.defineProperty(g, '__rkCE', { value: { withReactions: withReactions } });

    function protoWith(name) {
        return [g.Node, g.Element, g.Document, g.DocumentFragment, g.CharacterData].filter(function (C) {
            return C && C.prototype && Object.prototype.hasOwnProperty.call(C.prototype, name);
        }).map(function (C) { return C.prototype; });
    }
    function defineMethod(proto, name, fn) {
        fn.__rkCE = true;
        Object.defineProperty(proto, name, { value: fn, writable: true, configurable: true, enumerable: Object.prototype.propertyIsEnumerable.call(proto, name) });
    }
    function nodesIn(args) {
        var out = [];
        Array.prototype.forEach.call(args, function (a) {
            if (a && typeof a === 'object' && a.nodeType) {
                if (a.nodeType === 11) Array.prototype.forEach.call(a.childNodes, function (c) { out.push(c); });
                else out.push(a);
            }
        });
        return out;
    }
    // One [CEReactions] scope around `native`. `after` enqueues reactions
    // for the current scope; they run when withReactions returns.
    function wrapMethod(proto, name, after) {
        var native = proto && proto[name];
        if (typeof native !== 'function' || native.__rkCE) return;
        defineMethod(proto, name, function () {
            var self = this, args = arguments;
            if (!names.length) return native.apply(self, args);
            return withReactions(function () {
                var result = native.apply(self, args);
                if (after) after(self, args, result);
                return result;
            });
        });
    }
    // Insertions: reconcile what was inserted. `firstOnly` is the Node
    // methods that take one node; the ParentNode/ChildNode methods take
    // the rest of the arguments.
    function wrapInsert(name, firstOnly) {
        protoWith(name).forEach(function (proto) {
            var native = proto[name];
            if (typeof native !== 'function' || native.__rkCE) return;
            defineMethod(proto, name, function () {
                var self = this, args = arguments;
                if (!names.length) return native.apply(self, args);
                return withReactions(function () {
                    var inserted = nodesIn(firstOnly ? [args[0]] : args);
                    // A node that is already connected and is inserted again
                    // is a move: disconnected, then connected at its new place.
                    var moving = [];
                    inserted.forEach(function (n) { moving = moving.concat(connectedCustom(n)); });
                    var leaving = [];
                    if (name === 'replaceChild' && args[1]) leaving = connectedCustom(args[1]);
                    if (name === 'replaceWith') leaving = connectedCustom(self);
                    if (name === 'replaceChildren') leaving = connectedCustom(self).filter(function (el) { return el !== self; });
                    var result = native.apply(self, args);
                    afterRemoval(leaving);
                    moving.forEach(enqueueMoved);
                    inserted.forEach(reconcile);
                    return result;
                });
            });
        });
    }
    ['appendChild', 'insertBefore', 'replaceChild'].forEach(function (n) { wrapInsert(n, true); });
    ['append', 'prepend', 'before', 'after', 'replaceWith', 'replaceChildren'].forEach(function (n) { wrapInsert(n, false); });

    // Both are [CEReactions, NewObject]. The host copy finishes first, then
    // each defined name enqueues an upgrade. The queue runs when the call
    // returns, parent before descendants, while the copy is disconnected.
    // importNode is not cloneNode here: the host import adopts into this
    // document before reconcile, so that copy upgrades. cloneNode keeps
    // the source node document, and upgrade refuses a copy that is still
    // inert. A <template> element's contents are not descendants.
    protoWith('cloneNode').forEach(function (proto) {
        wrapMethod(proto, 'cloneNode', function (_self, _args, copy) { reconcile(copy); });
    });
    wrapMethod(DocumentProto, 'importNode', function (_self, _args, copy) { reconcile(copy); });
    protoWith('normalize').forEach(function (proto) { wrapMethod(proto, 'normalize'); });
    if (g.Range) {
        ['deleteContents', 'extractContents', 'cloneContents', 'insertNode', 'surroundContents'].forEach(function (n) {
            wrapMethod(g.Range.prototype, n);
        });
    }

    function wrapRemove(name, victim) {
        protoWith(name).forEach(function (proto) {
            var native = proto[name];
            if (typeof native !== 'function' || native.__rkCE) return;
            defineMethod(proto, name, function () {
                var self = this, args = arguments;
                if (!names.length) return native.apply(self, args);
                return withReactions(function () {
                    var leaving = connectedCustom(victim(self, args));
                    var result = native.apply(self, args);
                    afterRemoval(leaving);
                    return result;
                });
            });
        });
    }
    wrapRemove('removeChild', function (_self, args) { return args[0]; });
    wrapRemove('remove', function (self) { return self; });

    // innerHTML / textContent / innerText replace the children: those leave.
    // `mode` 'html' also upgrades what the parse inserted. 'text' only
    // disconnects. 'scope' pushes a queue around a setter that delegates
    // to another [CEReactions] member (id, className, DOMTokenList.value).
    function wrapSetter(proto, prop, mode) {
        if (!proto) return;
        var d = Object.getOwnPropertyDescriptor(proto, prop);
        if (!d || !d.set || d.set.__rkCE) return;
        var set = d.set;
        var wrapped = function (v) {
            var self = this;
            if (!names.length) return set.call(self, v);
            return withReactions(function () {
                var leaving = mode === 'scope' ? [] : connectedCustom(self).filter(function (el) { return el !== self; });
                var result = set.call(self, v);
                afterRemoval(leaving);
                if (mode === 'html') reconcile(self);
                return result;
            });
        };
        wrapped.__rkCE = true;
        Object.defineProperty(proto, prop, { get: d.get, set: wrapped, enumerable: d.enumerable, configurable: true });
    }
    wrapSetter(g.Element.prototype, 'innerHTML', 'html');
    wrapSetter(g.Node.prototype, 'textContent', 'text');
    if (g.HTMLElement) wrapSetter(g.HTMLElement.prototype, 'innerText', 'text');
    wrapSetter(g.Element.prototype, 'id', 'scope');
    wrapSetter(g.Element.prototype, 'className', 'scope');
    if (g.CharacterData) wrapSetter(g.CharacterData.prototype, 'nodeValue', 'scope');

    protoWith('insertAdjacentHTML').forEach(function (proto) {
        wrapMethod(proto, 'insertAdjacentHTML', function (self, args) {
            var w = String(args[0]).toLowerCase();
            reconcile(w === 'beforebegin' || w === 'afterend' ? self.parentNode : self);
        });
    });
    // Delegates to insertBefore, which is [CEReactions] itself, so the
    // reaction runs on that nested exit — still before this call returns.
    protoWith('insertAdjacentElement').forEach(function (proto) {
        wrapMethod(proto, 'insertAdjacentElement');
    });

    // Attributes: attributeChangedCallback for the observed ones. An element
    // whose upgrade has not finished (state "failed"/"precustomized") is
    // not custom yet, so a setAttribute inside its constructor does not
    // enqueue a second callback. The ones upgrade queued still run.
    function wrapAttribute(proto, name) {
        var native = proto && proto[name];
        if (typeof native !== 'function' || native.__rkCE) return;
        defineMethod(proto, name, function (attr) {
            var self = this, args = arguments;
            if (!names.length) return native.apply(self, args);
            return withReactions(function () {
                var st = stateOf(self);
                var key = String(attr);
                var watched = st && !st.failed && st.def.attributeChanged && st.def.observed.indexOf(key) >= 0;
                var old = watched ? self.getAttribute(key) : null;
                var result = native.apply(self, args);
                if (watched) {
                    var now = self.getAttribute(key);
                    if (name !== 'removeAttribute' || old !== null) {
                        enqueueCallback(self, st.def.attributeChanged, [key, old, now]);
                    }
                }
                return result;
            });
        });
    }
    wrapAttribute(g.Element.prototype, 'setAttribute');
    wrapAttribute(g.Element.prototype, 'removeAttribute');
    // These call setAttribute/removeAttribute. A scope here keeps the
    // member on the stack; the nested call enqueues the one callback.
    wrapMethod(g.Element.prototype, 'setAttributeNS');
    wrapMethod(g.Element.prototype, 'removeAttributeNS');
    wrapMethod(g.Element.prototype, 'toggleAttribute');
    if (g.DOMTokenList) {
        ['add', 'remove', 'toggle', 'replace'].forEach(function (n) { wrapMethod(g.DOMTokenList.prototype, n); });
        wrapSetter(g.DOMTokenList.prototype, 'value', 'scope');
    }
    if (g.NamedNodeMap) {
        ['setNamedItem', 'setNamedItemNS', 'removeNamedItem', 'removeNamedItemNS'].forEach(function (n) {
            wrapMethod(g.NamedNodeMap.prototype, n);
        });
    }
    if (g.Attr) {
        ['value', 'nodeValue', 'textContent'].forEach(function (n) { wrapSetter(g.Attr.prototype, n, 'scope'); });
    }
})(typeof globalThis === 'object' ? globalThis : this);
