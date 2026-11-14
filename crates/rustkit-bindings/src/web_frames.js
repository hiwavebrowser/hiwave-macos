// Nested browsing contexts, Stage 2 (docs/diagnostics/2026-11-11/
// nested-browsing-contexts-plan.md): the page half of `<iframe>`.
//
// Installed by `DomBindings::enable_frames`, which the engine calls only with
// `EngineConfig::nested_contexts` on. Without it nothing here exists and an
// iframe's `contentWindow`/`contentDocument` stay `null` (web_reflect.js).
//
// What it does: tells the engine which frame to load what (an iframe going
// into the document, its `src` set while it is in, it leaving), and answers
// `contentWindow` / `contentDocument` with the window the engine attached.
// The engine does the fetching, the origin check, the child realm, the
// child's scripts and the `load` event (`__rk.frames.loaded`).
//
// Stated limits of this stage: `contentWindow` is `null` until the engine
// has drained the request (after the script that inserted the frame
// returns; HTML makes it `about:blank` at once). A cross-origin frame is
// left blank: `contentWindow` and `contentDocument` stay `null`, and `load`
// fires. `srcdoc`, `name` targeting and `window.frames[i]` are not here.
(function (g) {
    var HOOKS = __rk.domHooks;
    var request = g.__rustkit_frame_request;
    if (!HOOKS || !HOOKS.nodeId || typeof request !== 'function' || __rk.frames) return;
    delete g.__rustkit_frame_request;
    // Taken now, before page script: the engine's side below (the scan,
    // a frame's size) calls only these and plain loops.
    var URL_ = g.URL, doc = g.document;
    var uncurry = Function.prototype.bind.bind(Function.prototype.call);
    var getProto = Object.getPrototypeOf, gOPD = Object.getOwnPropertyDescriptor, create = Object.create;
    // The getter of an accessor found on `o` or its prototypes, uncurried.
    function getterOf(o, name) {
        for (; o; o = getProto(o)) {
            var d = gOPD(o, name);
            if (d) return typeof d.get === 'function' ? uncurry(d.get) : null;
        }
        return null;
    }
    function methodOf(o, name) { return o && typeof o[name] === 'function' ? uncurry(o[name]) : null; }
    var E = g.Element && g.Element.prototype;
    var docAll = methodOf(doc, 'querySelectorAll'), elAll = methodOf(E, 'querySelectorAll'),
        getAttr = methodOf(E, 'getAttribute'), connected = getterOf(g.Node && g.Node.prototype, 'isConnected'),
        baseURI = getterOf(doc, 'baseURI'), trim = uncurry(String.prototype.trim),
        exec = uncurry(RegExp.prototype.exec), round = Math.round, toFloat = parseFloat, toInt = parseInt;
    var frames = create(null); // node id -> { el, win }
    function isConnected(n) { return connected ? connected(n) : n.isConnected; }
    function record(el) { var r = create(null); r.el = el; r.win = null; return r; }

    function isFrame(n) {
        return n != null && n.nodeType === 1 && n.localName === 'iframe';
    }
    function each(n, f) {
        if (isFrame(n)) return f(n);
        if (n == null) return;
        var list = n === doc && docAll ? docAll(n, 'iframe')
            : n.nodeType === 1 && elAll ? elAll(n, 'iframe')
            : typeof n.querySelectorAll === 'function' ? n.querySelectorAll('iframe') : null;
        for (var i = 0; list && i < list.length; i++) f(list[i]);
    }
    // The URL the frame is to show: its `src` resolved, or about:blank.
    function target(el) {
        var src = getAttr(el, 'src');
        if (src === null || trim(src) === '') return 'about:blank';
        try { return new URL_(src, (baseURI ? baseURI(doc) : doc.baseURI) || g.location.href).href; } catch (e) { return 'about:blank'; }
    }
    function navigate(el) {
        var id = HOOKS.nodeId(el);
        if (id < 0) return;
        frames[id] = record(el);
        request(id, target(el));
    }
    function gone(el) {
        var id = HOOKS.nodeId(el);
        if (id < 0 || !frames[id]) return;
        delete frames[id];
        request(id, null);
    }

    HOOKS.frames = {
        tree: function (op, node) {
            if (op === 'insert') each(node, function (f) { if (isConnected(f)) navigate(f); });
            else if (op === 'remove') each(node, gone);
        },
        changed: function (node, op, name) {
            if (isFrame(node) && isConnected(node) && op === 'setAttr' && String(name).toLowerCase() === 'src') navigate(node);
        }
    };

    // The engine's side, on the host object.
    __rk.frames = Object.setPrototypeOf({
        // The parser's iframes, once the document is in.
        scan: function () {
            var list = HOOKS.byTag('iframe');
            for (var i = 0; i < list.length; i++) {
                var f = list[i], id = HOOKS.nodeId(f);
                if (id >= 0 && !frames[id] && isConnected(f)) navigate(f);
            }
        },
        // The frame's element when it took the window, or null.
        attach: function (id, win) {
            var r = frames[id];
            if (r) r.win = win;
            return r ? r.el : null;
        },
        loaded: function (id) {
            var r = frames[id];
            // The engine's own event, through its own dispatch: whatever
            // the page has put on the element or the prototypes since.
            if (r) __rk.dispatchTo(r.el, __rk.trust(__rk.make('Event', 'load')));
            return !!r;
        },
        // The child's viewport until layout sizes frames (Stage 3): the
        // element's own CSS px width/height, its attributes, or 300x150.
        size: function (id) {
            var r = frames[id], w = 300, h = 150;
            if (!r) return '300x150';
            // The inline style is read from the `style` attribute's text.
            var el = r.el, st = getAttr(el, 'style') || '';
            function px(re) { var m = exec(re, st); return m ? round(toFloat(m[1])) : 0; }
            function attr(n) { var v = getAttr(el, n); return v && exec(/^\s*[0-9]+\s*$/, v) ? toInt(v, 10) : 0; }
            w = px(/(?:^|;)\s*width\s*:\s*([0-9.]+)px\s*(?:;|$)/i) || attr('width') || w;
            h = px(/(?:^|;)\s*height\s*:\s*([0-9.]+)px\s*(?:;|$)/i) || attr('height') || h;
            return w + 'x' + h;
        }
    }, null);

    var F = g.HTMLIFrameElement && g.HTMLIFrameElement.prototype;
    if (F) {
        Object.defineProperty(F, 'contentWindow', {
            get: function () { var r = frames[HOOKS.nodeId(this)]; return r && r.win || null; },
            enumerable: true, configurable: true
        });
        Object.defineProperty(F, 'contentDocument', {
            get: function () { var r = frames[HOOKS.nodeId(this)]; return r && r.win ? r.win.document : null; },
            enumerable: true, configurable: true
        });
    }
})(globalThis);
