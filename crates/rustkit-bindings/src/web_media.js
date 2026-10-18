// CSSOM View: matchMedia, MediaQueryList, MediaQueryListEvent, and a
// visual viewport that is the layout viewport (no pinch zoom).
//
// Queries are answered by the cascade's media evaluator
// (`__rustkit_media_matches`), against window.innerWidth / innerHeight and
// window.devicePixelRatio. A change event fires when that answer flips.
// visualViewport.scale stays 1: it is the pinch-zoom scale, not the
// backing scale. devicePixelRatio is the backing scale.
(function (g) {
    var matchesQuery = g.__rustkit_media_matches;
    delete g.__rustkit_media_matches;
    if (typeof matchesQuery !== 'function' || typeof g.Event !== 'function') return;

    var LAST = new WeakMap();
    var lists = [];

    function evalQuery(media) {
        var dpr = Number(g.devicePixelRatio);
        if (!(dpr > 0)) dpr = 1;
        return matchesQuery(String(media), Number(g.innerWidth) || 0, Number(g.innerHeight) || 0, dpr) === true;
    }

    function MediaQueryListEvent(type, init) {
        if (!(this instanceof MediaQueryListEvent)) {
            throw new TypeError("Failed to construct 'MediaQueryListEvent': Please use the 'new' operator.");
        }
        if (arguments.length < 1) {
            throw new TypeError("Failed to construct 'MediaQueryListEvent': 1 argument required, but only 0 present.");
        }
        g.Event.call(this, type, init);
        init = init || {};
        this.media = init.media === undefined ? '' : String(init.media);
        this.matches = !!init.matches;
    }
    MediaQueryListEvent.prototype = Object.create(g.Event.prototype, {
        constructor: { value: MediaQueryListEvent, writable: true, configurable: true }
    });
    Object.defineProperty(MediaQueryListEvent.prototype, Symbol.toStringTag, { value: 'MediaQueryListEvent', configurable: true });
    g.MediaQueryListEvent = MediaQueryListEvent;

    var MQL = g.MediaQueryList;
    function matchMedia(query) {
        if (arguments.length < 1) {
            throw new TypeError("Failed to execute 'matchMedia' on 'Window': 1 argument required, but only 0 present.");
        }
        var media = String(query);
        var list = Object.create(MQL && MQL.prototype || g.EventTarget.prototype);
        var matched = evalQuery(media);
        LAST.set(list, matched);
        Object.defineProperty(list, 'media', {
            get: function () { return media; },
            enumerable: true, configurable: true
        });
        Object.defineProperty(list, 'matches', {
            get: function () { return evalQuery(media); },
            enumerable: true, configurable: true
        });
        list.addListener = function (cb) { this.addEventListener('change', cb); };
        list.removeListener = function (cb) { this.removeEventListener('change', cb); };
        lists.push(list);
        return list;
    }
    g.matchMedia = matchMedia;

    function notify() {
        var w = Number(g.innerWidth) || 0, h = Number(g.innerHeight) || 0;
        for (var i = 0; i < lists.length; i++) {
            var list = lists[i];
            var now = list.matches;
            if (now === LAST.get(list)) continue;
            LAST.set(list, now);
            var ev = new MediaQueryListEvent('change', { media: list.media, matches: now });
            ev.isTrusted = true;
            list.dispatchEvent(ev);
        }
        if (vv && (w !== vvSize.w || h !== vvSize.h)) {
            vvSize.w = w;
            vvSize.h = h;
            var resize = new g.Event('resize');
            resize.isTrusted = true;
            vv.dispatchEvent(resize);
        }
    }
    g.__rustkit_media_notify = notify;

    // Visual Viewport: the layout viewport. offset is 0 (no pinch, no
    // layout viewport separate from the visual one). pageLeft/pageTop
    // follow the document scroll. scale is the pinch scale, which is 1.
    var vv = null, vvSize = { w: Number(g.innerWidth) || 0, h: Number(g.innerHeight) || 0 };
    if (typeof g.EventTarget === 'function' && g.visualViewport === undefined) {
        var VV = function VisualViewport() {
            throw new TypeError('Illegal constructor');
        };
        Object.defineProperty(VV, 'name', { value: 'VisualViewport' });
        VV.prototype = Object.create(g.EventTarget.prototype);
        Object.defineProperty(VV.prototype, Symbol.toStringTag, { value: 'VisualViewport', configurable: true });
        function getter(name, fn) {
            Object.defineProperty(VV.prototype, name, { get: fn, enumerable: true, configurable: true });
        }
        getter('offsetLeft', function () { return 0; });
        getter('offsetTop', function () { return 0; });
        getter('pageLeft', function () { return Number(g.scrollX) || 0; });
        getter('pageTop', function () { return Number(g.scrollY) || 0; });
        getter('width', function () { return Number(g.innerWidth) || 0; });
        getter('height', function () { return Number(g.innerHeight) || 0; });
        getter('scale', function () { return 1; });
        vv = Object.create(VV.prototype);
        g.visualViewport = vv;
        g.VisualViewport = VV;
    }
})(globalThis);
