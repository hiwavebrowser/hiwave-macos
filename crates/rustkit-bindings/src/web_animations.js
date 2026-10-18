// Web Animations (element.animate, Animation, document.timeline).
//
// Keyframes are sampled on the same virtual clock as requestAnimationFrame
// and written onto the element's inline style, which getComputedStyle
// reports. opacity and transform are interpolated; other properties are
// interpolated when both values are numbers (with an optional unit) and
// otherwise switch at the halfway point.
//
// Limits: no composite modes, no pseudo-elements, no ScrollTimeline, and
// easing is linear plus the four CSS keywords via their cubic beziers.
(function (g) {
    if (!g.Element || typeof g.requestAnimationFrame !== 'function') return;
    var raf = g.requestAnimationFrame.bind(g);
    var clock = 0;
    var animations = [];
    var looping = false;

    function illegal() { throw new TypeError('Illegal constructor'); }
    function define(obj, name, desc) {
        desc.configurable = true;
        if (desc.enumerable === undefined) desc.enumerable = true;
        Object.defineProperty(obj, name, desc);
    }
    function iface(name) {
        if (g[name]) return g[name];
        var ctor = function () { illegal(); };
        Object.defineProperty(ctor, 'name', { value: name });
        Object.defineProperty(ctor.prototype, Symbol.toStringTag, { value: name, configurable: true });
        Object.defineProperty(g, name, { value: ctor, writable: true, configurable: true, enumerable: false });
        return ctor;
    }

    function makeTarget(obj) {
        var listeners = {};
        obj.addEventListener = function (type, cb) {
            if (typeof cb !== 'function' && !(cb && typeof cb.handleEvent === 'function')) return;
            var list = listeners[type] || (listeners[type] = []);
            if (list.indexOf(cb) < 0) list.push(cb);
        };
        obj.removeEventListener = function (type, cb) {
            var list = listeners[type];
            if (!list) return;
            var i = list.indexOf(cb);
            if (i >= 0) list.splice(i, 1);
        };
        obj.dispatchEvent = function (event) {
            if (!event) return true;
            event.target = obj;
            event.currentTarget = obj;
            var list = (listeners[event.type] || []).slice();
            var handler = obj['on' + event.type];
            if (typeof handler === 'function') list.push(handler);
            for (var i = 0; i < list.length; i++) {
                try {
                    var cb = list[i];
                    if (typeof cb === 'function') cb.call(obj, event);
                    else cb.handleEvent(event);
                } catch (e) {}
            }
            return !event.defaultPrevented;
        };
    }

    function Deferred() {
        var self = this;
        this.promise = new Promise(function (resolve, reject) {
            self.resolve = resolve;
            self.reject = reject;
        });
        this.settled = false;
    }
    Deferred.prototype.finish = function (ok, value) {
        if (this.settled) return;
        this.settled = true;
        if (ok) this.resolve(value);
        else this.reject(value);
    };

    function num(value) {
        if (typeof value === 'number' && isFinite(value)) return { n: value, unit: '' };
        if (typeof value !== 'string') return null;
        var m = /^(-?(?:\d+\.?\d*|\.\d+))([a-z%]*)$/i.exec(value.trim());
        if (!m) return null;
        return { n: Number(m[1]), unit: m[2] || '' };
    }

    function splitFns(value) {
        if (value == null) return null;
        var text = String(value).trim();
        if (text === '' || text === 'none') return [];
        var out = [];
        var re = /([A-Za-z][\w-]*)\(([^)]*)\)/g;
        var m, last = 0;
        while ((m = re.exec(text))) {
            if (m.index !== last && text.slice(last, m.index).trim() !== '') return null;
            out.push({
                name: m[1],
                args: m[2].split(',').map(function (part) { return part.trim(); })
            });
            last = re.lastIndex;
        }
        if (!out.length || text.slice(last).trim() !== '') return null;
        return out;
    }

    function interpolate(from, to, t) {
        if (t <= 0) return from == null ? '' : String(from);
        if (t >= 1) return to == null ? '' : String(to);
        var a = num(from), b = num(to);
        if (a && b && a.unit === b.unit) return String(a.n + (b.n - a.n) * t) + a.unit;
        var fa = splitFns(from), fb = splitFns(to);
        if (fa && fb && fa.length === fb.length) {
            var parts = [];
            for (var i = 0; i < fa.length; i++) {
                if (fa[i].name !== fb[i].name || fa[i].args.length !== fb[i].args.length) {
                    parts = null;
                    break;
                }
                var args = [];
                for (var k = 0; k < fa[i].args.length; k++) {
                    args.push(interpolate(fa[i].args[k], fb[i].args[k], t));
                }
                parts.push(fa[i].name + '(' + args.join(', ') + ')');
            }
            if (parts) return parts.join(' ');
        }
        return t < 0.5 ? (from == null ? '' : String(from)) : (to == null ? '' : String(to));
    }

    // Cubic-bezier y at x, Newton on x then evaluate y. Enough for the CSS keywords.
    function bezier(x1, y1, x2, y2, x) {
        if (x <= 0) return 0;
        if (x >= 1) return 1;
        function cx(t) {
            var u = 1 - t;
            return 3 * u * u * t * x1 + 3 * u * t * t * x2 + t * t * t;
        }
        function cy(t) {
            var u = 1 - t;
            return 3 * u * u * t * y1 + 3 * u * t * t * y2 + t * t * t;
        }
        function dx(t) {
            var u = 1 - t;
            return 3 * u * u * x1 + 6 * u * t * (x2 - x1) + 3 * t * t * (1 - x2);
        }
        var t = x;
        for (var i = 0; i < 8; i++) {
            var slope = dx(t);
            if (Math.abs(slope) < 1e-6) break;
            t = t - (cx(t) - x) / slope;
            if (t < 0) t = 0;
            if (t > 1) t = 1;
        }
        return cy(t);
    }

    function ease(name, t) {
        if (name === 'ease') return bezier(0.25, 0.1, 0.25, 1, t);
        if (name === 'ease-in') return bezier(0.42, 0, 1, 1, t);
        if (name === 'ease-out') return bezier(0, 0, 0.58, 1, t);
        if (name === 'ease-in-out') return bezier(0.42, 0, 0.58, 1, t);
        return t;
    }

    function expandKeyframes(input) {
        var frames = [];
        if (Array.isArray(input)) {
            input.forEach(function (kf) {
                kf = kf || {};
                var props = {};
                Object.keys(kf).forEach(function (key) {
                    if (key === 'offset' || key === 'easing' || key === 'composite') return;
                    props[key] = kf[key];
                });
                frames.push({ offset: kf.offset, easing: kf.easing, props: props });
            });
        } else if (input && typeof input === 'object') {
            var keys = Object.keys(input).filter(function (key) {
                return key !== 'offset' && key !== 'easing' && key !== 'composite';
            });
            var n = 1;
            keys.forEach(function (key) {
                if (Array.isArray(input[key]) && input[key].length > n) n = input[key].length;
            });
            var offsets = Array.isArray(input.offset) ? input.offset : null;
            for (var i = 0; i < n; i++) {
                var props = {};
                keys.forEach(function (key) {
                    var value = input[key];
                    props[key] = Array.isArray(value) ? value[Math.min(i, value.length - 1)] : value;
                });
                frames.push({ offset: offsets ? offsets[i] : undefined, props: props });
            }
        }
        if (!frames.length) frames = [{ offset: 0, props: {} }, { offset: 1, props: {} }];
        frames[0].offset = frames[0].offset == null ? 0 : Number(frames[0].offset);
        frames[frames.length - 1].offset = frames[frames.length - 1].offset == null ? 1 : Number(frames[frames.length - 1].offset);
        for (var i = 0; i < frames.length; i++) {
            if (frames[i].offset != null) continue;
            var j = i;
            while (j < frames.length && frames[j].offset == null) j++;
            var start = Number(frames[i - 1].offset);
            var end = j < frames.length ? Number(frames[j].offset) : 1;
            var span = j - (i - 1);
            for (var k = i; k < j; k++) frames[k].offset = start + (end - start) * ((k - (i - 1)) / span);
            i = j;
        }
        frames.forEach(function (frame) { frame.offset = Number(frame.offset); });
        frames.sort(function (a, b) { return a.offset - b.offset; });
        return frames;
    }

    function readTiming(options) {
        if (typeof options === 'number') options = { duration: options };
        options = options || {};
        var iterations = options.iterations == null ? 1 : Number(options.iterations);
        if (iterations < 0) iterations = 0;
        return {
            duration: Math.max(0, Number(options.duration) || 0),
            delay: Number(options.delay) || 0,
            endDelay: Number(options.endDelay) || 0,
            iterations: iterations,
            direction: options.direction || 'normal',
            fill: options.fill || 'none',
            easing: options.easing || 'linear'
        };
    }

    function activeEnd(timing) {
        if (timing.iterations === Infinity) return Infinity;
        return timing.delay + timing.duration * timing.iterations + timing.endDelay;
    }

    function iterationProgress(timing, local) {
        var dur = timing.duration;
        var into = local - timing.delay;
        if (dur === 0) return { p: 1, atEnd: into >= 0 };
        var index = Math.floor(into / dur);
        var frac = (into % dur) / dur;
        if (timing.iterations !== Infinity && index >= timing.iterations) {
            return { p: 1, atEnd: true };
        }
        var dir = timing.direction;
        var reverse = dir === 'reverse' || (dir === 'alternate' && index % 2 === 1) ||
            (dir === 'alternate-reverse' && index % 2 === 0);
        var p = reverse ? 1 - frac : frac;
        return { p: ease(timing.easing, p), atEnd: false };
    }

    function inEffect(anim) {
        var t = anim.currentTime;
        if (t == null || anim.playState === 'idle') return false;
        var timing = anim._timing;
        var end = activeEnd(timing);
        if (t < timing.delay) return timing.fill === 'backwards' || timing.fill === 'both';
        if (end !== Infinity && t >= timing.delay + (timing.iterations === Infinity ? Infinity : timing.duration * timing.iterations)) {
            return timing.fill === 'forwards' || timing.fill === 'both';
        }
        return t >= timing.delay && (end === Infinity || t < end);
    }

    function sample(anim) {
        var frames = anim._frames;
        var timing = anim._timing;
        var t = anim.currentTime;
        if (!frames.length || t == null) return;
        var endActive = timing.delay + (timing.iterations === Infinity ? Infinity : timing.duration * timing.iterations);
        var p;
        if (t < timing.delay) p = (timing.fill === 'backwards' || timing.fill === 'both') ? 0 : null;
        else if (endActive !== Infinity && t >= endActive) p = (timing.fill === 'forwards' || timing.fill === 'both' || anim.playState === 'finished') ? 1 : null;
        else p = iterationProgress(timing, t).p;
        if (p == null) {
            restore(anim);
            return;
        }
        if (anim.playState === 'finished' && (timing.fill === 'forwards' || timing.fill === 'both')) p = 1;
        var prev = frames[0], next = frames[frames.length - 1];
        for (var i = 0; i < frames.length - 1; i++) {
            if (p >= frames[i].offset && p <= frames[i + 1].offset) {
                prev = frames[i];
                next = frames[i + 1];
                break;
            }
        }
        var span = next.offset - prev.offset;
        var local = span === 0 ? 1 : (p - prev.offset) / span;
        if (next.easing) local = ease(next.easing, local);
        var props = {};
        Object.keys(prev.props).forEach(function (key) { props[key] = true; });
        Object.keys(next.props).forEach(function (key) { props[key] = true; });
        var style = anim.effect.target.style;
        Object.keys(props).forEach(function (key) {
            var name = key.replace(/[A-Z]/g, function (ch) { return '-' + ch.toLowerCase(); });
            if (!Object.prototype.hasOwnProperty.call(anim._base, name)) {
                anim._base[name] = style.getPropertyValue(name);
            }
            style.setProperty(name, interpolate(
                prev.props[key] == null ? anim._base[name] : prev.props[key],
                next.props[key] == null ? anim._base[name] : next.props[key],
                local
            ));
        });
        anim._applied = true;
    }

    function restore(anim) {
        if (!anim._applied) return;
        var style = anim.effect.target && anim.effect.target.style;
        if (!style) return;
        Object.keys(anim._base).forEach(function (name) {
            if (anim._base[name]) style.setProperty(name, anim._base[name]);
            else style.removeProperty(name);
        });
        anim._applied = false;
    }

    function fire(anim, type) {
        var event = { type: type, bubbles: false, cancelable: false, defaultPrevented: false, target: anim };
        event.preventDefault = function () { this.defaultPrevented = true; };
        anim.dispatchEvent(event);
    }

    function settleFinished(anim, ok) {
        if (ok) anim._finished.finish(true, anim);
        else anim._finished.finish(false, new DOMException('The user aborted a request.', 'AbortError'));
    }

    function reachEnd(anim) {
        var timing = anim._timing;
        var end = activeEnd(timing);
        if (end === Infinity) return;
        if (anim.playbackRate >= 0) anim.currentTime = end;
        else anim.currentTime = 0;
        anim.playState = 'finished';
        sample(anim);
        if (timing.fill !== 'forwards' && timing.fill !== 'both') restore(anim);
        if (!anim._finishFired) {
            anim._finishFired = true;
            settleFinished(anim, true);
            fire(anim, 'finish');
        }
    }

    function tick(now) {
        clock = now;
        for (var i = 0; i < animations.length; i++) {
            var anim = animations[i];
            if (anim.playState !== 'running') continue;
            var dt = now - anim._lastTick;
            anim._lastTick = now;
            if (dt < 0) dt = 0;
            anim.currentTime += dt * anim.playbackRate;
            var timing = anim._timing;
            var end = activeEnd(timing);
            if (anim.playbackRate >= 0 && end !== Infinity && anim.currentTime >= end) reachEnd(anim);
            else if (anim.playbackRate < 0 && anim.currentTime <= 0) {
                anim.currentTime = 0;
                reachEnd(anim);
            } else sample(anim);
        }
    }

    function ensureLoop() {
        if (looping) return;
        looping = true;
        raf(function frame(now) {
            tick(now);
            if (animations.some(function (anim) { return anim.playState === 'running'; })) raf(frame);
            else looping = false;
        });
    }

    var DocumentTimeline = iface('DocumentTimeline');
    var timeline = Object.create(DocumentTimeline.prototype);
    define(timeline, 'currentTime', { get: function () { return clock; } });
    define(g.document, 'timeline', { get: function () { return timeline; } });

    var KeyframeEffect = iface('KeyframeEffect');
    function Effect(target, frames, timing) {
        this.target = target;
        this._frames = frames;
        this.pseudoElement = null;
        this.composite = 'replace';
    }
    Effect.prototype = KeyframeEffect.prototype;
    Effect.prototype.getKeyframes = function () {
        return this._frames.map(function (frame) {
            var copy = { offset: frame.offset };
            Object.keys(frame.props).forEach(function (key) { copy[key] = frame.props[key]; });
            if (frame.easing) copy.easing = frame.easing;
            return copy;
        });
    };
    Effect.prototype.getComputedTiming = function () {
        var anim = this._anim;
        var timing = anim ? anim._timing : readTiming(0);
        var end = activeEnd(timing);
        var progress = null;
        if (anim && anim.currentTime != null && anim.playState !== 'idle') {
            var sampled = iterationProgress(timing, Math.min(anim.currentTime, end === Infinity ? anim.currentTime : end));
            progress = sampled.p;
        }
        return {
            delay: timing.delay,
            endDelay: timing.endDelay,
            fill: timing.fill,
            iterationStart: 0,
            iterations: timing.iterations,
            duration: timing.duration,
            direction: timing.direction,
            easing: timing.easing,
            endTime: end === Infinity ? Infinity : end,
            activeDuration: timing.iterations === Infinity ? Infinity : timing.duration * timing.iterations,
            localTime: anim ? anim.currentTime : null,
            progress: progress,
            currentIteration: null
        };
    };

    var Animation = iface('Animation');
    function Anim(effect) {
        makeTarget(this);
        this.effect = effect;
        effect._anim = this;
        this.timeline = timeline;
        this.playbackRate = 1;
        this.playState = 'idle';
        this.currentTime = null;
        this.pending = false;
        this.id = '';
        this.onfinish = null;
        this.oncancel = null;
        this.onremove = null;
        this._timing = readTiming(0);
        this._frames = effect._frames;
        this._base = {};
        this._applied = false;
        this._lastTick = clock;
        this._finishFired = false;
        this._finished = new Deferred();
        this._ready = new Deferred();
        var self = this;
        define(this, 'finished', { get: function () { return self._finished.promise; } });
        define(this, 'ready', { get: function () { return self._ready.promise; } });
        define(this, 'replaceState', { get: function () { return 'active'; } });
    }
    Anim.prototype = Animation.prototype;
    Animation.prototype.play = function () {
        if (this.playState === 'finished') {
            this.currentTime = this.playbackRate < 0 ? activeEnd(this._timing) : 0;
            this._finishFired = false;
            this._finished = new Deferred();
        }
        if (this.currentTime == null) this.currentTime = this.playbackRate < 0 ? activeEnd(this._timing) : 0;
        if (this.currentTime === Infinity) this.currentTime = 0;
        this.playState = 'running';
        this._lastTick = clock;
        this._ready.finish(true, this);
        sample(this);
        var end = activeEnd(this._timing);
        if (end !== Infinity && ((this.playbackRate >= 0 && this.currentTime >= end) || (this.playbackRate < 0 && this.currentTime <= 0))) {
            reachEnd(this);
            return;
        }
        ensureLoop();
    };
    Animation.prototype.pause = function () {
        if (this.playState === 'running') tick(clock);
        this.playState = 'paused';
        this._ready.finish(true, this);
    };
    Animation.prototype.cancel = function () {
        if (this.playState === 'idle') return;
        restore(this);
        this.playState = 'idle';
        this.currentTime = null;
        this._finishFired = false;
        settleFinished(this, false);
        this._finished = new Deferred();
        fire(this, 'cancel');
    };
    Animation.prototype.finish = function () {
        if (this.playbackRate === 0) throw new DOMException('playbackRate is zero', 'InvalidStateError');
        if (activeEnd(this._timing) === Infinity) throw new DOMException('animation has infinite iterations', 'InvalidStateError');
        if (this.currentTime == null) this.currentTime = 0;
        this.playState = 'running';
        reachEnd(this);
    };
    Animation.prototype.reverse = function () {
        if (this.playbackRate === 0) throw new DOMException('playbackRate is zero', 'InvalidStateError');
        this.playbackRate = -this.playbackRate;
        if (this.playState === 'idle' || this.playState === 'finished') {
            this._finishFired = false;
            this._finished = new Deferred();
        }
        this.play();
    };
    Animation.prototype.updatePlaybackRate = function (rate) {
        this.playbackRate = Number(rate) || 0;
    };
    Animation.prototype.persist = function () {};
    Animation.prototype.commitStyles = function () {};

    function listFor(target) {
        return animations.filter(function (anim) {
            return anim.playState !== 'idle' && (!target || anim.effect.target === target);
        });
    }

    g.Element.prototype.animate = function animate(keyframes, options) {
        if (this == null || this.nodeType !== 1) {
            throw new TypeError("Failed to execute 'animate' on 'Element': 'this' is not an Element.");
        }
        var timing = readTiming(options);
        var effect = new Effect(this, expandKeyframes(keyframes), timing);
        var anim = new Anim(effect);
        anim._timing = timing;
        anim._frames = effect._frames;
        animations.push(anim);
        anim.play();
        return anim;
    };
    g.Element.prototype.getAnimations = function getAnimations() { return listFor(this); };
    if (g.Document && g.Document.prototype) {
        g.Document.prototype.getAnimations = function getAnimations() { return listFor(null); };
    }

    // An element id (and a named form/image/iframe) is a property of window,
    // so a bare `videoElement` resolves after the document is parsed.
    if (!g.__rkNamedWindow) {
        g.__rkNamedWindow = true;
        var proto = Object.getPrototypeOf(g);
        var bridge = Object.create(proto);
        function namedElement(prop) {
            var doc = g.document;
            if (!doc || typeof prop !== 'string' || !prop) return null;
            var el = typeof doc.getElementById === 'function' ? doc.getElementById(prop) : null;
            if (el) return el;
            if (typeof doc.getElementsByName === 'function') {
                var named = doc.getElementsByName(prop);
                if (named && named.length) return named[0];
            }
            return null;
        }
        var proxy = new Proxy(bridge, {
            get: function (target, prop, receiver) {
                if (typeof prop !== 'string' || prop in target) return Reflect.get(target, prop, receiver);
                var el = namedElement(prop);
                return el || Reflect.get(target, prop, receiver);
            },
            has: function (target, prop) {
                if (prop in target) return true;
                return !!namedElement(prop);
            }
        });
        Object.setPrototypeOf(g, proxy);
    }
})(typeof globalThis === 'object' ? globalThis : this);
