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
// child's scripts and the `load` event (`__rustkit_frames.loaded`).
//
// Stated limits of this stage: `contentWindow` is `null` until the engine
// has drained the request (after the script that inserted the frame
// returns; HTML makes it `about:blank` at once). A cross-origin frame is
// left blank: `contentWindow` and `contentDocument` stay `null`, and `load`
// fires. `srcdoc`, `name` targeting and `window.frames[i]` are not here.
(function (g) {
    var HOOKS = g.__rkDomHooks;
    var request = g.__rustkit_frame_request;
    if (!HOOKS || !HOOKS.nodeId || typeof request !== 'function' || g.__rustkit_frames) return;
    delete g.__rustkit_frame_request;
    // Captured before page script can replace them.
    var URL_ = g.URL, Event_ = g.Event, forEach = Array.prototype.forEach, doc = g.document;
    var frames = Object.create(null); // node id -> { el, win }

    function isFrame(n) {
        return n != null && n.nodeType === 1 && n.localName === 'iframe';
    }
    function each(n, f) {
        if (isFrame(n)) return f(n);
        if (n != null && typeof n.querySelectorAll === 'function') forEach.call(n.querySelectorAll('iframe'), f);
    }
    // The URL the frame is to show: its `src` resolved, or about:blank.
    function target(el) {
        var src = el.getAttribute('src');
        if (src === null || src.trim() === '') return 'about:blank';
        try { return new URL_(src, doc.baseURI || g.location.href).href; } catch (e) { return 'about:blank'; }
    }
    function navigate(el) {
        var id = HOOKS.nodeId(el);
        if (id < 0) return;
        frames[id] = { el: el, win: null };
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
            if (op === 'insert') each(node, function (f) { if (f.isConnected) navigate(f); });
            else if (op === 'remove') each(node, gone);
        },
        changed: function (node, op, name) {
            if (isFrame(node) && node.isConnected && op === 'setAttr' && String(name).toLowerCase() === 'src') navigate(node);
        }
    };

    // The engine's side.
    Object.defineProperty(g, '__rustkit_frames', {
        value: {
            // The parser's iframes, once the document is in.
            scan: function () {
                forEach.call(doc.querySelectorAll('iframe'), function (f) {
                    var id = HOOKS.nodeId(f);
                    if (id >= 0 && !frames[id] && f.isConnected) navigate(f);
                });
            },
            attach: function (id, win) {
                var r = frames[id];
                if (r) r.win = win;
                return !!r;
            },
            loaded: function (id) {
                var r = frames[id];
                if (r) r.el.dispatchEvent(new Event_('load'));
                return !!r;
            },
            // The child's viewport until layout sizes frames (Stage 3): the
            // element's own CSS px width/height, its attributes, or 300x150.
            size: function (id) {
                var r = frames[id], w = 300, h = 150;
                if (!r) return '300x150';
                var el = r.el, st = el.style;
                function px(v) { var m = /^\s*([0-9.]+)px\s*$/.exec(v || ''); return m ? Math.round(parseFloat(m[1])) : 0; }
                function attr(n) { var v = el.getAttribute(n); return v && /^\s*[0-9]+\s*$/.test(v) ? parseInt(v, 10) : 0; }
                w = px(st && st.width) || attr('width') || w;
                h = px(st && st.height) || attr('height') || h;
                return w + 'x' + h;
            }
        },
        configurable: true
    });

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
