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
            var m = localMatrix(kid);
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

    // ---- SVG2: where an SVG element is on the page, for script.
    //
    // An SVG shape has no layout box of its own here (the `<svg>` is one
    // replaced box), so its geometry for script is built from the same
    // attributes: its box in its own user space (`boxOf`), carried to the
    // page through every transform between it and the outermost `<svg>`
    // (`transform` attributes, each `<svg>`'s viewBox, a nested `<svg>`'s
    // x and y) and that `<svg>`'s own client rect.
    //
    // Not here: `transform` set from CSS, CSS `fill` / `stroke` /
    // `pointer-events` (the attributes are read, inherited through the
    // ancestors), `text`, `use`, clip paths and masks in hit testing.
    var IDENTITY = [1, 0, 0, 1, 0, 0];
    var elementRect = g.Element.prototype.getBoundingClientRect;
    function isSvg(n) { return !!n && n.nodeType === 1 && n.namespaceURI === SVG_NS; }
    function ownerSvg(el) {
        for (var n = el.parentNode; isSvg(n); n = n.parentNode) if (n.localName === 'svg') return n;
        return null;
    }
    function number(el, name, fallback) {
        var v = parseFloat(el.getAttribute(name));
        return isFinite(v) ? v : fallback;
    }
    // The size of the viewport an `<svg>` establishes: the outermost one's
    // box on the page, a nested one's `width` and `height`.
    function viewportSize(svg) {
        var outer = ownerSvg(svg);
        if (!outer) { var r = elementRect.call(svg); return [r.width, r.height]; }
        var parent = viewportSize(outer);
        return [number(svg, 'width', parent[0]), number(svg, 'height', parent[1])];
    }
    // viewBox and preserveAspectRatio as a matrix (SVG 2 §8.2).
    function viewBoxMatrix(svg) {
        var vb = (svg.getAttribute('viewBox') || '').split(/[\s,]+/).filter(function (s) { return s !== ''; }).map(Number);
        if (vb.length !== 4 || !(vb[2] > 0) || !(vb[3] > 0)) return IDENTITY;
        var size = viewportSize(svg), w = size[0], h = size[1];
        if (!(w > 0) || !(h > 0)) return IDENTITY;
        var par = (svg.getAttribute('preserveAspectRatio') || 'xMidYMid meet').trim().split(/\s+/);
        var sx = w / vb[2], sy = h / vb[3], align = par[0];
        if (align === 'none') return [sx, 0, 0, sy, -vb[0] * sx, -vb[1] * sy];
        var s = par[1] === 'slice' ? Math.max(sx, sy) : Math.min(sx, sy);
        var tx = /xMid/.test(align) ? (w - vb[2] * s) / 2 : /xMax/.test(align) ? w - vb[2] * s : 0;
        var ty = /YMid/.test(align) ? (h - vb[3] * s) / 2 : /YMax/.test(align) ? h - vb[3] * s : 0;
        return [s, 0, 0, s, tx - vb[0] * s, ty - vb[1] * s];
    }
    // What an element does to the coordinates of what is in it.
    function localMatrix(el) {
        if (el.localName === 'svg') {
            var m = viewBoxMatrix(el);
            return ownerSvg(el) ? multiply([1, 0, 0, 1, number(el, 'x', 0), number(el, 'y', 0)], m) : m;
        }
        return transformOf(el) || IDENTITY;
    }
    // SVG 2 §4.6.3 getCTM: from the element's user space to its nearest
    // viewport element's (an outermost `<svg>`: its own viewBox).
    function ctm(el) {
        var chain = [el];
        if (el.localName !== 'svg' || ownerSvg(el)) {
            for (var n = el.parentNode; isSvg(n); n = n.parentNode) {
                chain.push(n);
                if (n.localName === 'svg') break;
            }
        }
        var m = IDENTITY;
        for (var i = chain.length - 1; i >= 0; i--) m = multiply(m, localMatrix(chain[i]));
        return m;
    }
    // getScreenCTM: to client coordinates. Null outside an `<svg>`.
    function screenCtm(el) {
        var chain = [];
        for (var n = el; isSvg(n); n = n.parentNode) chain.push(n);
        var root = chain[chain.length - 1];
        if (!root || root.localName !== 'svg') return null;
        var r = elementRect.call(root);
        var m = [1, 0, 0, 1, r.left + (root.clientLeft || 0), r.top + (root.clientTop || 0)];
        for (var i = chain.length - 1; i >= 0; i--) m = multiply(m, localMatrix(chain[i]));
        return m;
    }
    function invert(m) {
        var det = m[0] * m[3] - m[1] * m[2];
        if (!det || !isFinite(det)) return null;
        return [m[3] / det, -m[1] / det, -m[2] / det, m[0] / det,
                (m[2] * m[5] - m[3] * m[4]) / det, (m[1] * m[4] - m[0] * m[5]) / det];
    }

    // SVGMatrix (SVG 1.1 §7.11; Chrome for Testing 148 still answers one
    // from getCTM, not a DOMMatrix): six numbers and the operations d3 and
    // charting code call on them.
    function SVGMatrix() { illegal(); }
    Object.defineProperty(SVGMatrix.prototype, Symbol.toStringTag, { value: 'SVGMatrix' });
    if (g.SVGMatrix === undefined) Object.defineProperty(g, 'SVGMatrix', { value: SVGMatrix, writable: true, configurable: true });
    var FIELDS = ['a', 'b', 'c', 'd', 'e', 'f'];
    function matrix(m) {
        var o = Object.create(SVGMatrix.prototype);
        FIELDS.forEach(function (k, i) { o[k] = m[i]; });
        return o;
    }
    function values(m) {
        if (m === null || typeof m !== 'object') throw new TypeError("parameter 1 is not of type 'SVGMatrix'.");
        return FIELDS.map(function (k) { return Number(m[k]); });
    }
    function turned(self, by) { return matrix(multiply(values(self), by)); }
    var OPERATIONS = {
        multiply: function (other) { return turned(this, values(other)); },
        inverse: function () {
            var inv = invert(values(this));
            if (!inv) throw new g.DOMException("Failed to execute 'inverse' on 'SVGMatrix': The matrix is not invertible.", 'InvalidStateError');
            return matrix(inv);
        },
        translate: function (x, y) { return turned(this, [1, 0, 0, 1, Number(x), Number(y)]); },
        scale: function (s) { return turned(this, [Number(s), 0, 0, Number(s), 0, 0]); },
        scaleNonUniform: function (sx, sy) { return turned(this, [Number(sx), 0, 0, Number(sy), 0, 0]); },
        rotate: function (deg) { var r = Number(deg) * Math.PI / 180; return turned(this, [Math.cos(r), Math.sin(r), -Math.sin(r), Math.cos(r), 0, 0]); },
        flipX: function () { return turned(this, [-1, 0, 0, 1, 0, 0]); },
        flipY: function () { return turned(this, [1, 0, 0, -1, 0, 0]); },
        skewX: function (deg) { return turned(this, [1, 0, Math.tan(Number(deg) * Math.PI / 180), 1, 0, 0]); },
        skewY: function (deg) { return turned(this, [1, Math.tan(Number(deg) * Math.PI / 180), 0, 1, 0, 0]); }
    };
    Object.keys(OPERATIONS).forEach(function (name) { method(SVGMatrix.prototype, name, OPERATIONS[name]); });
    method(g.SVGPoint.prototype, 'matrixTransform', function matrixTransform(m) {
        var v = values(m), x = Number(this.x), y = Number(this.y);
        return point([v[0] * x + v[2] * y + v[4], v[1] * x + v[3] * y + v[5]]);
    });

    method(Graphics.prototype, 'getCTM', function getCTM() {
        own(this, Graphics);
        return screenCtm(this) ? matrix(ctm(this)) : null;
    });
    method(Graphics.prototype, 'getScreenCTM', function getScreenCTM() {
        var m = screenCtm(own(this, Graphics));
        return m ? matrix(m) : null;
    });
    var SVGSVG = g.SVGSVGElement;
    if (SVGSVG) {
        method(SVGSVG.prototype, 'createSVGPoint', function createSVGPoint() { return point([0, 0]); });
        method(SVGSVG.prototype, 'createSVGMatrix', function createSVGMatrix() { return matrix(IDENTITY); });
        method(SVGSVG.prototype, 'createSVGRect', function createSVGRect() { return rect([0, 0, 0, 0]); });
    }
    ['ownerSVGElement', 'viewportElement'].forEach(function (name) {
        Object.defineProperty(g.SVGElement.prototype, name, {
            get: function () { return ownerSvg(this); }, enumerable: true, configurable: true
        });
    });

    // getBoundingClientRect of an element inside an `<svg>`: its box,
    // through getScreenCTM. The outermost `<svg>` keeps its layout box.
    function clientBox(el) {
        if (!(el instanceof Graphics) || (el.localName === 'svg' && !ownerSvg(el))) return null;
        var m = screenCtm(el);
        if (!m) return null;
        var b = boxOf(el) || [0, 0, 0, 0];
        var x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
        [[b[0], b[1]], [b[0] + b[2], b[1]], [b[0], b[1] + b[3]], [b[0] + b[2], b[1] + b[3]]].forEach(function (c) {
            var x = m[0] * c[0] + m[2] * c[1] + m[4], y = m[1] * c[0] + m[3] * c[1] + m[5];
            x0 = Math.min(x0, x); y0 = Math.min(y0, y); x1 = Math.max(x1, x); y1 = Math.max(y1, y);
        });
        return typeof g.DOMRect === 'function' ? new g.DOMRect(x0, y0, x1 - x0, y1 - y0)
            : { x: x0, y: y0, width: x1 - x0, height: y1 - y0, left: x0, top: y0, right: x1, bottom: y1 };
    }
    method(g.SVGElement.prototype, 'getBoundingClientRect', function getBoundingClientRect() {
        return clientBox(this) || elementRect.call(this);
    });
    var elementRects = g.Element.prototype.getClientRects;
    method(g.SVGElement.prototype, 'getClientRects', function getClientRects() {
        var box = clientBox(this);
        return box ? [box] : elementRects.call(this);
    });
})(globalThis);
