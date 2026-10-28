// window.matchMedia (CSSOM View §4.2). `matches` is the engine's own media
// query evaluator (rustkit-css `media_query_list_matches`, the one `@media`
// rules use) against the window's current size, read live on every access.
// A list with a `change` listener or an `onchange` handler is remembered, and
// `__rkMediaChanged()` (called after the window is resized) fires `change` on
// each one whose answer flipped since it was made or last fired.
(function (g) {
    var watched = [];
    var HANDLER = {};

    function evaluate(media) {
        return __rustkit_media_matches(media, Number(g.innerWidth) || 0, Number(g.innerHeight) || 0) === true;
    }

    function watch(list) {
        if (watched.indexOf(list) < 0) watched.push(list);
    }

    g.matchMedia = function matchMedia(query) {
        if (arguments.length === 0) {
            throw new TypeError("Failed to execute 'matchMedia' on 'Window': 1 argument required, but only 0 present.");
        }
        var media = String(query);
        var listeners = [];
        var handler = null;
        var list = {};
        var last = evaluate(media);
        Object.defineProperties(list, {
            media: { get: function () { return media; }, enumerable: true, configurable: true },
            matches: { get: function () { return evaluate(media); }, enumerable: true, configurable: true },
            onchange: {
                get: function () { return handler; },
                set: function (fn) {
                    handler = (typeof fn === 'function' || (fn && typeof fn === 'object')) ? fn : null;
                    // An event handler keeps the place among the listeners
                    // where it was first set (HTML §8.1.8.1).
                    if (handler) {
                        if (listeners.indexOf(HANDLER) < 0) listeners.push(HANDLER);
                        watch(list);
                    }
                },
                enumerable: true, configurable: true
            }
        });
        list.addEventListener = function (type, cb) {
            if (String(type) !== 'change' || !cb) return;
            if (listeners.indexOf(cb) < 0) listeners.push(cb);
            watch(list);
        };
        list.removeEventListener = function (type, cb) {
            if (String(type) !== 'change') return;
            var at = listeners.indexOf(cb);
            if (at >= 0) listeners.splice(at, 1);
        };
        list.addListener = function (cb) { list.addEventListener('change', cb); };
        list.removeListener = function (cb) { list.removeEventListener('change', cb); };
        list.dispatchEvent = function (event) {
            var all = listeners.slice();
            for (var i = 0; i < all.length; i++) {
                var cb = all[i] === HANDLER ? handler : all[i];
                if (!cb) continue;
                try {
                    if (typeof cb === 'function') cb.call(list, event);
                    else if (cb && typeof cb.handleEvent === 'function') cb.handleEvent(event);
                } catch (e) {
                    if (typeof g.reportError === 'function') g.reportError(e);
                }
            }
            return true;
        };
        Object.defineProperty(list, '__rkFlip', {
            value: function () {
                var now = evaluate(media);
                if (now === last) return;
                last = now;
                var event;
                if (typeof g.MediaQueryListEvent === 'function') {
                    event = new g.MediaQueryListEvent('change', { media: media, matches: now });
                } else {
                    event = { type: 'change', media: media, matches: now };
                }
                list.dispatchEvent(event);
            },
            enumerable: false
        });
        return list;
    };

    Object.defineProperty(g, '__rkMediaChanged', {
        value: function () {
            var lists = watched.slice();
            for (var i = 0; i < lists.length; i++) lists[i].__rkFlip();
        },
        configurable: true, writable: true, enumerable: false
    });
})(globalThis);
