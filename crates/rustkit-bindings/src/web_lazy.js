// Lazy installation for a nested page's realm (`DomBindings::new_child`).
//
// A child realm installs every page script again, and that is most of what
// a frame costs to set up (2026-11-12: ~97 ms of a child page's ~97 ms in
// the test build). Four interfaces most frames never touch are installed
// here on first use instead: Intl (and the toLocale*String methods over
// it), Streams, Web Animations, and the canvas 2D context. Until then each
// entry point is a stand-in that installs the real script in this realm
// and answers through it, so script sees the same values either way.
//
// The sources come in `__rustkit_lazy_sources` (taken and deleted here);
// each runs as the eager install would have run it, with the host object
// (`__rk`) as its argument.
(function (g) {
    var SOURCES = g.__rustkit_lazy_sources;
    delete g.__rustkit_lazy_sources;
    if (!SOURCES) return;
    var host = __rk, Fn = Function, gOPD = Object.getOwnPropertyDescriptor, dP = Object.defineProperty;

    function lazy(key, globals, methods, accessors) {
        var src = SOURCES[key], done = false, undo = [];
        if (typeof src !== 'string') return;
        function install() {
            if (done) return;
            done = true;
            for (var i = undo.length - 1; i >= 0; i--) undo[i]();
            Fn('__rk', src).call(g, host);
        }
        function keep(obj, name) {
            var had = gOPD(obj, name);
            undo.push(function () {
                if (had) dP(obj, name, had);
                else delete obj[name];
            });
        }
        globals.forEach(function (name) {
            if (gOPD(g, name)) return; // someone defined it already: leave it
            keep(g, name);
            dP(g, name, {
                get: function () { install(); return g[name]; },
                set: function (v) { install(); g[name] = v; },
                enumerable: false, configurable: true
            });
        });
        methods.forEach(function (m) {
            var obj = m[0], name = m[1];
            if (!obj) return;
            keep(obj, name);
            dP(obj, name, {
                value: function () { install(); return obj[name].apply(this, arguments); },
                writable: true, enumerable: false, configurable: true
            });
        });
        (accessors || []).forEach(function (a) {
            var obj = a[0], name = a[1];
            if (!obj) return;
            keep(obj, name);
            dP(obj, name, {
                get: function () { install(); return this[name]; },
                set: function (v) { install(); this[name] = v; },
                enumerable: true, configurable: true
            });
        });
    }

    var D = g.Date && g.Date.prototype, N = g.Number && g.Number.prototype,
        B = g.BigInt && g.BigInt.prototype, S = g.String && g.String.prototype,
        A = g.Array && g.Array.prototype;
    lazy('intl', ['Intl'], [
        [N, 'toLocaleString'], [B, 'toLocaleString'], [D, 'toLocaleString'],
        [D, 'toLocaleDateString'], [D, 'toLocaleTimeString'], [S, 'localeCompare'], [A, 'toLocaleString']
    ]);
    lazy('streams', ['ReadableStream', 'ReadableStreamDefaultReader', 'ReadableStreamDefaultController',
        'WritableStream', 'WritableStreamDefaultWriter', 'WritableStreamDefaultController',
        'TransformStream', 'TransformStreamDefaultController', 'CountQueuingStrategy',
        'ByteLengthQueuingStrategy', 'TextEncoderStream', 'TextDecoderStream'], []);
    var E = g.Element && g.Element.prototype, Doc = g.Document && g.Document.prototype;
    lazy('animations', ['Animation', 'KeyframeEffect', 'AnimationEffect', 'AnimationTimeline',
        'DocumentTimeline', 'AnimationPlaybackEvent', 'CSSAnimation', 'CSSTransition'],
        [[E, 'animate'], [E, 'getAnimations'], [Doc, 'getAnimations'], [g.document, 'getAnimations']],
        [[g.document, 'timeline']]);
    var C = g.HTMLCanvasElement && g.HTMLCanvasElement.prototype;
    lazy('canvas', ['CanvasRenderingContext2D', 'CanvasGradient', 'CanvasPattern', 'TextMetrics', 'ImageData'],
        [[C, 'getContext'], [C, 'toDataURL'], [C, 'toBlob']],
        [[C, 'width'], [C, 'height']]);
})(globalThis);
