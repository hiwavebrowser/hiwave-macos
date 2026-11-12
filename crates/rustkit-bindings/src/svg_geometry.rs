//! SVG shape geometry for `getBBox()`, `getTotalLength()` and
//! `getPointAtLength()` (SVG 2 §4.6.3, §9.9; STR-8).
//!
//! `web_svg_geometry.js` is the object layer: it reads a shape's attributes
//! and asks one host function for the numbers. The path a shape stands for is
//! built the way Blink builds it, because the answers script sees depend on
//! it: a circle is four conics starting at its right-most point and going
//! clockwise, an elliptical arc command is cubic Beziers (at most a quarter
//! turn each), and a length is the sum of the chords Skia's contour measure
//! cuts a curve into (a chord is kept once the curve is within half a unit of
//! it). So `getTotalLength()` of an `r=10` circle is 62.43, sixteen chords,
//! and not 62.83 (Chrome for Testing 148,
//! docs/diagnostics/2026-11-12/str8_svg_geometry_readystate_serializer_cft148.txt).

use rustkit_js::{JsError, JsRuntime, JsValue};

type P = (f64, f64);

#[derive(Clone, Copy, Debug)]
enum Seg {
    Line(P, P),
    Quad(P, P, P),
    /// A rational quadratic: start, control, end, weight.
    Conic(P, P, P, f64),
    Cubic(P, P, P, P),
}

/// A path as its drawn segments, in order. Contours only matter to the
/// builder (where `close` goes back to); the measure runs across them.
#[derive(Default)]
struct Path {
    segs: Vec<Seg>,
    start: P,
    at: P,
}

impl Path {
    fn move_to(&mut self, p: P) {
        self.start = p;
        self.at = p;
    }
    fn line_to(&mut self, p: P) {
        self.segs.push(Seg::Line(self.at, p));
        self.at = p;
    }
    fn quad_to(&mut self, c: P, p: P) {
        self.segs.push(Seg::Quad(self.at, c, p));
        self.at = p;
    }
    fn conic_to(&mut self, c: P, p: P, w: f64) {
        self.segs.push(Seg::Conic(self.at, c, p, w));
        self.at = p;
    }
    fn cubic_to(&mut self, c1: P, c2: P, p: P) {
        self.segs.push(Seg::Cubic(self.at, c1, c2, p));
        self.at = p;
    }
    fn close(&mut self) {
        if self.at != self.start {
            self.line_to(self.start);
        }
    }

    /// An elliptical arc command as cubics (Blink's
    /// `SVGPathNormalizer::DecomposeArcToCubic`).
    fn arc_to(&mut self, rx: f64, ry: f64, rotation_deg: f64, large: bool, sweep: bool, end: P) {
        let start = self.at;
        let (mut rx, mut ry) = (rx.abs(), ry.abs());
        if start == end {
            return;
        }
        if rx == 0.0 || ry == 0.0 {
            self.line_to(end);
            return;
        }
        let angle = rotation_deg.to_radians();
        let (sin_a, cos_a) = angle.sin_cos();
        // Rotate by -angle.
        let unrotate = |p: P| (p.0 * cos_a + p.1 * sin_a, -p.0 * sin_a + p.1 * cos_a);
        let mid = unrotate(((start.0 - end.0) / 2.0, (start.1 - end.1) / 2.0));
        let radii_scale = mid.0 * mid.0 / (rx * rx) + mid.1 * mid.1 / (ry * ry);
        if radii_scale > 1.0 {
            rx *= radii_scale.sqrt();
            ry *= radii_scale.sqrt();
        }
        let to_unit = |p: P| {
            let r = unrotate(p);
            (r.0 / rx, r.1 / ry)
        };
        let (p1, p2) = (to_unit(start), to_unit(end));
        let delta = (p2.0 - p1.0, p2.1 - p1.1);
        let d = delta.0 * delta.0 + delta.1 * delta.1;
        let mut factor = (1.0 / d - 0.25).max(0.0).sqrt();
        if sweep == large {
            factor = -factor;
        }
        let delta = (delta.0 * factor, delta.1 * factor);
        let center = ((p1.0 + p2.0) / 2.0 - delta.1, (p1.1 + p2.1) / 2.0 + delta.0);
        let theta1 = (p1.1 - center.1).atan2(p1.0 - center.0);
        let theta2 = (p2.1 - center.1).atan2(p2.0 - center.0);
        let mut arc = theta2 - theta1;
        if arc < 0.0 && sweep {
            arc += std::f64::consts::TAU;
        } else if arc > 0.0 && !sweep {
            arc -= std::f64::consts::TAU;
        }
        // Scale by the radii, then rotate by angle.
        let from_unit = |p: P| {
            let s = (p.0 * rx, p.1 * ry);
            (s.0 * cos_a - s.1 * sin_a, s.0 * sin_a + s.1 * cos_a)
        };
        let segments = (arc / (std::f64::consts::FRAC_PI_2 + 0.001)).abs().ceil() as usize;
        for i in 0..segments {
            let a0 = theta1 + i as f64 * arc / segments as f64;
            let a1 = theta1 + (i + 1) as f64 * arc / segments as f64;
            let t = (8.0 / 6.0) * (0.25 * (a1 - a0)).tan();
            if !t.is_finite() {
                return;
            }
            let (s0, c0) = a0.sin_cos();
            let (s1, c1) = a1.sin_cos();
            let q1 = (c0 - t * s0 + center.0, s0 + t * c0 + center.1);
            let target = (c1 + center.0, s1 + center.1);
            let q2 = (target.0 + t * s1, target.1 - t * c1);
            self.cubic_to(from_unit(q1), from_unit(q2), from_unit(target));
        }
        // The last cubic ends within rounding of `end`; the next command
        // starts from the point the author wrote.
        self.at = end;
    }
}

/// Numbers out of an attribute value: separated by spaces and commas, a sign
/// or a second decimal point starts a new one (`10-5`, `.5.5`).
struct Numbers<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Numbers<'a> {
    fn new(s: &'a str) -> Self {
        Self { s: s.as_bytes(), i: 0 }
    }
    fn skip_separators(&mut self) {
        while self.i < self.s.len() && (self.s[self.i].is_ascii_whitespace() || self.s[self.i] == b',') {
            self.i += 1;
        }
    }
    fn number(&mut self) -> Option<f64> {
        self.skip_separators();
        let start = self.i;
        let digits = |n: &mut Self| {
            let from = n.i;
            while n.i < n.s.len() && n.s[n.i].is_ascii_digit() {
                n.i += 1;
            }
            n.i > from
        };
        if self.i < self.s.len() && (self.s[self.i] == b'+' || self.s[self.i] == b'-') {
            self.i += 1;
        }
        let whole = digits(self);
        let mut fraction = false;
        if self.i < self.s.len() && self.s[self.i] == b'.' {
            self.i += 1;
            fraction = digits(self);
        }
        if !whole && !fraction {
            self.i = start;
            return None;
        }
        if self.i < self.s.len() && (self.s[self.i] == b'e' || self.s[self.i] == b'E') {
            let mark = self.i;
            self.i += 1;
            if self.i < self.s.len() && (self.s[self.i] == b'+' || self.s[self.i] == b'-') {
                self.i += 1;
            }
            if !digits(self) {
                self.i = mark;
            }
        }
        std::str::from_utf8(&self.s[start..self.i]).ok()?.parse().ok()
    }
    /// An arc flag: a single `0` or `1`, which may touch what follows.
    fn flag(&mut self) -> Option<bool> {
        self.skip_separators();
        match self.s.get(self.i) {
            Some(b'0') => {
                self.i += 1;
                Some(false)
            }
            Some(b'1') => {
                self.i += 1;
                Some(true)
            }
            _ => None,
        }
    }
    fn command(&mut self) -> Option<u8> {
        self.skip_separators();
        match self.s.get(self.i) {
            Some(c) if c.is_ascii_alphabetic() => {
                self.i += 1;
                Some(*c)
            }
            _ => None,
        }
    }
    fn at_end(&mut self) -> bool {
        self.skip_separators();
        self.i >= self.s.len()
    }
}

/// A `d` attribute. Parsing stops at the first error and keeps what came
/// before it (SVG 2 §9.5.4).
fn parse_path(d: &str) -> Path {
    let mut path = Path::default();
    let mut n = Numbers::new(d);
    let mut command = 0u8;
    // The control point a smooth command reflects, and the kind it is for.
    let mut last_control: Option<(P, u8)> = None;
    let mut first = true;
    while !n.at_end() {
        if let Some(c) = n.command() {
            command = c;
        } else if command == 0 || command.eq_ignore_ascii_case(&b'z') {
            break;
        }
        if first && !command.eq_ignore_ascii_case(&b'm') {
            break;
        }
        first = false;
        let relative = command.is_ascii_lowercase();
        let base = if relative { path.at } else { (0.0, 0.0) };
        let point = |n: &mut Numbers| -> Option<P> { Some((base.0 + n.number()?, base.1 + n.number()?)) };
        let mut control = None;
        match command.to_ascii_uppercase() {
            b'M' => {
                let Some(p) = point(&mut n) else { break };
                path.move_to(p);
                // Further pairs are implicit line-tos.
                command = if relative { b'l' } else { b'L' };
            }
            b'L' => {
                let Some(p) = point(&mut n) else { break };
                path.line_to(p);
            }
            b'H' => {
                let Some(x) = n.number() else { break };
                path.line_to((base.0 + x, path.at.1));
            }
            b'V' => {
                let Some(y) = n.number() else { break };
                path.line_to((path.at.0, base.1 + y));
            }
            b'C' => {
                let (Some(c1), Some(c2), Some(p)) = (point(&mut n), point(&mut n), point(&mut n)) else { break };
                path.cubic_to(c1, c2, p);
                control = Some((c2, b'C'));
            }
            b'S' => {
                let (Some(c2), Some(p)) = (point(&mut n), point(&mut n)) else { break };
                let c1 = match last_control {
                    Some((c, b'C')) => (2.0 * path.at.0 - c.0, 2.0 * path.at.1 - c.1),
                    _ => path.at,
                };
                path.cubic_to(c1, c2, p);
                control = Some((c2, b'C'));
            }
            b'Q' => {
                let (Some(c), Some(p)) = (point(&mut n), point(&mut n)) else { break };
                path.quad_to(c, p);
                control = Some((c, b'Q'));
            }
            b'T' => {
                let Some(p) = point(&mut n) else { break };
                let c = match last_control {
                    Some((c, b'Q')) => (2.0 * path.at.0 - c.0, 2.0 * path.at.1 - c.1),
                    _ => path.at,
                };
                path.quad_to(c, p);
                control = Some((c, b'Q'));
            }
            b'A' => {
                let (Some(rx), Some(ry), Some(rotation), Some(large), Some(sweep)) =
                    (n.number(), n.number(), n.number(), n.flag(), n.flag())
                else {
                    break;
                };
                let Some(p) = point(&mut n) else { break };
                path.arc_to(rx, ry, rotation, large, sweep, p);
            }
            b'Z' => path.close(),
            _ => break,
        }
        last_control = control;
    }
    path
}

const CONIC_QUARTER: f64 = std::f64::consts::FRAC_1_SQRT_2;

fn ellipse(cx: f64, cy: f64, rx: f64, ry: f64) -> Path {
    let mut path = Path::default();
    if !(rx > 0.0 && ry > 0.0) {
        return path;
    }
    path.move_to((cx + rx, cy));
    path.conic_to((cx + rx, cy + ry), (cx, cy + ry), CONIC_QUARTER);
    path.conic_to((cx - rx, cy + ry), (cx - rx, cy), CONIC_QUARTER);
    path.conic_to((cx - rx, cy - ry), (cx, cy - ry), CONIC_QUARTER);
    path.conic_to((cx + rx, cy - ry), (cx + rx, cy), CONIC_QUARTER);
    path
}

/// `rx` / `ry` are NaN where the attribute is absent: each then takes the
/// other's value, and both are capped at half the side (SVG 2 §10.2).
fn rect(x: f64, y: f64, w: f64, h: f64, rx: f64, ry: f64) -> Path {
    let mut path = Path::default();
    if !(w > 0.0 && h > 0.0) {
        return path;
    }
    let (rx, ry) = match (rx.is_finite() && rx >= 0.0, ry.is_finite() && ry >= 0.0) {
        (true, true) => (rx, ry),
        (true, false) => (rx, rx),
        (false, true) => (ry, ry),
        (false, false) => (0.0, 0.0),
    };
    let (rx, ry) = (rx.min(w / 2.0), ry.min(h / 2.0));
    if rx <= 0.0 || ry <= 0.0 {
        path.move_to((x, y));
        path.line_to((x + w, y));
        path.line_to((x + w, y + h));
        path.line_to((x, y + h));
        path.close();
        return path;
    }
    path.move_to((x + rx, y));
    path.line_to((x + w - rx, y));
    path.conic_to((x + w, y), (x + w, y + ry), CONIC_QUARTER);
    path.line_to((x + w, y + h - ry));
    path.conic_to((x + w, y + h), (x + w - rx, y + h), CONIC_QUARTER);
    path.line_to((x + rx, y + h));
    path.conic_to((x, y + h), (x, y + h - ry), CONIC_QUARTER);
    path.line_to((x, y + ry));
    path.conic_to((x, y), (x + rx, y), CONIC_QUARTER);
    path
}

fn poly(points: &str, closed: bool) -> Path {
    let mut path = Path::default();
    let mut n = Numbers::new(points);
    let mut first = true;
    while let (Some(x), Some(y)) = (n.number(), n.number()) {
        if first {
            path.move_to((x, y));
            first = false;
        } else {
            path.line_to((x, y));
        }
    }
    if closed && !path.segs.is_empty() {
        path.close();
    }
    path
}

/// The path of a shape element. `spec` is the `d` or `points` attribute, or
/// the shape's numbers in the order the JS layer writes them.
fn shape(kind: &str, spec: &str) -> Path {
    let mut n = Numbers::new(spec);
    // An absent attribute is written as `x`, which reads as NaN here.
    let mut next = |fallback: f64| -> f64 {
        n.skip_separators();
        if n.s.get(n.i) == Some(&b'x') {
            n.i += 1;
            return fallback;
        }
        n.number().unwrap_or(fallback)
    };
    match kind {
        "path" => parse_path(spec),
        "polyline" => poly(spec, false),
        "polygon" => poly(spec, true),
        "rect" => {
            let (x, y, w, h) = (next(0.0), next(0.0), next(0.0), next(0.0));
            rect(x, y, w, h, next(f64::NAN), next(f64::NAN))
        }
        "circle" => {
            let (cx, cy, r) = (next(0.0), next(0.0), next(0.0));
            ellipse(cx, cy, r, r)
        }
        "ellipse" => {
            let (cx, cy, rx, ry) = (next(0.0), next(0.0), next(0.0), next(0.0));
            ellipse(cx, cy, rx, ry)
        }
        "line" => {
            let mut path = Path::default();
            let (x1, y1, x2, y2) = (next(0.0), next(0.0), next(0.0), next(0.0));
            path.move_to((x1, y1));
            path.line_to((x2, y2));
            path
        }
        _ => Path::default(),
    }
}

fn lerp(a: P, b: P, t: f64) -> P {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

fn distance(a: P, b: P) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn eval(seg: &Seg, t: f64) -> P {
    match *seg {
        Seg::Line(a, b) => lerp(a, b, t),
        Seg::Quad(a, c, b) => lerp(lerp(a, c, t), lerp(c, b, t), t),
        Seg::Conic(a, c, b, w) => {
            let (u, wt) = (1.0 - t, 2.0 * t * (1.0 - t) * w);
            let denominator = u * u + wt + t * t;
            (
                (u * u * a.0 + wt * c.0 + t * t * b.0) / denominator,
                (u * u * a.1 + wt * c.1 + t * t * b.1) / denominator,
            )
        }
        Seg::Cubic(a, c1, c2, b) => {
            let (ab, bc, cd) = (lerp(a, c1, t), lerp(c1, c2, t), lerp(c2, b, t));
            lerp(lerp(ab, bc, t), lerp(bc, cd, t), t)
        }
    }
}

/// Skia's contour measure (`SkContourMeasure`): `t` is a 30-bit fixed-point
/// number, a curve is halved while it is further than `TOLERANCE` from its
/// chord (the larger of the two axis distances) and its `t` span is still
/// wide enough, and each piece then counts as its chord.
const MAX_T: u32 = 0x3FFF_FFFF;
const TOLERANCE: f64 = 0.5;

struct Chord {
    /// Distance along the path at the chord's end.
    end: f64,
    t: u32,
    seg: usize,
}

fn far(a: P, b: P) -> bool {
    (a.0 - b.0).abs().max((a.1 - b.1).abs()) > TOLERANCE
}

fn span_is_wide(span: u32) -> bool {
    span >> 10 != 0
}

fn scalar(t: u32) -> f64 {
    t as f64 / MAX_T as f64
}

struct Measure {
    chords: Vec<Chord>,
    length: f64,
}

impl Measure {
    fn chord(&mut self, from: P, to: P, t: u32, seg: usize) {
        let d = distance(from, to);
        if d > 0.0 && d.is_finite() {
            self.length += d;
            self.chords.push(Chord { end: self.length, t, seg });
        }
    }

    fn quad(&mut self, a: P, c: P, b: P, min: u32, max: u32, seg: usize) {
        // Half the control point's offset from the chord's midpoint.
        let middle = (a.0 / 4.0 + b.0 / 4.0, a.1 / 4.0 + b.1 / 4.0);
        if span_is_wide(max - min) && far((c.0 / 2.0, c.1 / 2.0), middle) {
            let half = (min + max) >> 1;
            let (ac, cb) = (lerp(a, c, 0.5), lerp(c, b, 0.5));
            let mid = lerp(ac, cb, 0.5);
            self.quad(a, ac, mid, min, half, seg);
            self.quad(mid, cb, b, half, max, seg);
        } else {
            self.chord(a, b, max, seg);
        }
    }

    fn conic(&mut self, curve: &Seg, min: u32, from: P, max: u32, to: P, seg: usize) {
        let half = (min + max) >> 1;
        let mid = eval(curve, scalar(half));
        if !(mid.0.is_finite() && mid.1.is_finite()) {
            return;
        }
        if span_is_wide(max - min) && far(mid, lerp(from, to, 0.5)) {
            self.conic(curve, min, from, half, mid, seg);
            self.conic(curve, half, mid, max, to, seg);
        } else {
            self.chord(from, to, max, seg);
        }
    }

    fn cubic(&mut self, p: [P; 4], min: u32, max: u32, seg: usize) {
        let curvy = far(p[1], lerp(p[0], p[3], 1.0 / 3.0)) || far(p[2], lerp(p[0], p[3], 2.0 / 3.0));
        if span_is_wide(max - min) && curvy {
            let half = (min + max) >> 1;
            let (ab, bc, cd) = (lerp(p[0], p[1], 0.5), lerp(p[1], p[2], 0.5), lerp(p[2], p[3], 0.5));
            let (abc, bcd) = (lerp(ab, bc, 0.5), lerp(bc, cd, 0.5));
            let mid = lerp(abc, bcd, 0.5);
            self.cubic([p[0], ab, abc, mid], min, half, seg);
            self.cubic([mid, bcd, cd, p[3]], half, max, seg);
        } else {
            self.chord(p[0], p[3], max, seg);
        }
    }

    /// A `path` element's curve, the way Blink's path traversal measures
    /// it (`PathTraversalState`): halved until its control polygon is within
    /// 0.00001 of its chord (or twenty levels deep), each piece counting as
    /// its control polygon. So a path's length is its true length, where a
    /// shape's is Skia's chords: `M10 10 C 20 20, 40 20, 50 10` is 43.81,
    /// and the same cubic cut into chords would be 43.74.
    fn fine(&mut self, points: &[P], t0: f64, t1: f64, depth: u32, seg: usize) {
        let polygon: f64 = points.windows(2).map(|w| distance(w[0], w[1])).sum();
        let chord = distance(points[0], points[points.len() - 1]);
        if polygon - chord > 0.00001 && depth <= 20 {
            let mid = (t0 + t1) / 2.0;
            if let [a, c, b] = *points {
                let (ac, cb) = (lerp(a, c, 0.5), lerp(c, b, 0.5));
                let m = lerp(ac, cb, 0.5);
                self.fine(&[a, ac, m], t0, mid, depth + 1, seg);
                self.fine(&[m, cb, b], mid, t1, depth + 1, seg);
            } else if let [a, c1, c2, b] = *points {
                let (ab, bc, cd) = (lerp(a, c1, 0.5), lerp(c1, c2, 0.5), lerp(c2, b, 0.5));
                let (abc, bcd) = (lerp(ab, bc, 0.5), lerp(bc, cd, 0.5));
                let m = lerp(abc, bcd, 0.5);
                self.fine(&[a, ab, abc, m], t0, mid, depth + 1, seg);
                self.fine(&[m, bcd, cd, b], mid, t1, depth + 1, seg);
            }
        } else if polygon > 0.0 && polygon.is_finite() {
            self.length += polygon;
            self.chords.push(Chord { end: self.length, t: (t1 * MAX_T as f64) as u32, seg });
        }
    }

    /// `traversal`: the path is a `path` element's (see [`Self::fine`]).
    fn of(path: &Path, traversal: bool) -> Self {
        let mut measure = Measure { chords: Vec::new(), length: 0.0 };
        for (index, seg) in path.segs.iter().enumerate() {
            match *seg {
                Seg::Line(a, b) => measure.chord(a, b, MAX_T, index),
                Seg::Quad(a, c, b) if traversal => measure.fine(&[a, c, b], 0.0, 1.0, 0, index),
                Seg::Cubic(a, c1, c2, b) if traversal => measure.fine(&[a, c1, c2, b], 0.0, 1.0, 0, index),
                Seg::Quad(a, c, b) => measure.quad(a, c, b, 0, MAX_T, index),
                Seg::Conic(a, _, b, _) => measure.conic(seg, 0, a, MAX_T, b, index),
                Seg::Cubic(a, c1, c2, b) => measure.cubic([a, c1, c2, b], 0, MAX_T, index),
            }
        }
        measure
    }

    /// The point `at` along the path, clamped to its ends. An empty path
    /// answers the origin.
    fn point(&self, path: &Path, at: f64) -> P {
        let at = if at.is_nan() { 0.0 } else { at.clamp(0.0, self.length) };
        let Some(index) = self.chords.iter().position(|c| c.end >= at) else {
            return (0.0, 0.0);
        };
        let chord = &self.chords[index];
        let (mut start_distance, mut start_t) = (0.0, 0u32);
        if index > 0 {
            let before = &self.chords[index - 1];
            start_distance = before.end;
            if before.seg == chord.seg {
                start_t = before.t;
            }
        }
        let along = (at - start_distance) / (chord.end - start_distance);
        let t = scalar(start_t) + (scalar(chord.t) - scalar(start_t)) * along;
        eval(&path.segs[chord.seg], t)
    }
}

/// The tight box around the path: a curve reaches it at its ends and where
/// its derivative is zero on an axis, not at its control points. The conics
/// here are quarter ellipses on the axes, whose extremes are their ends.
fn bbox(path: &Path) -> [f64; 4] {
    let mut bounds: Option<(f64, f64, f64, f64)> = None;
    let mut add = |p: P| {
        if !(p.0.is_finite() && p.1.is_finite()) {
            return;
        }
        bounds = Some(match bounds {
            None => (p.0, p.1, p.0, p.1),
            Some((x0, y0, x1, y1)) => (x0.min(p.0), y0.min(p.1), x1.max(p.0), y1.max(p.1)),
        });
    };
    for seg in &path.segs {
        match *seg {
            Seg::Line(a, b) | Seg::Conic(a, _, b, _) => {
                add(a);
                add(b);
            }
            Seg::Quad(a, c, b) => {
                add(a);
                add(b);
                for (p0, p1, p2) in [(a.0, c.0, b.0), (a.1, c.1, b.1)] {
                    let denominator = p0 - 2.0 * p1 + p2;
                    if denominator != 0.0 {
                        let t = (p0 - p1) / denominator;
                        if t > 0.0 && t < 1.0 {
                            add(eval(seg, t));
                        }
                    }
                }
            }
            Seg::Cubic(a, c1, c2, b) => {
                add(a);
                add(b);
                for (p0, p1, p2, p3) in [(a.0, c1.0, c2.0, b.0), (a.1, c1.1, c2.1, b.1)] {
                    // The derivative, as At^2 + Bt + C.
                    let qa = -p0 + 3.0 * p1 - 3.0 * p2 + p3;
                    let qb = 2.0 * (p0 - 2.0 * p1 + p2);
                    let qc = p1 - p0;
                    let mut root = |t: f64| {
                        if t > 0.0 && t < 1.0 {
                            add(eval(seg, t));
                        }
                    };
                    if qa.abs() < 1e-12 {
                        if qb != 0.0 {
                            root(-qc / qb);
                        }
                    } else {
                        let discriminant = qb * qb - 4.0 * qa * qc;
                        if discriminant >= 0.0 {
                            let s = discriminant.sqrt();
                            root((-qb + s) / (2.0 * qa));
                            root((-qb - s) / (2.0 * qa));
                        }
                    }
                }
            }
        }
    }
    match bounds {
        Some((x0, y0, x1, y1)) => [x0, y0, x1 - x0, y1 - y0],
        None => [0.0; 4],
    }
}

/// `[x, y, width, height, length, pointX, pointY]` for a shape, the point
/// being the one `at` along it.
fn geometry(kind: &str, spec: &str, at: f64) -> [f64; 7] {
    let path = shape(kind, spec);
    let measure = Measure::of(&path, kind == "path");
    let [x, y, w, h] = bbox(&path);
    let point = measure.point(&path, at);
    [x, y, w, h, measure.length, point.0, point.1]
}

pub(crate) fn install(runtime: &mut JsRuntime) -> Result<(), JsError> {
    runtime.register_host_function(
        "__rustkit_svg_geometry",
        3,
        Box::new(|args| {
            let (Some(JsValue::String(kind)), Some(JsValue::String(spec))) = (args.first(), args.get(1)) else {
                return JsValue::Null;
            };
            let at = match args.get(2) {
                Some(JsValue::Number(n)) => *n,
                _ => 0.0,
            };
            let numbers: Vec<String> = geometry(kind, spec, at)
                .iter()
                .map(|n| if n.is_finite() { n.to_string() } else { "0".to_string() })
                .collect();
            JsValue::String(format!("[{}]", numbers.join(",")))
        }),
    )?;
    runtime.evaluate_script(include_str!("web_svg_geometry.js")).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 0.005
    }

    /// Chrome for Testing 148 `getTotalLength()`, two decimals.
    #[test]
    fn lengths_are_skias_chords() {
        for (kind, spec, length) in [
            ("rect", "5 6 30 20 x x", 100.0),
            ("rect", "5 6 30 20 4 x", 92.49),
            ("circle", "50 60 10", 62.43),
            ("ellipse", "80 60 20 10", 96.27),
            ("line", "10 100 40 140", 50.0),
            ("polyline", "100,10 130,10 130,50", 70.0),
            ("polygon", "150,10 180,10 180,50", 120.0),
            ("path", "M10 150 L 110 150 L 110 190", 140.0),
            ("path", "M200 100 h 30 v 40 z", 120.0),
            ("path", "M10 10 C 20 20, 40 20, 50 10", 43.81),
            ("path", "M10 80 Q 30 40 50 80 T 90 80", 118.32),
            ("path", "M200 150 A 20 10 0 0 1 240 150", 48.45),
            ("path", "M0 0 L 10 0 M 20 0 L 20 30", 40.0),
            ("path", "", 0.0),
        ] {
            let got = geometry(kind, spec, 0.0)[4];
            assert!(close(got, length), "{kind} `{spec}`: length {got}, Chrome {length}");
        }
    }

    /// Chrome for Testing 148 `getBBox()`.
    #[test]
    fn boxes_are_tight() {
        for (kind, spec, want) in [
            ("circle", "50 60 10", [40.0, 50.0, 20.0, 20.0]),
            ("path", "M10 10 C 20 20, 40 20, 50 10", [10.0, 10.0, 40.0, 7.5]),
            ("path", "M10 80 Q 30 40 50 80 T 90 80", [10.0, 60.0, 80.0, 40.0]),
            ("path", "M200 150 A 20 10 0 0 1 240 150", [200.0, 140.0, 40.0, 10.0]),
            ("path", "M0 0 L 10 0 M 20 0 L 20 30", [0.0, 0.0, 20.0, 30.0]),
            ("path", "", [0.0, 0.0, 0.0, 0.0]),
        ] {
            let got = geometry(kind, spec, 0.0);
            assert!(
                (0..4).all(|i| close(got[i], want[i])),
                "{kind} `{spec}`: bbox {:?}, Chrome {want:?}",
                &got[..4]
            );
        }
    }

    /// Chrome for Testing 148 `getPointAtLength()`: clamped to the path's
    /// ends, carried across subpaths, and on the curve (not on its chord).
    #[test]
    fn points_follow_the_path() {
        for (kind, spec, at, want) in [
            ("path", "M0 0 L 10 0 M 20 0 L 20 30", 25.0, (20.0, 15.0)),
            ("path", "M0 0 L 10 0 M 20 0 L 20 30", 1000.0, (20.0, 30.0)),
            ("path", "M0 0 L 10 0 M 20 0 L 20 30", -5.0, (0.0, 0.0)),
            ("rect", "5 6 30 20 x x", 25.0, (30.0, 6.0)),
            ("rect", "5 6 30 20 x x", 1000.0, (5.0, 6.0)),
            ("circle", "50 60 10", 0.0, (60.0, 60.0)),
            ("circle", "50 60 10", 25.0, (41.88, 65.84)),
        ] {
            let got = geometry(kind, spec, at);
            assert!(
                close(got[5], want.0) && close(got[6], want.1),
                "{kind} `{spec}` at {at}: ({}, {}), Chrome {want:?}",
                got[5],
                got[6]
            );
        }
    }
}
