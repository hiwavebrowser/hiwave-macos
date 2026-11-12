// The global scope of a dedicated worker (STR-4). It runs in a JsRuntime of
// its own, on a thread of its own, and starts from nothing: no `window`, no
// `document`, no storage, none of the page's objects. Only what a worker
// scope has is defined here. The host supplies `__rk_worker_out`,
// `__rk_worker_close` and `__rk_worker_error`.
(function (g) {
    'use strict';
    var out = g.__rk_worker_out, closeHost = g.__rk_worker_close, errorHost = g.__rk_worker_error;
    var listeners = Object.create(null);
    var closing = false;

    function hide(name, value) {
        Object.defineProperty(g, name, { value: value, writable: true, configurable: true, enumerable: false });
    }

    class Event {
        constructor(type) {
            this.type = String(type);
            this.target = null;
            this.currentTarget = null;
            this.defaultPrevented = false;
            this.bubbles = false;
            this.cancelable = false;
        }
        preventDefault() {}
        stopPropagation() {}
        stopImmediatePropagation() { this.__stop = true; }
    }
    class MessageEvent extends Event {
        constructor(type, init) {
            super(type);
            init = init || {};
            this.data = 'data' in init ? init.data : null;
            this.origin = init.origin === undefined ? '' : String(init.origin);
            this.lastEventId = '';
            this.source = null;
            this.ports = [];
        }
    }
    hide('Event', Event);
    hide('MessageEvent', MessageEvent);

    function addEventListener(type, cb) {
        if (typeof cb !== 'function' && !(cb && typeof cb.handleEvent === 'function')) return;
        var list = listeners[type] || (listeners[type] = []);
        for (var i = 0; i < list.length; i++) if (list[i] === cb) return;
        list.push(cb);
    }
    function removeEventListener(type, cb) {
        var list = listeners[type];
        if (!list) return;
        var i = list.indexOf(cb);
        if (i >= 0) list.splice(i, 1);
    }
    function dispatchEvent(ev) {
        ev.target = g;
        ev.currentTarget = g;
        var list = (listeners[ev.type] || []).slice();
        for (var i = 0; i < list.length; i++) {
            try {
                if (typeof list[i] === 'function') list[i].call(g, ev);
                else list[i].handleEvent(ev);
            } catch (e) { errorHost(String(e && e.message !== undefined ? e.message : e)); }
            if (ev.__stop) break;
        }
        return true;
    }
    hide('addEventListener', addEventListener);
    hide('removeEventListener', removeEventListener);
    hide('dispatchEvent', dispatchEvent);

    // onmessage and its kin are listeners registered when first assigned, so
    // their order against addEventListener calls is the order of the calls.
    ['message', 'messageerror', 'error'].forEach(function (type) {
        var handler = null, registered = false;
        Object.defineProperty(g, 'on' + type, {
            configurable: true,
            enumerable: true,
            get: function () { return handler; },
            set: function (fn) {
                handler = typeof fn === 'function' ? fn : null;
                if (!registered) {
                    registered = true;
                    addEventListener(type, function (ev) { if (handler) handler.call(g, ev); });
                }
            }
        });
    });

    hide('self', g);
    hide('name', '');
    hide('postMessage', function postMessage(message) {
        if (arguments.length < 1) throw new TypeError("Failed to execute 'postMessage' on 'DedicatedWorkerGlobalScope': 1 argument required, but only 0 present.");
        if (closing) return;
        var json = JSON.stringify(message);
        out(json === undefined ? 'null' : json);
    });
    hide('close', function close() { closing = true; closeHost(); });
    hide('importScripts', function importScripts() {
        throw new Error('importScripts is not available yet');
    });
    hide('__rk_worker_setup', function (name, locJson) {
        g.name = String(name);
        var loc = JSON.parse(locJson);
        loc.toString = function () { return loc.href; };
        hide('location', loc);
        hide('navigator', { userAgent: loc.ua || '', hardwareConcurrency: 1, onLine: true, language: 'en-US', languages: ['en-US'] });
    });
    hide('__rk_worker_dispatch', function (json) {
        if (closing) return;
        dispatchEvent(new MessageEvent('message', { data: JSON.parse(json) }));
    });
})(globalThis);
