// Script elements that script inserts (HTML §4.12.1.1 "prepare the script
// element"): `document.createElement('script')`, a `src` or text, and an
// insert into the document. Pages load whole bundles this way (loaders,
// analytics, a challenge page's check script); until 2026-10-16 such an
// element sat in the tree and nothing fetched or ran it.
//
// An external classic script is queued for the engine, which fetches it
// like a parser-inserted one (its own request, never through page script)
// and runs it as the element's script, then fires `load` or `error`. It is
// force-async (HTML: the flag starts true; the parser is what clears it),
// so it runs when its own fetch finishes. `async = false` clears the flag
// and puts it in the in-order list. An external module is `import()`ed. An
// inline classic script runs at once.
//
// Only elements script created, or scripts that have not started and are not
// already in this document, are prepared (`createElement`, an HTML
// `createElementNS`, a copy of one not yet started, or a script moved out of
// a `<template>` content fragment or another document). The parser's scripts
// ran (or were skipped) with the load. `innerHTML` never runs scripts: that
// setter does not come through here. Each element is prepared once. Not here
// (stated): inline module scripts and `document.write`.
(function (g) {
    var internals = g.__rkScriptInternals;
    delete g.__rkScriptInternals;
    var HOOKS = g.__rkDomHooks;
    if (!internals || !HOOKS) return;
    // Captured before page script can replace them.
    var indirectEval = g.eval, resolved = Promise.resolve(), URL_ = g.URL, Event_ = g.Event;
    var STATE = Symbol('rustkit.script');
    // HTML "force async", initially true. The parser clears it on the
    // scripts it inserts; those never come through here. Adding the `async`
    // attribute, or the `async` setter, clears it. With the flag still set,
    // the script runs as soon as it is fetched. With the flag clear and no
    // `async` attribute (`async = false`), it waits for earlier in-order
    // scripts.
    var FORCE_ASYNC = Symbol('rustkit.forceAsync');
    var XHTML = 'http://www.w3.org/1999/xhtml';
    var scriptProto = g.HTMLScriptElement && g.HTMLScriptElement.prototype;
    if (scriptProto) {
        Object.defineProperty(scriptProto, 'async', {
            configurable: true, enumerable: true,
            get: function () {
                if (this[FORCE_ASYNC] === true) return true;
                return this.hasAttribute('async');
            },
            set: function (v) {
                this[FORCE_ASYNC] = false;
                if (v) this.setAttribute('async', '');
                else this.removeAttribute('async');
            }
        });
    }
    function keepsOrder(el) {
        return el[FORCE_ASYNC] !== true && !el.hasAttribute('async');
    }
    // Created and not yet started: inserts look for scripts only while
    // there is one.
    var fresh = 0;
    var JS_TYPE = /^(?:(?:text|application)\/(?:x-)?(?:java|ecma)script|text\/javascript1\.[0-5]|text\/(?:jscript|livescript))$/;

    function isScript(n) {
        return n != null && n.nodeType === 1 && n.localName === 'script';
    }
    // The script's type (HTML §4.12.1.1 steps 8-10): 'classic', 'module',
    // or null for a data block.
    function kind(el) {
        var type = el.getAttribute('type'), lang = el.getAttribute('language'), t;
        if (type === null ? !lang : type === '') t = 'text/javascript';
        else if (type === null) t = 'text/' + lang;
        else t = type;
        t = t.trim().toLowerCase();
        if (JS_TYPE.test(t)) return 'classic';
        return t === 'module' ? 'module' : null;
    }
    function fire(el, type) {
        resolved.then(function () { el.dispatchEvent(new Event_(type)); });
    }
    // Walk to the root, crossing a shadow root to its host. A script whose
    // root is this document was already in the tree (the parser's, or one
    // this already started). A script whose root is a fragment (`<template>`
    // content) or another document has not started. `isConnected` is the
    // wrong test here: a DOMParser document shares the node table, so its
    // nodes look connected before they join this document.
    function rootOf(n) {
        var p = n, guard = 0, host;
        while (p && guard++ < 10000) {
            if (p.parentNode) { p = p.parentNode; continue; }
            if (p.nodeType === 11 && HOOKS.hostOf) {
                host = HOOKS.hostOf(p);
                if (host && host !== p) { p = host; continue; }
            }
            break;
        }
        return p;
    }
    // True once the element has started, or it already lives in this document
    // and was not created fresh (a parser script, including one in a shadow tree).
    function prepared(el) {
        if (el[STATE] === 'started') return true;
        if (el[STATE] === 'fresh') return false;
        return rootOf(el) === g.document;
    }
    function prepare(el) {
        if (el[STATE] === 'started' || !el.isConnected) return;
        var src = el.getAttribute('src');
        if (src === null && el.textContent === '') return;
        var k = kind(el);
        if (!k) return;
        var wasFresh = el[STATE] === 'fresh';
        el[STATE] = 'started';
        // Only a createElement script incremented `fresh`. A template or
        // fragment script must not drive that counter negative.
        if (wasFresh) fresh--;
        if (k === 'classic' && el.hasAttribute('nomodule')) return;
        if (src !== null) {
            var url;
            try { url = new URL_(src, g.document.baseURI || g.location.href).href; } catch (e) { url = null; }
            if (!url || src === '') return fire(el, 'error');
            if (k === 'module') {
                import(url).then(function () { fire(el, 'load'); }, function () { fire(el, 'error'); });
                return;
            }
            internals.fetch(el, url, keepsOrder(el));
            return;
        }
        if (k === 'module') return;
        try { if (internals.traceInline) internals.traceInline(el); } catch (e) {}
        var was = internals.current(el);
        try {
            indirectEval(el.textContent);
        } catch (e) {
            try { __rk.reportError(e); } catch (_) {}
        } finally {
            internals.current(was);
        }
    }

    HOOKS.scripts = {
        created: function (el) {
            if (!isScript(el) || (el.namespaceURI != null && el.namespaceURI !== XHTML)) return;
            Object.defineProperty(el, STATE, { value: 'fresh', writable: true, configurable: true });
            el[FORCE_ASYNC] = true;
            fresh++;
        },
        // HTML §4.12.1 cloning steps: a copy is started when its original
        // is, so a copy of a script not yet started (or of a subtree holding
        // one) is prepared when it goes in. The parser's scripts, and those
        // `innerHTML` made, are started: their copies never run.
        cloned: function (from, copy) {
            if (from == null || copy == null) return;
            // Tree order, including `<template>` content. `querySelectorAll`
            // does not see those scripts: they are not descendants of the template.
            function collect(n, out) {
                if (!n) return;
                if (isScript(n)) out.push(n);
                if (n.localName === 'template' && n.content) {
                    collect(n.content, out);
                    return;
                }
                for (var c = n.firstChild; c; c = c.nextSibling) collect(c, out);
            }
            var a = [], b = [];
            collect(from, a);
            collect(copy, b);
            for (var i = 0; i < a.length && i < b.length; i++) {
                if (a[i][STATE] === 'fresh' || !prepared(a[i])) this.created(b[i]);
            }
        },
        // Before `node` goes into `parent`: scripts that have not started,
        // prepared in tree order after the insert. A fragment's scripts are
        // walked, not queried. A node that is already in this document and
        // carries no fresh script is a move of parser output: nothing runs again.
        inserting: function (node, parent) {
            if (fresh === 0 && rootOf(node) === g.document && !(isScript(parent) && !prepared(parent))) return null;
            var found = [];
            function consider(n) {
                if (isScript(n) && (n[STATE] === 'fresh' || !prepared(n))) found.push(n);
            }
            function walk(n) {
                if (!n || n.localName === 'template') return;
                consider(n);
                for (var c = n.firstChild; c; c = c.nextSibling) walk(c);
            }
            if (isScript(node)) consider(node);
            else walk(node);
            // Text added to a script already in the document.
            if (isScript(parent)) consider(parent);
            return found.length ? function () { found.forEach(prepare); } : null;
        },
        // A `src` set, or text written, on a script already in the document.
        changed: function (node, op, name) {
            if (isScript(node) && op === 'setAttr' && String(name).toLowerCase() === 'async') node[FORCE_ASYNC] = false;
            if (!isScript(node)) return;
            if (node[STATE] !== 'fresh' && prepared(node)) return;
            if (op === 'setText' || (op === 'setAttr' && String(name).toLowerCase() === 'src')) prepare(node);
        }
    };
})(globalThis);
