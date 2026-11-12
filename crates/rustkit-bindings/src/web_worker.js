// The page side of dedicated workers (STR-4). `new Worker(url)` fetches the
// script through the script-network bridge, so the page's fetch policy and
// the shield decide whether it loads, then hands the source to the host,
// which runs it on a thread of its own (worker_host.rs, worker_prelude.js).
//
// Messages are structured clones (structured_clone.js), as strings between the threads. The worker's replies are drained by
// `__rustkit_workers_pump`, which the timer pump calls, so every handler
// runs on the page's own thread and turn.
(function () {
    var g = globalThis;
    if (typeof g.__rustkit_worker_spawn !== 'function') return;
    var spawn = g.__rustkit_worker_spawn, post = g.__rustkit_worker_post,
        poll = g.__rustkit_worker_poll, reply = g.__rustkit_worker_reply, terminate = g.__rustkit_worker_terminate;
    var states = new WeakMap();
    var live = {};
    var MAX_QUEUED = 1024;

    function fromBase64(b64) {
        var s = g.atob(b64 || ''), out = new Uint8Array(s.length);
        for (var i = 0; i < s.length; i++) out[i] = s.charCodeAt(i);
        return out;
    }
    function dom(message, name) {
        return typeof g.DOMException === 'function' ? new g.DOMException(message, name) : new Error(message);
    }
    function fire(w, ev) {
        try { w.dispatchEvent(ev); } catch (e) { try { __rk.reportError(e); } catch (_) {} }
    }
    function failLoad(w, st) {
        // Chrome fires an `error` event on the Worker, asynchronously.
        g.setTimeout(function () {
            if (st.dead) return;
            st.dead = true;
            // A plain Event, not an ErrorEvent, as in Chrome.
            fire(w, new g.Event('error'));
        }, 0);
    }
    // The text of a data: URL, or null when it is malformed.
    function dataText(href) {
        var comma = href.indexOf(',');
        if (comma < 0) return null;
        var meta = href.slice(5, comma), body = href.slice(comma + 1);
        try {
            if (/;base64$/i.test(meta)) return new g.TextDecoder().decode(fromBase64(g.decodeURIComponent(body)));
            return g.decodeURIComponent(body);
        } catch (_) { return null; }
    }
    function encode(message, transfer) {
        return g.__rk_sc.serialize(message, transfer, 'Worker');
    }

    // importScripts: fetch the scripts under the page's fetch policy and
    // answer the worker, which waits for the whole set.
    function serviceFetch(st, rid, urlsJson) {
        var urls = JSON.parse(urlsJson), out = [], left = urls.length, net = g.__rustkit_net, id = st.id;
        function settle(i, value) {
            out[i] = value;
            if (--left === 0 && !st.dead && st.id === id) reply(id, rid, JSON.stringify(out));
        }
        urls.forEach(function (raw, i) {
            var href;
            try { href = new g.URL(raw, st.href).href; } catch (_) { settle(i, { ok: false }); return; }
            var nid = net ? net.request({ url: href, destination: 'script', mode: 'cors', credentials: 'same-origin' }, function (r) {
                if (!r.ok || r.status < 200 || r.status >= 300) { settle(i, { ok: false }); return; }
                try { settle(i, { ok: true, src: new g.TextDecoder().decode(fromBase64(r.body_b64)) }); }
                catch (_) { settle(i, { ok: false }); }
            }) : 0;
            if (!nid) settle(i, { ok: false });
        });
    }

    function pump() {
        var ran = 0;
        for (var key in live) {
            var st = live[key];
            if (!st || !st.id) continue;
            var raw = poll(st.id);
            if (raw === 'null') { delete live[key]; continue; }
            var items = JSON.parse(raw);
            for (var i = 0; i < items.length; i++) {
                if (st.dead) break;
                var it = items[i];
                ran++;
                if (it.k === 'm') {
                    var data;
                    try { data = g.__rk_sc.deserialize(it.d); } catch (_) { fire(st.w, new g.MessageEvent('messageerror', { origin: '' })); continue; }
                    fire(st.w, new g.MessageEvent('message', { data: data, origin: '' }));
                } else if (it.k === 'e') {
                    var info = JSON.parse(it.m), ev;
                    if (typeof g.ErrorEvent === 'function') {
                        ev = new g.ErrorEvent('error', {
                            message: info.m, filename: info.f || st.href, lineno: info.l, colno: info.c, cancelable: true
                        });
                    } else {
                        ev = new g.Event('error', { cancelable: true });
                    }
                    fire(st.w, ev);
                } else if (it.k === 'f') {
                    serviceFetch(st, it.r, it.u);
                } else if (it.k === 'x') {
                    delete live[key];
                }
            }
        }
        return ran;
    }

    function handlerProperty(type) {
        return {
            configurable: true,
            enumerable: true,
            get: function () { var st = states.get(this); return st ? st.on[type] || null : null; },
            set: function (fn) {
                var st = states.get(this);
                if (!st) return;
                st.on[type] = typeof fn === 'function' ? fn : null;
                if (!st.reg[type]) {
                    st.reg[type] = true;
                    var w = this;
                    this.addEventListener(type, function (ev) { if (st.on[type]) st.on[type].call(w, ev); });
                }
            }
        };
    }

    class Worker extends g.EventTarget {
        constructor(url, options) {
            if (arguments.length < 1) {
                throw new TypeError("Failed to construct 'Worker': 1 argument required, but only 0 present.");
            }
            super();
            var u;
            try {
                u = new g.URL(String(url), g.document && g.document.baseURI ? g.document.baseURI : undefined);
            } catch (e) {
                throw dom("Failed to construct 'Worker': Script at '" + String(url) + "' cannot be accessed from origin '" + (g.location && g.location.origin) + "'.", 'SyntaxError');
            }
            var st = { w: this, href: u.href, id: 0, dead: false, queue: [], net: 0, on: {}, reg: {} };
            states.set(this, st);
            var name = options && options.name !== undefined ? String(options.name) : '';
            var self = this;
            // The source is in hand (blob:, data:) or comes over the bridge.
            function begin(source) {
                if (st.dead) return;
                var loc = {
                    href: u.href, origin: u.origin, protocol: u.protocol, host: u.host, hostname: u.hostname,
                    port: u.port, pathname: u.pathname, search: u.search, hash: u.hash,
                    ua: g.navigator ? String(g.navigator.userAgent) : ''
                };
                var id = spawn(source, name, JSON.stringify(loc));
                if (!id) { failLoad(self, st); return; }
                st.id = id;
                live[id] = st;
                for (var i = 0; i < st.queue.length; i++) post(id, st.queue[i]);
                st.queue = [];
            }
            if (u.protocol === 'blob:') {
                // Resolved now, as Chrome does: revoking right after `new Worker` does not matter.
                // The bytes are the host's blob store's (createObjectURL put them there).
                var found = __rk.blobGet(u.href.split('#')[0]);
                if (!found) { failLoad(this, st); return; }
                var blobText;
                try { blobText = new g.TextDecoder().decode(new Uint8Array(found[0])); } catch (_) { failLoad(this, st); return; }
                g.queueMicrotask(function () { begin(blobText); });
                return;
            }
            if (u.protocol === 'data:') {
                var text = dataText(u.href);
                if (text === null) { failLoad(this, st); return; }
                g.queueMicrotask(function () { begin(text); });
                return;
            }
            var net = g.__rustkit_net;
            if (!net) { failLoad(this, st); return; }
            st.net = net.request({ url: u.href, destination: 'worker', mode: 'cors', credentials: 'same-origin' }, function (r) {
                st.net = 0;
                if (st.dead) return;
                if (!r.ok || r.status < 200 || r.status >= 300) { failLoad(self, st); return; }
                var source;
                try { source = new g.TextDecoder().decode(fromBase64(r.body_b64)); } catch (_) { failLoad(self, st); return; }
                begin(source);
            });
            if (!st.net) failLoad(this, st);
        }
        postMessage(message, transfer) {
            if (arguments.length < 1) {
                throw new TypeError("Failed to execute 'postMessage' on 'Worker': 1 argument required, but only 0 present.");
            }
            var st = states.get(this);
            if (!st) throw new TypeError('Illegal invocation');
            var json = encode(message, transfer);
            if (st.dead) return;
            if (st.id) post(st.id, json);
            else if (st.queue.length < MAX_QUEUED) st.queue.push(json);
        }
        terminate() {
            var st = states.get(this);
            if (!st) throw new TypeError('Illegal invocation');
            if (st.dead) return;
            st.dead = true;
            st.queue = [];
            if (st.net) { try { g.__rustkit_net.cancel(st.net); } catch (_) {} st.net = 0; }
            if (st.id) { var id = st.id; delete live[id]; st.id = 0; terminate(id); }
        }
    }
    ['message', 'messageerror', 'error'].forEach(function (type) {
        Object.defineProperty(Worker.prototype, 'on' + type, handlerProperty(type));
    });
    Object.defineProperty(Worker.prototype, Symbol.toStringTag, { value: 'Worker', configurable: true });
    Object.defineProperty(g, 'Worker', { value: Worker, writable: true, configurable: true, enumerable: false });
    Object.defineProperty(g, '__rustkit_workers_pump', { value: pump, writable: true, configurable: true, enumerable: false });
})();
