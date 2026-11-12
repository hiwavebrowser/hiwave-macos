// SVGGraphicsElement.getBBox and SVGGeometryElement.getTotalLength /
// getPointAtLength (SVG 2 §4.6.3, §9.9; STR-8). The numbers are
// svg_geometry.rs's; this layer reads a shape's attributes, and for a
// container (`g`, `svg`) unions its children's boxes, each through the
// child's own `transform`.
//
// Not here yet: `text` (its box is the glyphs'), `use`, `image`, nested
// `svg` viewports, percentages and units in lengths, and `pathLength`.
(function (g) {
    var geometry = g.__rustkit_svg_geometry;
    delete g.__rustkit_svg_geometry;
    var Graphics = g.SVGGraphicsElement, Geometry = g.SVGGeometryElement;
    if (typeof geometry !== 'function' || !Graphics || !Geometry) return;
    var SVG_NS = 'http://www.w3.org/2000/svg';

    function illegal() { throw new TypeError('Illegal constructor'); }
    function valueType(name, fields) {
        var ctor = function () { illegal(); };
        Object.defineProperty(ctor, 'name', { value: name });
        Object.defineProperty(ctor.prototype, Symbol.toStringTag, { value: name });
        if (g[name] === undefined) Object.defineProperty(g, name, { value: ctor, writable: true, configurable: true });
        return function (values) {
            var o = Object.create(ctor.prototype);
            fields.forEach(function (f, i) { o[f] = values[i]; });
            return o;
        };
    }
    var rect = valueType('SVGRect', ['x', 'y', 'width', 'height']);
    var point = valueType('SVGPoint', ['x', 'y']);

    // An absent or unreadable attribute is written `x`: the default.
    function numbers(el, names) {
        return names.map(function (name) {
            var n = parseFloat(el.getAttribute(name));
            return isFinite(n) ? String(n) : 'x';
        }).join(' ');
    }
    var SPEC = {
        path: function (el) { return el.getAttribute('d') || ''; },
        polyline: function (el) { return el.getAttribute('points') || ''; },
        polygon: function (el) { return el.getAttribute('points') || ''; },
        rect: function (el) { return numbers(el, ['x', 'y', 'width', 'height', 'rx', 'ry']); },
        circle: function (el) { return numbers(el, ['cx', 'cy', 'r']); },
        ellipse: function (el) { return numbers(el, ['cx', 'cy', 'rx', 'ry']); },
        line: function (el) { return numbers(el, ['x1', 'y1', 'x2', 'y2']); }
    };
    function measure(el, at) {
        var kind = el.localName, spec = SPEC[kind];
        if (!spec) return null;
        return JSON.parse(geometry(kind, spec(el), at === undefined ? 0 : at));
    }

    // A `transform` attribute as [a, b, c, d, e, f].
    function multiply(m, n) {
        return [m[0] * n[0] + m[2] * n[1], m[1] * n[0] + m[3] * n[1],
                m[0] * n[2] + m[2] * n[3], m[1] * n[2] + m[3] * n[3],
                m[0] * n[4] + m[2] * n[5] + m[4], m[1] * n[4] + m[3] * n[5] + m[5]];
    }
    function transformOf(el) {
        var text = el.getAttribute('transform'), m = [1, 0, 0, 1, 0, 0];
        if (!text) return null;
        var re = /(matrix|translate|scale|rotate|skewX|skewY)\s*\(([^)]*)\)/g, hit;
        while ((hit = re.exec(text))) {
            var a = hit[2].split(/[\s,]+/).filter(function (s) { return s !== ''; }).map(Number);
            if (a.some(function (n) { return !isFinite(n); })) return null;
            var t = null, r;
            switch (hit[1]) {
                case 'matrix': if (a.length === 6) t = a; break;
                case 'translate': if (a.length >= 1) t = [1, 0, 0, 1, a[0], a.length > 1 ? a[1] : 0]; break;
                case 'scale': if (a.length >= 1) t = [a[0], 0, 0, a.length > 1 ? a[1] : a[0], 0, 0]; break;
                case 'rotate':
                    if (a.length >= 1) {
                        r = a[0] * Math.PI / 180;
                        t = [Math.cos(r), Math.sin(r), -Math.sin(r), Math.cos(r), 0, 0];
                        if (a.length >= 3) t = multiply(multiply([1, 0, 0, 1, a[1], a[2]], t), [1, 0, 0, 1, -a[1], -a[2]]);
                    }
                    break;
                case 'skewX': if (a.length === 1) t = [1, 0, Math.tan(a[0] * Math.PI / 180), 1, 0, 0]; break;
                case 'skewY': if (a.length === 1) t = [1, Math.tan(a[0] * Math.PI / 180), 0, 1, 0, 0]; break;
            }
            if (!t) return null;
            m = multiply(m, t);
        }
        return m;
    }

    // [x, y, width, height] in the element's own user space, or null when
    // it has nothing to measure.
    function boxOf(el) {
        var own = measure(el);
        if (own) return (el.localName === 'path' || el.localName === 'polyline' || el.localName === 'polygon') &&
            own[2] === 0 && own[3] === 0 && own[4] === 0 ? null : own.slice(0, 4);
        var x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
        var kids = el.children;
        for (var i = 0; i < kids.length; i++) {
            var kid = kids[i];
            if (kid.namespaceURI !== SVG_NS || !(kid instanceof Graphics) || kid.localName === 'defs') continue;
            var b = boxOf(kid);
            if (!b) continue;
            var m = transformOf(kid);
            var corners = [[b[0], b[1]], [b[0] + b[2], b[1]], [b[0], b[1] + b[3]], [b[0] + b[2], b[1] + b[3]]];
            corners.forEach(function (c) {
                var x = m ? m[0] * c[0] + m[2] * c[1] + m[4] : c[0];
                var y = m ? m[1] * c[0] + m[3] * c[1] + m[5] : c[1];
                x0 = Math.min(x0, x); y0 = Math.min(y0, y); x1 = Math.max(x1, x); y1 = Math.max(y1, y);
            });
        }
        return x0 === Infinity ? null : [x0, y0, x1 - x0, y1 - y0];
    }

    function method(proto, name, fn) {
        Object.defineProperty(proto, name, { value: fn, writable: true, configurable: true, enumerable: true });
    }
    function own(self, C) {
        if (!(self instanceof C)) throw new TypeError('Illegal invocation');
        return self;
    }
    method(Graphics.prototype, 'getBBox', function getBBox() {
        return rect(boxOf(own(this, Graphics)) || [0, 0, 0, 0]);
    });
    method(Geometry.prototype, 'getTotalLength', function getTotalLength() {
        var m = measure(own(this, Geometry));
        return m ? m[4] : 0;
    });
    method(Geometry.prototype, 'getPointAtLength', function getPointAtLength(distance) {
        own(this, Geometry);
        if (arguments.length < 1) {
            throw new TypeError("Failed to execute 'getPointAtLength' on 'SVGGeometryElement': 1 argument required, but only 0 present.");
        }
        var at = Number(distance);
        if (!isFinite(at)) {
            throw new TypeError("Failed to execute 'getPointAtLength' on 'SVGGeometryElement': The provided float value is non-finite.");
        }
        var m = measure(this, at);
        return point(m ? [m[5], m[6]] : [0, 0]);
    });
})(globalThis);
