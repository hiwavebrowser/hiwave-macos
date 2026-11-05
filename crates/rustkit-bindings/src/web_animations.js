// Web Animations, the part sites call: `Element.animate(keyframes, options)`,
// `Animation`, `KeyframeEffect`, `document.timeline`, `getAnimations()`.
// Without `animate` a page that ships an element at `opacity:0` and fades it
// in with `el.animate(...).finished.then(() => el.style.opacity = "")`
// threw and left the element transparent (a site's hero, 2026-11-04, S8).
//
// Clock: the page's own timer clock. The engine has no animation clock of
// its own (CSS animations settle on their end frame, transitions do not
// run), so animations tick on animation frames (`requestAnimationFrame`, as
// installed before page script), and the timeline's time is the frame's
// timestamp. A played animation starts on the next frame, as in a browser.
//
// Effect: the animated values are written into the element's inline style
// and the style the page had is put back, exactly, when no animation is in
// effect any more (finished without a forwards fill, cancelled). A value the
// page writes during the animation becomes the new underlying value.
// Stated limits: the writes are visible as `style` attribute changes
// (getAttribute, MutationObserver) while an animation is in effect; an
// inline `!important` declaration is left alone (as in a browser, where it
// wins). Interpolated: numbers with matching units (opacity, lengths,
// transforms with the same function list or `none`, rgb()/rgba()/#hex
// colours); anything else flips at the midpoint. Reported unsupported:
// `composite` other than replace, `pseudoElement`, `reverse()`,
// `updatePlaybackRate()`, a `playbackRate` other than 1.
(function (g) {
    var document = g.document;
    if (!document || !g.Element) return;

    var raf = g.requestAnimationFrame;
    var Promise_ = Promise;

    function report(e) {
        try { __rk.reportError(e); } catch (_) {}
    }
    function domError(name, message) {
        return typeof g.DOMException === 'function' ? new g.DOMException(message, name)
            : (function () { var e = new Error(message); e.name = name; return e; })();
    }
    function microtask(f) {
        Promise_.resolve().then(function () { try { f(); } catch (e) { report(e); } });
    }

    // ---------------------------------------------------------------- easing
    function bezier(x1, y1, x2, y2) {
        function a(p1, p2) { return 1 - 3 * p2 + 3 * p1; }
        function b(p1, p2) { return 3 * p2 - 6 * p1; }
        function c(p1) { return 3 * p1; }
        function at(t, p1, p2) { return ((a(p1, p2) * t + b(p1, p2)) * t + c(p1)) * t; }
        function slope(t, p1, p2) { return 3 * a(p1, p2) * t * t + 2 * b(p1, p2) * t + c(p1); }
        return function (x) {
            if (x <= 0) return x1 > 0 ? (y1 / x1) * x : (x2 > 0 ? (y2 / x2) * x : 0);
            if (x >= 1) return x2 < 1 ? 1 + ((y2 - 1) / (x2 - 1)) * (x - 1) : 1;
            var t = x;
            for (var i = 0; i < 8; i++) {
                var err = at(t, x1, x2) - x;
                if (Math.abs(err) < 1e-6) break;
                var d = slope(t, x1, x2);
                if (Math.abs(d) < 1e-6) break;
                t -= err / d;
            }
            if (t < 0 || t > 1 || Math.abs(at(t, x1, x2) - x) > 1e-4) {
                var lo = 0, hi = 1;
                t = x;
                for (var j = 0; j < 30; j++) {
                    if (at(t, x1, x2) < x) lo = t; else hi = t;
                    t = (lo + hi) / 2;
                }
            }
            return at(t, y1, y2);
        };
    }
    function linear(x) { return x; }
    var NAMED = {
        'linear': linear,
        'ease': bezier(0.25, 0.1, 0.25, 1),
        'ease-in': bezier(0.42, 0, 1, 1),
        'ease-out': bezier(0, 0, 0.58, 1),
        'ease-in-out': bezier(0.42, 0, 0.58, 1)
    };
    function parseEasing(s) {
        if (s == null) return linear;
        var str = String(s).trim().toLowerCase();
        if (NAMED[str]) return NAMED[str];
        var m = /^cubic-bezier\(([^)]*)\)$/.exec(str);
        if (m) {
            var p = m[1].split(',').map(Number);
            if (p.length === 4 && p.every(isFinite) && p[0] >= 0 && p[0] <= 1 && p[2] >= 0 && p[2] <= 1) {
                return bezier(p[0], p[1], p[2], p[3]);
            }
        }
        m = /^steps\(\s*(\d+)\s*(?:,\s*([a-z-]+)\s*)?\)$/.exec(str);
        if (m) {
            var n = parseInt(m[1], 10), pos = m[2] || 'end';
            if (n > 0) {
                return function (x) {
                    var jumps = pos === 'jump-both' ? n + 1 : pos === 'jump-none' ? n - 1 : n;
                    var step = Math.floor(x * n);
                    if (pos === 'start' || pos === 'jump-start' || pos === 'jump-both') step += 1;
                    if (x >= 0 && step < 0) step = 0;
                    if (x <= 1 && step > jumps) step = jumps;
                    return jumps > 0 ? step / jumps : 1;
                };
            }
        }
        if (str === 'step-start') return parseEasing('steps(1, start)');
        if (str === 'step-end') return parseEasing('steps(1, end)');
        throw new TypeError("Failed to execute 'animate' on 'Element': '" + s + "' is not a valid value for easing");
    }

    // ---------------------------------------------------------- interpolation
    var NUM = /(-?(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?)([a-z%]*)/gi;
    function hexToRgba(s) {
        return s.replace(/#([0-9a-f]{3,8})\b/gi, function (all, h) {
            if (h.length === 3 || h.length === 4) h = h.split('').map(function (c) { return c + c; }).join('');
            if (h.length !== 6 && h.length !== 8) return all;
            var a = h.length === 8 ? parseInt(h.slice(6), 16) / 255 : 1;
            return 'rgba(' + parseInt(h.slice(0, 2), 16) + ', ' + parseInt(h.slice(2, 4), 16) + ', ' +
                parseInt(h.slice(4, 6), 16) + ', ' + a + ')';
        });
    }
    function normalize(v) {
        var s = hexToRgba(String(v).trim());
        // rgb(a, b, c) -> rgba(a, b, c, 1) so both sides line up.
        return s.replace(/rgb\(\s*([^,()]+),\s*([^,()]+),\s*([^,()]+)\)/gi, 'rgba($1, $2, $3, 1)');
    }
    function tokens(s) {
        var nums = [], units = [];
        var skeleton = s.replace(NUM, function (all, n, u) {
            nums.push(parseFloat(n)); units.push(u.toLowerCase());
            return '\u0000';
        });
        return { skeleton: skeleton, nums: nums, units: units };
    }
    // `none` against a transform list: the identity of that list.
    function identityOf(list) {
        return list.replace(/([a-z0-9]+)\(([^)]*)\)/gi, function (all, name, args) {
            var one = /^scale/i.test(name);
            return name + '(' + args.split(',').map(function (a) {
                var m = /^\s*-?(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?([a-z%]*)\s*$/i.exec(a);
                return one ? '1' : '0' + (m ? m[1] : '');
            }).join(', ') + ')';
        });
    }
    function fmt(n) { return String(Math.round(n * 10000) / 10000); }
    function interpolate(prop, from, to, p) {
        var a = normalize(from), b = normalize(to);
        if (prop === 'transform') {
            if (a === 'none' && b !== 'none') a = identityOf(b);
            else if (b === 'none' && a !== 'none') b = identityOf(a);
        }
        var ta = tokens(a), tb = tokens(b);
        if (ta.skeleton !== tb.skeleton || ta.nums.length !== tb.nums.length || ta.nums.length === 0) {
            return p < 0.5 ? from : to;
        }
        var units = [];
        for (var i = 0; i < ta.units.length; i++) {
            var ua = ta.units[i], ub = tb.units[i];
            if (ua === ub) units.push(ua);
            else if (ua === '' && ta.nums[i] === 0) units.push(ub);
            else if (ub === '' && tb.nums[i] === 0) units.push(ua);
            else return p < 0.5 ? from : to;
        }
        var k = 0;
        return ta.skeleton.replace(/\u0000/g, function () {
            var v = ta.nums[k] + (tb.nums[k] - ta.nums[k]) * p;
            var out = fmt(v) + units[k];
            k++;
            return out;
        }).replace(/rgba\(([^,]+),\s*([^,]+),\s*([^,]+),/gi, function (all, r, gr, bl) {
            function c(x) { return String(Math.max(0, Math.min(255, Math.round(parseFloat(x))))); }
            return 'rgba(' + c(r) + ', ' + c(gr) + ', ' + c(bl) + ',';
        });
    }
    function finalValue(prop, v) {
        if (prop === 'opacity') {
            var n = parseFloat(v);
            if (isFinite(n)) return fmt(Math.max(0, Math.min(1, n)));
        }
        return v;
    }

    // -------------------------------------------------------------- keyframes
    function cssName(key) {
        if (key === 'cssFloat') return 'float';
        if (key === 'cssOffset') return 'offset';
        if (key.indexOf('--') === 0) return key;
        return key.replace(/[A-Z]/g, function (c) { return '-' + c.toLowerCase(); });
    }
    var RESERVED = { offset: 1, easing: 1, composite: 1 };
    function parseKeyframes(input) {
        if (input == null) return [];
        var frames = [];
        if (typeof input[Symbol.iterator] === 'function' && typeof input !== 'string') {
            var list = Array.prototype.slice.call(input);
            for (var i = 0; i < list.length; i++) {
                var f = list[i] || {};
                var kf = { offset: f.offset == null ? null : Number(f.offset), easing: f.easing || 'linear', values: {} };
                if (f.composite && f.composite !== 'replace' && f.composite !== 'auto') __rk.unsupported('Element', 'animate(composite)');
                Object.keys(f).forEach(function (k) {
                    if (!RESERVED[k] && f[k] != null) kf.values[cssName(k)] = String(f[k]);
                });
                frames.push(kf);
            }
        } else if (typeof input === 'object') {
            var n = 0;
            Object.keys(input).forEach(function (k) {
                if (RESERVED[k]) return;
                var v = input[k];
                var len = Array.isArray(v) ? v.length : 1;
                if (len > n) n = len;
            });
            // Each property spaces its own values evenly; merge by offset.
            var byOffset = {};
            Object.keys(input).forEach(function (k) {
                if (RESERVED[k]) return;
                var v = Array.isArray(input[k]) ? input[k] : [input[k]];
                v.forEach(function (val, j) {
                    var off = v.length === 1 ? 1 : j / (v.length - 1);
                    var key = String(off);
                    var kf = byOffset[key] || (byOffset[key] = { offset: off, easing: 'linear', values: {} });
                    kf.values[cssName(k)] = String(val);
                });
            });
            if (input.composite && input.composite !== 'replace' && input.composite !== 'auto') __rk.unsupported('Element', 'animate(composite)');
            var easings = input.easing == null ? [] : (Array.isArray(input.easing) ? input.easing : [input.easing]);
            frames = Object.keys(byOffset).map(function (k) { return byOffset[k]; })
                .sort(function (x, y) { return x.offset - y.offset; });
            frames.forEach(function (f, j) { if (easings.length) f.easing = easings[j % easings.length]; });
        } else {
            throw new TypeError("Failed to execute 'animate' on 'Element': The provided value is not of type '(sequence<object> or object)'.");
        }
        // Computed offsets: missing ones spaced evenly between their neighbours.
        if (frames.length === 1 && frames[0].offset == null) frames[0].offset = 1;
        if (frames.length > 1) {
            if (frames[0].offset == null) frames[0].offset = 0;
            if (frames[frames.length - 1].offset == null) frames[frames.length - 1].offset = 1;
        }
        for (var s = 0; s < frames.length; s++) {
            if (frames[s].offset != null) continue;
            var e = s;
            while (frames[e].offset == null) e++;
            var start = frames[s - 1].offset, end = frames[e].offset;
            for (var q = s; q < e; q++) frames[q].offset = start + (end - start) * (q - s + 1) / (e - s + 1);
            s = e;
        }
        frames.forEach(function (f) {
            if (!(f.offset >= 0 && f.offset <= 1)) throw new TypeError("Offsets must be null or in the range [0,1].");
            f.ease = parseEasing(f.easing);
        });
        return frames;
    }

    // ----------------------------------------------------------------- timing
    function parseTiming(options) {
        var t = { delay: 0, endDelay: 0, fill: 'auto', iterationStart: 0, iterations: 1,
                  duration: 'auto', direction: 'normal', easing: 'linear' };
        if (typeof options === 'number') t.duration = options;
        else if (options && typeof options === 'object') {
            ['delay', 'endDelay', 'fill', 'iterationStart', 'iterations', 'duration', 'direction', 'easing']
                .forEach(function (k) { if (options[k] !== undefined) t[k] = options[k]; });
            if (options.pseudoElement) __rk.unsupported('Element', 'animate(pseudoElement)');
            if (options.composite && options.composite !== 'replace' && options.composite !== 'auto') __rk.unsupported('Element', 'animate(composite)');
        }
        t.delay = Number(t.delay) || 0;
        t.endDelay = Number(t.endDelay) || 0;
        t.iterations = Number(t.iterations);
        if (!(t.iterations >= 0)) throw new TypeError("iterations must be non-negative");
        t.iterationStart = Number(t.iterationStart) || 0;
        if (t.duration !== 'auto') {
            t.duration = Number(t.duration);
            if (!(t.duration >= 0)) throw new TypeError("duration must be non-negative");
        }
        t.ease = parseEasing(t.easing);
        return t;
    }
    function dur(t) { return t.duration === 'auto' ? 0 : t.duration; }
    function activeDuration(t) { var d = dur(t); return d === 0 || t.iterations === 0 ? 0 : d * t.iterations; }
    function endTime(t) { return Math.max(t.delay + activeDuration(t) + t.endDelay, 0); }

    // Progress at local time `lt`, or null when the effect is not in effect.
    function computed(t, lt) {
        var ad = activeDuration(t), d = dur(t);
        var fill = t.fill === 'auto' ? 'none' : t.fill;
        var beforeEnd = Math.max(Math.min(t.delay, endTime(t)), 0);
        var activeEnd = Math.max(Math.min(t.delay + ad, endTime(t)), 0);
        var phase = lt == null ? 'idle' : lt < beforeEnd ? 'before' : lt >= activeEnd && !(ad === 0 && lt < activeEnd) ? 'after' : 'active';
        var active;
        if (phase === 'before') active = (fill === 'backwards' || fill === 'both') ? 0 : null;
        else if (phase === 'active') active = lt - t.delay;
        else if (phase === 'after') active = (fill === 'forwards' || fill === 'both') ? Math.max(Math.min(lt - t.delay, ad), 0) : null;
        else active = null;
        if (active == null) return { phase: phase, progress: null, iteration: null, local: lt };
        var overall = d === 0 ? (phase === 'before' ? 0 : t.iterations) : active / d;
        overall += t.iterationStart;
        var simple = overall === Infinity ? t.iterationStart % 1 : overall % 1;
        if (simple === 0 && (phase === 'active' || phase === 'after') && active === ad && t.iterations !== 0 && overall !== 0) simple = 1;
        var iteration = phase === 'after' && t.iterations === Infinity ? Infinity : Math.floor(overall);
        if (simple === 1) iteration -= 1;
        var dir = t.direction, forwards = true;
        if (dir === 'reverse') forwards = false;
        else if (dir === 'alternate') forwards = iteration % 2 === 0 || iteration === Infinity;
        else if (dir === 'alternate-reverse') forwards = !(iteration % 2 === 0 || iteration === Infinity);
        var directed = forwards ? simple : 1 - simple;
        return { phase: phase, progress: t.ease(directed), iteration: iteration, local: lt };
    }

    // ----------------------------------------------------------------- effect
    function KeyframeEffect(target, keyframes, options) {
        if (!(this instanceof KeyframeEffect)) throw new TypeError("Failed to construct 'KeyframeEffect': Please use the 'new' operator.");
        if (target instanceof KeyframeEffect) {
            this.target = target.target; this._frames = target._frames; this._timing = Object.assign({}, target._timing);
        } else {
            this.target = target || null;
            this._frames = parseKeyframes(keyframes);
            this._timing = parseTiming(options);
        }
        this.composite = 'replace';
        this.pseudoElement = null;
        this._animation = null;
    }
    KeyframeEffect.prototype.getKeyframes = function () {
        return this._frames.map(function (f) {
            var o = { offset: f.offset, computedOffset: f.offset, easing: f.easing, composite: 'auto' };
            Object.keys(f.values).forEach(function (k) {
                o[k.replace(/-([a-z])/g, function (m, c) { return c.toUpperCase(); })] = f.values[k];
            });
            return o;
        });
    };
    KeyframeEffect.prototype.setKeyframes = function (k) { this._frames = parseKeyframes(k); touch(this); };
    KeyframeEffect.prototype.getTiming = function () {
        var t = this._timing;
        return { delay: t.delay, endDelay: t.endDelay, fill: t.fill, iterationStart: t.iterationStart,
                 iterations: t.iterations, duration: t.duration, direction: t.direction, easing: String(t.easing) };
    };
    KeyframeEffect.prototype.getComputedTiming = function () {
        var t = this._timing, a = this._animation;
        var lt = a ? a.currentTime : null;
        var c = computed(t, lt);
        var o = this.getTiming();
        o.fill = t.fill === 'auto' ? 'none' : t.fill;
        o.duration = dur(t);
        o.endTime = endTime(t);
        o.activeDuration = activeDuration(t);
        o.localTime = lt;
        o.progress = c.progress;
        o.currentIteration = c.iteration;
        return o;
    };
    KeyframeEffect.prototype.updateTiming = function (opts) {
        var merged = this.getTiming();
        Object.keys(opts || {}).forEach(function (k) { merged[k] = opts[k]; });
        this._timing = parseTiming(merged);
        if (this._animation) this._animation._updateFinished(false);
        touch(this);
    };
    // Property values at progress: { prop: value }.
    KeyframeEffect.prototype._values = function (p, underlying) {
        var frames = this._frames, out = {};
        var props = {};
        frames.forEach(function (f) { Object.keys(f.values).forEach(function (k) { props[k] = 1; }); });
        Object.keys(props).forEach(function (prop) {
            var list = frames.filter(function (f) { return prop in f.values; })
                .map(function (f) { return { offset: f.offset, value: f.values[prop], ease: f.ease }; });
            if (list[0].offset !== 0) list.unshift({ offset: 0, value: underlying(prop), ease: linear });
            if (list[list.length - 1].offset !== 1) list.push({ offset: 1, value: underlying(prop), ease: linear });
            var a, b;
            if (p < 0) { a = list[0]; b = list[1]; }
            else if (p >= 1) { a = list[list.length - 2]; b = list[list.length - 1]; if (p === 1) { out[prop] = finalValue(prop, b.value); return; } }
            else {
                for (var i = 0; i < list.length - 1; i++) {
                    if (p >= list[i].offset && p < list[i + 1].offset) { a = list[i]; b = list[i + 1]; break; }
                }
                if (!a) { a = list[list.length - 2]; b = list[list.length - 1]; }
            }
            var span = b.offset - a.offset;
            var local = span === 0 ? 0 : (p - a.offset) / span;
            out[prop] = finalValue(prop, interpolate(prop, a.value, b.value, a.ease(local)));
        });
        return out;
    };

    // ------------------------------------------------------------- timeline
    var timeNow = 0;          // the last frame's time
    var all = [];             // animations, oldest first (composite order)
    var frameQueued = false;
    var timeline = {};
    Object.defineProperty(timeline, 'currentTime', { get: function () { return timeNow; }, enumerable: true });
    function AnimationTimeline() { throw new TypeError('Illegal constructor'); }
    function DocumentTimeline() { return timeline; }
    DocumentTimeline.prototype = Object.create(AnimationTimeline.prototype);
    Object.setPrototypeOf(timeline, DocumentTimeline.prototype);

    function queueFrame() {
        if (frameQueued) return;
        frameQueued = true;
        raf.call(g, frame);
    }
    function frame(ts) {
        frameQueued = false;
        if (typeof ts === 'number' && ts > timeNow) timeNow = ts;
        var list = all.slice();
        var more = false;
        for (var i = 0; i < list.length; i++) {
            var a = list[i];
            if (a._pending === 'play') {
                a._startTime = timeNow - (a._holdTime || 0);
                a._holdTime = null;
                a._pending = null;
                a._resolveReady();
            } else if (a._pending === 'pause') {
                a._pending = null;
                a._resolveReady();
            }
            a._updateFinished(false);
            if (a._pending || a.playState === 'running') more = true;
        }
        applyAll();
        if (more) queueFrame();
    }

    // ------------------------------------------------------ inline write-back
    // One record per animated element: the style attribute the page had
    // (`base`), what we last wrote (`wrote`), and the values in it.
    var records = [];
    function recordFor(el, make) {
        for (var i = 0; i < records.length; i++) if (records[i].el === el) return records[i];
        if (!make) return null;
        var r = { el: el, base: el.getAttribute('style'), wrote: null, values: {} };
        records.push(r);
        return r;
    }
    function setAttr(el, v) {
        if (v == null) el.removeAttribute('style'); else el.setAttribute('style', v);
    }
    // The page wrote the style attribute since we did: what it now holds,
    // less our values where it left them, is the new base.
    function rebase(r) {
        var el = r.el;
        if (r.wrote == null || el.getAttribute('style') === r.wrote) return;
        var keep = {};
        var baseDecl = parseDecls(r.base);
        Object.keys(r.values).forEach(function (p) {
            if (el.style.getPropertyValue(p) === r.values[p]) keep[p] = baseDecl[p];
        });
        Object.keys(keep).forEach(function (p) {
            if (keep[p]) el.style.setProperty(p, keep[p].value, keep[p].priority);
            else el.style.removeProperty(p);
        });
        r.base = el.getAttribute('style');
        r.wrote = null;
        r.values = {};
    }
    function parseDecls(text) {
        var out = {};
        if (!text) return out;
        var el = probe(text);
        for (var i = 0; i < el.style.length; i++) {
            var p = el.style.item(i);
            out[p] = { value: el.style.getPropertyValue(p), priority: el.style.getPropertyPriority(p) };
        }
        return out;
    }
    function inEffect(a) {
        if (!a._effect || !a._effect.target) return false;
        var c = computed(a._effect._timing, a.currentTime);
        return c.progress != null;
    }
    function applyAll() {
        var touched = [];
        records.forEach(function (r) { touched.push(r.el); });
        all.forEach(function (a) {
            var t = a._effect && a._effect.target;
            if (t && touched.indexOf(t) < 0 && inEffect(a)) touched.push(t);
        });
        touched.forEach(applyTo);
    }
    var probeEl = null;
    function probe(text) {
        if (!probeEl) probeEl = document.createElement('div');
        if (text == null) probeEl.removeAttribute('style'); else probeEl.setAttribute('style', text);
        return probeEl;
    }
    function applyTo(el) {
        var mine = all.filter(function (a) { return a._effect && a._effect.target === el && inEffect(a); });
        var r = recordFor(el, mine.length > 0);
        if (!r) return;
        rebase(r);
        if (!mine.length) {
            if (r.wrote != null && el.getAttribute('style') === r.wrote) setAttr(el, r.base);
            records.splice(records.indexOf(r), 1);
            return;
        }
        // Build the new attribute off-tree: one write to the element.
        var p = probe(r.base);
        var important = {};
        for (var i = 0; i < p.style.length; i++) {
            var name = p.style.item(i);
            if (p.style.getPropertyPriority(name) === 'important') important[name] = 1;
        }
        var values = {};
        mine.forEach(function (a) {
            var e = a._effect;
            var c = computed(e._timing, a.currentTime);
            var v = e._values(c.progress, function (prop) {
                if (prop in values) return values[prop];
                var s = p.style.getPropertyValue(prop);
                if (s) return s;
                // Underlying computed value, read once, before any write of ours.
                var under = e._under || (e._under = {});
                if (!(prop in under)) {
                    try { under[prop] = r.wrote == null ? (g.getComputedStyle(el).getPropertyValue(prop) || '') : ''; }
                    catch (_) { under[prop] = ''; }
                }
                return under[prop];
            });
            Object.keys(v).forEach(function (k) { values[k] = v[k]; });
        });
        var written = {};
        Object.keys(values).forEach(function (name) {
            if (important[name] || values[name] === '') return;
            p.style.setProperty(name, values[name]);
            written[name] = p.style.getPropertyValue(name);
        });
        var text = p.getAttribute('style');
        if (el.getAttribute('style') !== text) setAttr(el, text);
        r.values = written;
        r.wrote = el.getAttribute('style');
    }
    function touch(effect) {
        if (effect && effect.target) applyTo(effect.target);
    }

    // -------------------------------------------------------------- Animation
    function Animation(effect, tl) {
        if (!(this instanceof Animation)) throw new TypeError("Failed to construct 'Animation': Please use the 'new' operator.");
        this._effect = null;
        this.effect = effect || null;
        this.timeline = tl === undefined ? timeline : tl;
        this.id = '';
        this._startTime = null;
        this._holdTime = null;
        this._pending = null;
        this._idle = true;
        this._finishedState = false;
        this._listeners = {};
        this.onfinish = null;
        this.oncancel = null;
        this.onremove = null;
        this.replaceState = 'active';
        this._newReady(true);
        this._newFinished();
        all.push(this);
    }
    Object.defineProperty(Animation.prototype, 'effect', {
        get: function () { return this._effect; },
        set: function (e) {
            var old = this._effect;
            if (old) old._animation = null;
            this._effect = e || null;
            if (e) e._animation = this;
            if (old) touch(old);
            touch(e);
        }
    });
    Animation.prototype._newReady = function (resolved) {
        var self = this;
        this._ready = new Promise_(function (res, rej) { self._readyRes = res; self._readyRej = rej; });
        this._ready.catch(function () {});
        if (resolved) this._readyRes(this);
    };
    Animation.prototype._resolveReady = function () { this._readyRes(this); };
    Animation.prototype._newFinished = function () {
        var self = this;
        this._finished = new Promise_(function (res, rej) { self._finRes = res; self._finRej = rej; });
        this._finished.catch(function () {});
        this._finishedState = false;
    };
    Object.defineProperty(Animation.prototype, 'ready', { get: function () { return this._ready; } });
    Object.defineProperty(Animation.prototype, 'finished', { get: function () { return this._finished; } });
    Object.defineProperty(Animation.prototype, 'pending', { get: function () { return this._pending != null; } });
    Object.defineProperty(Animation.prototype, 'startTime', {
        get: function () { return this._startTime; },
        set: function (v) {
            this._startTime = v == null ? null : Number(v);
            this._holdTime = null;
            this._pending = null;
            this._idle = this._startTime == null;
            if (all.indexOf(this) < 0) all.push(this);
            this._updateFinished(true);
            touch(this._effect);
            queueFrame();
        }
    });
    Object.defineProperty(Animation.prototype, 'playbackRate', {
        get: function () { return 1; },
        set: function (v) { if (Number(v) !== 1) __rk.unsupported('Animation', 'playbackRate'); }
    });
    Object.defineProperty(Animation.prototype, 'currentTime', {
        get: function () {
            if (this._idle) return null;
            if (this._holdTime != null) return this._holdTime;
            if (this._startTime == null) return null;
            return timeNow - this._startTime;
        },
        set: function (v) {
            if (v == null) throw new TypeError("Failed to set the 'currentTime' property on 'Animation': currentTime may not be changed from resolved to unresolved");
            v = Number(v);
            this._idle = false;
            if (this._holdTime != null || this._startTime == null || this._pending === 'pause') this._holdTime = v;
            else this._startTime = timeNow - v;
            if (all.indexOf(this) < 0) all.push(this);
            this._updateFinished(true);
            touch(this._effect);
        }
    });
    Object.defineProperty(Animation.prototype, 'playState', {
        get: function () {
            var ct = this.currentTime;
            if (ct == null && this._startTime == null && this._pending == null) return 'idle';
            if (this._pending === 'pause' || (this._startTime == null && this._pending !== 'play')) return 'paused';
            if (ct != null && ct >= this._end()) return 'finished';
            return 'running';
        }
    });
    Animation.prototype._end = function () { return this._effect ? endTime(this._effect._timing) : 0; };
    // Clamp at the end and settle `finished` (Web Animations "update an
    // animation's finished state", playback rate 1).
    Animation.prototype._updateFinished = function (didSeek) {
        var end = this._end();
        if (this._startTime != null && this._pending == null) {
            var ct = this.currentTime;
            if (ct != null && ct >= end) this._holdTime = didSeek ? ct : end;
            else if (didSeek && this._holdTime != null) {
                this._startTime = timeNow - this._holdTime;
                this._holdTime = null;
            }
        }
        if (this.playState === 'finished') {
            if (!this._finishedState) {
                this._finishedState = true;
                var self = this;
                this._finRes(this);
                microtask(function () {
                    if (self._finishedState) self._dispatch('finish');
                });
            }
        } else if (this._finishedState) {
            this._newFinished();
        }
    };
    Animation.prototype._dispatch = function (type) {
        var ev = Object.create(AnimationPlaybackEvent.prototype);
        ev.type = type; ev.target = this; ev.currentTarget = this; ev.bubbles = false; ev.cancelable = false;
        ev.currentTime = type === 'cancel' ? null : this.currentTime;
        ev.timelineTime = timeNow; ev.timeStamp = timeNow; ev.defaultPrevented = false; ev.isTrusted = true;
        var h = this['on' + type];
        var list = (this._listeners[type] || []).slice();
        if (typeof h === 'function') { try { h.call(this, ev); } catch (e) { report(e); } }
        for (var i = 0; i < list.length; i++) {
            try {
                var cb = list[i];
                if (typeof cb === 'function') cb.call(this, ev); else cb.handleEvent(ev);
            } catch (e) { report(e); }
        }
    };
    Animation.prototype.addEventListener = function (type, cb) {
        if (!cb) return;
        var l = this._listeners[type] || (this._listeners[type] = []);
        if (l.indexOf(cb) < 0) l.push(cb);
    };
    Animation.prototype.removeEventListener = function (type, cb) {
        var l = this._listeners[type];
        if (l) { var i = l.indexOf(cb); if (i >= 0) l.splice(i, 1); }
    };
    Animation.prototype.dispatchEvent = function (ev) {
        var list = (this._listeners[ev.type] || []).slice();
        for (var i = 0; i < list.length; i++) { try { list[i].call(this, ev); } catch (e) { report(e); } }
        return true;
    };
    Animation.prototype.play = function () {
        var ct = this.currentTime, end = this._end();
        if (this._idle || ct == null || ct < 0 || ct >= end) this._holdTime = 0;
        else if (this._pending === 'play') return;
        else if (this._startTime != null && this._pending == null) return;
        else this._holdTime = ct;
        this._idle = false;
        if (all.indexOf(this) < 0) all.push(this);
        if (this._pending == null) this._newReady(false);
        this._pending = 'play';
        this._startTime = null;
        if (this._finishedState) this._newFinished();
        touch(this._effect);
        queueFrame();
    };
    Animation.prototype.pause = function () {
        if (this._pending === 'pause' || (this._startTime == null && this._pending == null && !this._idle)) return;
        var ct = this.currentTime;
        if (this._idle || ct == null) ct = 0;
        this._idle = false;
        this._holdTime = ct;
        this._startTime = null;
        if (this._pending == null) this._newReady(false);
        this._pending = 'pause';
        if (all.indexOf(this) < 0) all.push(this);
        if (this._finishedState) this._newFinished();
        touch(this._effect);
        queueFrame();
    };
    Animation.prototype.finish = function () {
        if (this._effect && this._effect._timing.iterations === Infinity && dur(this._effect._timing) > 0) {
            throw domError('InvalidStateError', "Failed to execute 'finish' on 'Animation': Cannot finish Animation with an infinite target effect end.");
        }
        var end = this._end();
        this._idle = false;
        if (all.indexOf(this) < 0) all.push(this);
        if (this._holdTime != null || this._startTime == null) this._holdTime = end;
        else this._startTime = timeNow - end;
        if (this._startTime == null) this._startTime = timeNow - end;
        if (this._pending === 'pause') this._holdTime = null;
        if (this._pending) { this._pending = null; this._resolveReady(); }
        this._updateFinished(true);
        touch(this._effect);
    };
    Animation.prototype.cancel = function () {
        if (this.playState !== 'idle') {
            if (this._pending) { this._readyRej(domError('AbortError', 'The user aborted a request.')); this._newReady(true); }
            if (!this._finishedState) this._finRej(domError('AbortError', 'The user aborted a request.'));
            this._newFinished();
            this._pending = null;
            this._startTime = null;
            this._holdTime = null;
            this._idle = true;
            var self = this;
            microtask(function () { self._dispatch('cancel'); });
        }
        touch(this._effect);
    };
    Animation.prototype.reverse = function () { __rk.unsupported('Animation', 'reverse'); };
    Animation.prototype.updatePlaybackRate = function (r) { if (Number(r) !== 1) __rk.unsupported('Animation', 'updatePlaybackRate'); };
    Animation.prototype.persist = function () { this.replaceState = 'persisted'; };
    Animation.prototype.commitStyles = function () {
        var e = this._effect;
        if (!e || !e.target) return;
        var el = e.target;
        var c = computed(e._timing, this.currentTime);
        if (c.progress == null) return;
        var r = recordFor(el, false);
        if (r) rebase(r);
        var vals = e._values(c.progress, function (p) { return g.getComputedStyle(el).getPropertyValue(p); });
        // Into the base, so it stays once the animation is gone.
        var pe = probe(r ? r.base : el.getAttribute('style'));
        Object.keys(vals).forEach(function (p) { pe.style.setProperty(p, vals[p]); });
        if (r) { r.base = pe.getAttribute('style'); applyTo(el); }
        else setAttr(el, pe.getAttribute('style'));
    };
    Animation.prototype._relevant = function () {
        var s = this.playState;
        if (s === 'idle') return false;
        if (s !== 'finished') return true;
        return inEffect(this);
    };

    function AnimationPlaybackEvent(type, init) {
        this.type = String(type);
        this.currentTime = init && init.currentTime != null ? init.currentTime : null;
        this.timelineTime = init && init.timelineTime != null ? init.timelineTime : null;
    }
    if (g.Event) AnimationPlaybackEvent.prototype = Object.create(g.Event.prototype);
    AnimationPlaybackEvent.prototype.constructor = AnimationPlaybackEvent;
    AnimationPlaybackEvent.prototype.preventDefault = function () {};
    AnimationPlaybackEvent.prototype.stopPropagation = function () {};
    AnimationPlaybackEvent.prototype.stopImmediatePropagation = function () {};

    function animationsOf(filter) {
        // Drop idle animations nobody can reach through getAnimations.
        return all.filter(function (a) { return a._relevant() && filter(a); });
    }

    g.Element.prototype.animate = function (keyframes, options) {
        var effect = new KeyframeEffect(this, keyframes, options);
        var a = new Animation(effect, timeline);
        if (options && typeof options === 'object' && options.id != null) a.id = String(options.id);
        a.play();
        return a;
    };
    g.Element.prototype.getAnimations = function (opts) {
        var el = this;
        var sub = opts && opts.subtree;
        return animationsOf(function (a) {
            var t = a._effect && a._effect.target;
            return !!t && (t === el || (sub && el.contains && el.contains(t)));
        });
    };
    var getDocAnimations = function () { return animationsOf(function (a) { return !!(a._effect && a._effect.target); }); };
    if (g.Document && g.Document.prototype) g.Document.prototype.getAnimations = getDocAnimations;
    document.getAnimations = getDocAnimations;
    Object.defineProperty(document, 'timeline', { get: function () { return timeline; }, configurable: true });

    g.Animation = Animation;
    g.KeyframeEffect = KeyframeEffect;
    g.AnimationEffect = KeyframeEffect;
    g.AnimationTimeline = AnimationTimeline;
    g.DocumentTimeline = DocumentTimeline;
    g.AnimationPlaybackEvent = AnimationPlaybackEvent;

    // Idle animations fall out of the list so a page that animates on every
    // hover does not keep them all.
    var sweep = Animation.prototype.cancel;
    Animation.prototype.cancel = function () {
        sweep.call(this);
        var i = all.indexOf(this);
        if (i >= 0) all.splice(i, 1);
    };
    var settle = Animation.prototype._updateFinished;
    Animation.prototype._updateFinished = function (byClock) {
        settle.call(this, byClock);
        if (this._finishedState && !inEffect(this)) {
            var self = this;
            // After the finish event's microtask, so it still sees itself.
            microtask(function () {
                if (self._finishedState && !inEffect(self)) {
                    var i = all.indexOf(self);
                    if (i >= 0) all.splice(i, 1);
                }
            });
        }
    };
})(typeof window !== 'undefined' ? window : this);
