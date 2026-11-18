//! A blurred box shadow's alpha, as Chrome paints it (SHADOW-BLUR).
//!
//! CSS Backgrounds 3 §7.2: the shadow shape is blurred by a Gaussian of
//! standard deviation half the blur radius. Chrome for Testing 148 paints
//! exactly that, sampled at pixel centres (to within 0.7 of a level on
//! `tools/parity_oracle/box_shadow_blur_cases.json`).
//!
//! The blurred coverage of a rounded rect is computed per point: the shape
//! is a stack of horizontal spans, the Gaussian is separable, so the
//! horizontal blur of each span is a difference of two normal CDFs and the
//! vertical blur is an integral over the spans' heights. Where both sides
//! are straight the integral is closed form; across a corner it is summed
//! over short slices, each weighted by its exact Gaussian mass.
//!
//! The renderer draws the result as a mesh of cells whose corners carry the
//! exact value and whose insides are interpolated by the GPU (`mesh_knots`).

use hashbrown::HashMap;
use rustkit_layout::{BorderRadius, CornerRadius, Rect};
use std::sync::Arc;

/// The largest standard deviation drawn. A blur past it (an infinite or
/// absurd length) is drawn at it, so mesh positions stay finite.
const MAX_SIGMA: f32 = 1000.0;

/// The Gaussian's standard deviation for a CSS blur radius: half of it,
/// clamped to `0..=MAX_SIGMA` (NaN is no blur).
pub(crate) fn sigma_of(blur_radius: f32) -> f32 {
    if blur_radius.is_nan() {
        return 0.0;
    }
    (blur_radius.max(0.0) * 0.5).min(MAX_SIGMA)
}

/// How far past the shape the blur is drawn: three standard deviations,
/// where a full-alpha shadow is about a third of a level (1 - Phi(3) of 255,
/// 0.34) from nothing.
pub(crate) fn reach_of(sigma: f32) -> f32 {
    3.0 * sigma
}

/// The standard normal CDF (Abramowitz and Stegun 7.1.26 for erf; absolute
/// error under 1.5e-7).
fn phi(z: f32) -> f32 {
    let x = (z as f64) * std::f64::consts::FRAC_1_SQRT_2;
    let t = 1.0 / (1.0 + 0.327_591_1 * x.abs());
    let y = 1.0
        - (((((1.061_405_429 * t - 1.453_152_027) * t) + 1.421_413_741) * t - 0.284_496_736) * t
            + 0.254_829_592)
            * t
            * (-x * x).exp();
    let erf = if x >= 0.0 { y } else { -y };
    (0.5 * (1.0 + erf)) as f32
}

/// A rounded rect: the shadow shape, its radii already fitted.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Shape {
    pub rect: Rect,
    pub radius: BorderRadius,
}

impl Shape {
    pub(crate) fn new(rect: Rect, radius: BorderRadius) -> Self {
        let radius = if radius.is_zero() {
            radius
        } else {
            radius.fitted(rect.width.max(0.0), rect.height.max(0.0))
        };
        Self { rect, radius }
    }

    fn corners(&self) -> [CornerRadius; 4] {
        let r = self.radius;
        [r.top_left, r.top_right, r.bottom_right, r.bottom_left]
    }

    /// The shape's horizontal extent at height `t` (inside its rect).
    fn span(&self, t: f32) -> (f32, f32) {
        let r = self.rect;
        let [tl, tr, br, bl] = self.corners();
        // How far a corner's curve pulls the edge in at height `t`.
        let pull = |c: CornerRadius, dy: f32| {
            if c.is_zero() || dy <= 0.0 {
                return 0.0;
            }
            let k = (dy / c.v).min(1.0);
            c.h * (1.0 - (1.0 - k * k).max(0.0).sqrt())
        };
        let top = |c: CornerRadius| r.y + c.v - t;
        let bottom = |c: CornerRadius| t - (r.bottom() - c.v);
        let left = r.x + pull(tl, top(tl)).max(pull(bl, bottom(bl)));
        let right = r.right() - pull(tr, top(tr)).max(pull(br, bottom(br)));
        (left, right)
    }

    /// Whether neither side is curved anywhere in `[a, b]`.
    fn straight_between(&self, a: f32, b: f32) -> bool {
        let r = self.rect;
        let [tl, tr, br, bl] = self.corners();
        let v = |c: CornerRadius| if c.is_zero() { 0.0 } else { c.v };
        a >= r.y + v(tl).max(v(tr)) && b <= r.bottom() - v(bl).max(v(br))
    }

    /// The x (or, with `vertical`, y) positions where the blurred shape
    /// stops being constant along that axis: its edges and the inner ends of
    /// its corners.
    pub(crate) fn breaks(&self, vertical: bool) -> [f32; 4] {
        let r = self.rect;
        let [tl, tr, br, bl] = self.corners();
        let h = |c: CornerRadius| if c.is_zero() { 0.0 } else { c.h };
        let v = |c: CornerRadius| if c.is_zero() { 0.0 } else { c.v };
        if vertical {
            [
                r.y,
                r.y + v(tl).max(v(tr)),
                r.bottom() - v(bl).max(v(br)),
                r.bottom(),
            ]
        } else {
            [
                r.x,
                r.x + h(tl).max(h(bl)),
                r.right() - h(tr).max(h(br)),
                r.right(),
            ]
        }
    }
}

/// Slices per curved stretch of the vertical integral.
const CURVED_SLICES: usize = 12;

/// The shape's coverage at `(x, y)` after a Gaussian blur of `sigma`, in
/// 0..=1. With `sigma` 0, whether the point is inside.
pub(crate) fn blurred_coverage(shape: &Shape, sigma: f32, x: f32, y: f32) -> f32 {
    let r = shape.rect;
    if r.width <= 0.0 || r.height <= 0.0 {
        return 0.0;
    }
    if sigma <= 0.0 {
        if y < r.y || y >= r.bottom() {
            return 0.0;
        }
        let (l, rt) = shape.span(y);
        return if x >= l && x < rt { 1.0 } else { 0.0 };
    }
    let window = 4.0 * sigma;
    let lo = r.y.max(y - window);
    let hi = r.bottom().min(y + window);
    if hi <= lo {
        return 0.0;
    }
    let mass = |a: f32, b: f32| phi((b - y) / sigma) - phi((a - y) / sigma);
    let across = |t: f32| {
        let (l, rt) = shape.span(t);
        if rt <= l {
            0.0
        } else {
            phi((rt - x) / sigma) - phi((l - x) / sigma)
        }
    };
    let mut cuts: Vec<f32> = shape
        .breaks(true)
        .into_iter()
        .filter(|c| *c > lo && *c < hi)
        .collect();
    cuts.push(lo);
    cuts.push(hi);
    cuts.sort_by(f32::total_cmp);
    let mut sum = 0.0;
    for w in cuts.windows(2) {
        let (a, b) = (w[0], w[1]);
        if b <= a {
            continue;
        }
        if shape.straight_between(a, b) {
            sum += mass(a, b) * across(0.5 * (a + b));
        } else {
            let step = (b - a) / CURVED_SLICES as f32;
            for i in 0..CURVED_SLICES {
                let s0 = a + step * i as f32;
                sum += mass(s0, s0 + step) * across(s0 + 0.5 * step);
            }
        }
    }
    sum.clamp(0.0, 1.0)
}

/// Most knots one axis of a shadow's mesh gets.
const MAX_KNOTS: usize = 64;

/// The cut positions along one axis of a shadow's mesh, from `lo` to `hi`:
/// every `step` across each stretch where the blurred shape changes along
/// this axis (`breaks` grown by `reach`), plus the `extra` positions
/// (a hole's or clip's edges), so a cell is either all inside or all outside
/// them. Between the stretches the blurred value is constant along this
/// axis and one cell spans the gap.
pub(crate) fn mesh_knots(
    lo: f32,
    hi: f32,
    breaks: [f32; 4],
    reach: f32,
    extra: &[f32],
    detail: Detail,
) -> Vec<f32> {
    let mut knots = vec![lo, hi];
    if hi <= lo {
        return knots;
    }
    // Two stretches, near and far edge, merged if they meet.
    let mut stretches = vec![
        (breaks[0] - reach, breaks[1] + reach),
        (breaks[2] - reach, breaks[3] + reach),
    ];
    if stretches[0].1 >= stretches[1].0 {
        stretches = vec![(stretches[0].0, stretches[1].1)];
    }
    let total: f32 = stretches
        .iter()
        .map(|(a, b)| (b.min(hi) - a.max(lo)).max(0.0))
        .sum();
    // Fine: a third of a standard deviation (reach / 9) keeps linear
    // interpolation within about a percent of the alpha, corners included
    // (half a sigma leaves 4 levels at a corner); no finer than a pixel; no
    // more than MAX_KNOTS. Coarse (`MeshCache`'s fallback): one sigma, at
    // most a quarter of the knots.
    let (per_reach, max_knots) = match detail {
        Detail::Fine => (9.0, MAX_KNOTS),
        Detail::Coarse => (3.0, MAX_KNOTS / 4),
    };
    let step = (reach / per_reach).max(1.0).max(total / max_knots as f32);
    for (a, b) in stretches {
        let (a, b) = (a.max(lo), b.min(hi));
        if b <= a {
            continue;
        }
        let n = ((b - a) / step).ceil().max(1.0) as usize;
        for i in 0..=n {
            knots.push(a + (b - a) * i as f32 / n as f32);
        }
    }
    knots.extend(extra.iter().copied().filter(|k| *k > lo && *k < hi));
    knots.sort_by(f32::total_cmp);
    knots.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    knots
}

/// A shadow's mesh: cut positions along each axis and the value at every
/// knot crossing, row-major. Positions are relative to the mesh's origin, so
/// one mesh serves every box of the same shape (`MeshKey`).
#[derive(Debug)]
pub(crate) struct Mesh {
    pub xs: Vec<f32>,
    pub ys: Vec<f32>,
    pub values: Vec<f32>,
}

impl Mesh {
    /// The four corner values of cell `(i, j)`: top-left, top-right,
    /// bottom-right, bottom-left.
    pub(crate) fn cell(&self, i: usize, j: usize) -> [f32; 4] {
        let n = self.xs.len();
        [
            self.values[j * n + i],
            self.values[j * n + i + 1],
            self.values[(j + 1) * n + i + 1],
            self.values[(j + 1) * n + i],
        ]
    }
}

/// What a mesh depends on, each length to 1/64 px: the shadow shape and
/// the box it is cut against (the hole of an outer shadow, the clip of an
/// inset one), both relative to the mesh origin, sigma, and which kind.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct MeshKey([i32; 26]);

fn quantize(v: f32) -> i32 {
    (v * 64.0).round() as i32
}

fn radii(r: &BorderRadius) -> [f32; 8] {
    let c = [r.top_left, r.top_right, r.bottom_right, r.bottom_left];
    [
        c[0].h, c[0].v, c[1].h, c[1].v, c[2].h, c[2].v, c[3].h, c[3].v,
    ]
}

/// The mesh of an outer shadow (`inset` false: `shape` blurred, over the
/// shape grown by the reach) or an inset one (`inset` true: one minus
/// `shape` blurred, over `other`, the clip). `other` adds its own edges to
/// the cuts. Positions are relative to `origin`.
pub(crate) fn mesh_key(
    shape: &Shape,
    other: &Shape,
    sigma: f32,
    inset: bool,
    origin: (f32, f32),
) -> MeshKey {
    let mut k = [0i32; 26];
    let (ox, oy) = origin;
    let rel = |r: Rect| [r.x - ox, r.y - oy, r.width, r.height];
    let fields = rel(shape.rect)
        .into_iter()
        .chain(radii(&shape.radius))
        .chain(rel(other.rect))
        .chain(radii(&other.radius))
        .chain([sigma, if inset { 1.0 } else { 0.0 }]);
    for (slot, v) in k.iter_mut().zip(fields) {
        *slot = quantize(v);
    }
    MeshKey(k)
}

/// How finely a mesh is cut (`mesh_knots`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Detail {
    Fine,
    Coarse,
}

/// The mesh's cut positions, in page coordinates.
fn knots_of(
    shape: &Shape,
    other: &Shape,
    sigma: f32,
    inset: bool,
    detail: Detail,
) -> (Vec<f32>, Vec<f32>) {
    let reach = reach_of(sigma);
    let (x0, x1, y0, y1) = if inset {
        (
            other.rect.x,
            other.rect.right(),
            other.rect.y,
            other.rect.bottom(),
        )
    } else {
        (
            shape.rect.x - reach,
            shape.rect.right() + reach,
            shape.rect.y - reach,
            shape.rect.bottom() + reach,
        )
    };
    let xs = mesh_knots(
        x0,
        x1,
        shape.breaks(false),
        reach,
        &other.breaks(false),
        detail,
    );
    let ys = mesh_knots(
        y0,
        y1,
        shape.breaks(true),
        reach,
        &other.breaks(true),
        detail,
    );
    (xs, ys)
}

/// Most slices across one curved stretch of `build_mesh`'s integral.
const MAX_SLICES: usize = 256;

/// The horizontal slices `build_mesh` integrates over: the straight middle
/// whole, each curved stretch in slices of at most half a sigma (at least
/// `CURVED_SLICES`, at most `MAX_SLICES`).
fn slices_of(shape: &Shape, sigma: f32) -> Vec<(f32, f32)> {
    let [top, b1, b2, bottom] = shape.breaks(true);
    let mut out = Vec::new();
    let mut curved = |a: f32, b: f32, out: &mut Vec<(f32, f32)>| {
        if b <= a {
            return;
        }
        let n =
            (((b - a) / (0.5 * sigma).max(0.25)).ceil() as usize).clamp(CURVED_SLICES, MAX_SLICES);
        let step = (b - a) / n as f32;
        for i in 0..n {
            out.push((a + step * i as f32, a + step * (i + 1) as f32));
        }
    };
    if b1 < b2 {
        curved(top, b1, &mut out);
        out.push((b1, b2));
        curved(b2, bottom, &mut out);
    } else {
        curved(top, bottom, &mut out);
    }
    out
}

/// Build the mesh `mesh_key` describes, relative to `origin`.
///
/// The value at every knot crossing is `blurred_coverage`'s integral, taken
/// over one set of slices for the whole mesh: each slice's horizontal blur
/// is computed once per column and its Gaussian mass once per row, so a
/// value costs a multiply-add per nearby slice, not two CDFs.
pub(crate) fn build_mesh(
    shape: &Shape,
    other: &Shape,
    sigma: f32,
    inset: bool,
    origin: (f32, f32),
    detail: Detail,
) -> Mesh {
    let (xs, ys) = knots_of(shape, other, sigma, inset, detail);
    let values = mesh_values(shape, sigma, inset, &xs, &ys);
    let (ox, oy) = origin;
    Mesh {
        xs: xs.into_iter().map(|x| x - ox).collect(),
        ys: ys.into_iter().map(|y| y - oy).collect(),
        values,
    }
}

fn mesh_values(shape: &Shape, sigma: f32, inset: bool, xs: &[f32], ys: &[f32]) -> Vec<f32> {
    let nx = xs.len();
    let mut values = vec![0.0f32; nx * ys.len()];
    let r = shape.rect;
    if r.width > 0.0 && r.height > 0.0 && sigma > 0.0 {
        let slices = slices_of(shape, sigma);
        // across[s * nx + i]: slice s's span blurred along x, at column i.
        let mut across = vec![0.0f32; slices.len() * nx];
        for (s, (a, b)) in slices.iter().enumerate() {
            let (l, rt) = shape.span(0.5 * (a + b));
            if rt <= l {
                continue;
            }
            for (i, &x) in xs.iter().enumerate() {
                across[s * nx + i] = phi((rt - x) / sigma) - phi((l - x) / sigma);
            }
        }
        let window = 4.0 * sigma;
        for (j, &y) in ys.iter().enumerate() {
            let row = &mut values[j * nx..(j + 1) * nx];
            for (s, &(a, b)) in slices.iter().enumerate() {
                if b < y - window || a > y + window {
                    continue;
                }
                let mass = phi((b - y) / sigma) - phi((a - y) / sigma);
                if mass <= 0.0 {
                    continue;
                }
                for (v, c) in row.iter_mut().zip(&across[s * nx..(s + 1) * nx]) {
                    *v += mass * c;
                }
            }
        }
    }
    for v in &mut values {
        let c = v.clamp(0.0, 1.0);
        *v = if inset { 1.0 - c } else { c };
    }
    values
}

/// Fine mesh vertices `MeshCache` builds in one frame before it falls back
/// to coarse meshes: about 2 ms of building. The first build of a frame is always
/// fine, so one animated shadow stays fine.
pub(crate) const MESH_BUILD_BUDGET: usize = MESH_VERTICES_PER_MS * 2;

/// Fine mesh vertices built per millisecond (release build, M-series Mac:
/// 100k-160k for pill shapes at blur 8..64; the low end is taken).
const MESH_VERTICES_PER_MS: usize = 100_000;

/// Fine meshes kept; the least recently used goes first.
pub(crate) const MESH_CACHE_CAPACITY: usize = 256;

/// Shadow meshes by shape (`MeshKey`), least recently used evicted, with a
/// per-frame build budget. Past the budget a mesh is built coarse (a
/// sixteenth of the vertices) and not kept, so a later frame with budget
/// left builds it fine.
#[derive(Default)]
pub(crate) struct MeshCache {
    meshes: HashMap<MeshKey, (Arc<Mesh>, u64)>,
    tick: u64,
    spent: usize,
    /// Fine meshes built this frame.
    pub builds: usize,
    /// Coarse meshes built this frame (the budget was spent).
    pub coarse: usize,
}

impl MeshCache {
    /// A new frame: its budget is unspent.
    pub(crate) fn begin_frame(&mut self) {
        self.spent = 0;
        self.builds = 0;
        self.coarse = 0;
    }

    pub(crate) fn len(&self) -> usize {
        self.meshes.len()
    }

    /// The mesh for this shadow: cached, built fine within the budget, or
    /// built coarse past it.
    pub(crate) fn get(
        &mut self,
        shape: &Shape,
        other: &Shape,
        sigma: f32,
        inset: bool,
        origin: (f32, f32),
    ) -> Arc<Mesh> {
        self.tick += 1;
        let key = mesh_key(shape, other, sigma, inset, origin);
        if let Some((mesh, used)) = self.meshes.get_mut(&key) {
            *used = self.tick;
            return mesh.clone();
        }
        let (xs, ys) = knots_of(shape, other, sigma, inset, Detail::Fine);
        let cost = xs.len() * ys.len();
        if self.spent > 0 && self.spent + cost > MESH_BUILD_BUDGET {
            self.coarse += 1;
            return Arc::new(build_mesh(
                shape,
                other,
                sigma,
                inset,
                origin,
                Detail::Coarse,
            ));
        }
        self.spent += cost;
        self.builds += 1;
        let values = mesh_values(shape, sigma, inset, &xs, &ys);
        let (ox, oy) = origin;
        let mesh = Arc::new(Mesh {
            xs: xs.into_iter().map(|x| x - ox).collect(),
            ys: ys.into_iter().map(|y| y - oy).collect(),
            values,
        });
        if self.meshes.len() >= MESH_CACHE_CAPACITY {
            if let Some(oldest) = self
                .meshes
                .iter()
                .min_by_key(|(_, (_, used))| *used)
                .map(|(k, _)| k.clone())
            {
                self.meshes.remove(&oldest);
            }
        }
        self.meshes.insert(key, (mesh.clone(), self.tick));
        mesh
    }
}

/// The value at `(u, v)` (0..=1 across the cell) of the cell whose corners
/// are `c` (top-left, top-right, bottom-right, bottom-left).
pub(crate) fn bilinear(c: [f32; 4], u: f32, v: f32) -> f32 {
    let top = c[0] + (c[1] - c[0]) * u;
    let bottom = c[3] + (c[2] - c[3]) * u;
    top + (bottom - top) * v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f32, y: f32, w: f32, h: f32) -> Shape {
        Shape::new(Rect::new(x, y, w, h), BorderRadius::default())
    }

    /// A square shape's blur is the product of two 1-D edge profiles.
    #[test]
    fn a_square_shape_blurs_separably() {
        let s = square(0.0, 0.0, 100.0, 60.0);
        let sigma = 8.0;
        for (x, y) in [
            (-10.0, 30.0),
            (0.0, 0.0),
            (5.0, -3.0),
            (50.0, 30.0),
            (104.0, 62.0),
        ] {
            let fx = phi((100.0 - x) / sigma) - phi((0.0 - x) / sigma);
            let fy = phi((60.0 - y) / sigma) - phi((0.0 - y) / sigma);
            let got = blurred_coverage(&s, sigma, x, y);
            assert!(
                (got - fx * fy).abs() < 1e-3,
                "({x}, {y}): {got} vs {}",
                fx * fy
            );
        }
        // Half at an edge, a quarter at a corner.
        assert!((blurred_coverage(&s, sigma, 0.0, 30.0) - 0.5).abs() < 1e-3);
        assert!((blurred_coverage(&s, sigma, 0.0, 0.0) - 0.25).abs() < 1e-3);
    }

    /// A rounded corner is lighter than a square one, and a circle's blur
    /// is the same in every direction.
    #[test]
    fn a_round_corner_is_lighter_and_a_circle_is_round() {
        let sq = square(0.0, 0.0, 100.0, 100.0);
        let round = Shape::new(
            Rect::new(0.0, 0.0, 100.0, 100.0),
            BorderRadius {
                top_left: CornerRadius::circular(50.0),
                top_right: CornerRadius::circular(50.0),
                bottom_right: CornerRadius::circular(50.0),
                bottom_left: CornerRadius::circular(50.0),
            },
        );
        assert!(
            blurred_coverage(&round, 8.0, 5.0, 5.0) < blurred_coverage(&sq, 8.0, 5.0, 5.0) * 0.2
        );
        let d = 60.0;
        let east = blurred_coverage(&round, 8.0, 50.0 + d, 50.0);
        let diag = blurred_coverage(&round, 8.0, 50.0 + d * 0.707_106_8, 50.0 + d * 0.707_106_8);
        assert!((east - diag).abs() < 0.01, "east {east}, diagonal {diag}");
    }

    #[test]
    fn knots_cover_the_changing_stretches_and_skip_the_flat_middle() {
        let k = mesh_knots(
            -30.0,
            1030.0,
            [0.0, 0.0, 1000.0, 1000.0],
            30.0,
            &[],
            Detail::Fine,
        );
        assert_eq!(*k.first().unwrap(), -30.0);
        assert_eq!(*k.last().unwrap(), 1030.0);
        assert!(k
            .windows(2)
            .all(|w| w[1] - w[0] <= 3.34 || (w[0] >= 30.0 && w[1] <= 970.0)));
        assert!(k.len() < 60, "{} knots", k.len());
    }

    /// Two boxes of one shape anywhere share a mesh; a different blur or a
    /// different hole does not.
    #[test]
    fn a_mesh_is_keyed_by_shape_not_place() {
        let at = |x: f32, y: f32, blur: f32| {
            let shape = square(x, y, 100.0, 40.0);
            let hole = square(x + 4.0, y + 4.0, 92.0, 32.0);
            mesh_key(&shape, &hole, sigma_of(blur), false, (x, y))
        };
        assert_eq!(at(10.0, 20.0, 16.0), at(310.5, 900.25, 16.0));
        assert_ne!(at(10.0, 20.0, 16.0), at(10.0, 20.0, 32.0));
        let shape = square(0.0, 0.0, 100.0, 40.0);
        let moved_hole = square(6.0, 4.0, 92.0, 32.0);
        assert_ne!(
            at(0.0, 0.0, 16.0),
            mesh_key(&shape, &moved_hole, 8.0, false, (0.0, 0.0))
        );
        let mesh = build_mesh(&shape, &moved_hole, 8.0, false, (0.0, 0.0), Detail::Fine);
        assert_eq!(mesh.values.len(), mesh.xs.len() * mesh.ys.len());
        assert_eq!((mesh.xs[0], mesh.ys[0]), (-24.0, -24.0), "three sigma out");
    }

    fn pill(w: f32, h: f32) -> Shape {
        let r = CornerRadius::circular(h / 2.0);
        Shape::new(
            Rect::new(0.0, 0.0, w, h),
            BorderRadius {
                top_left: r,
                top_right: r,
                bottom_right: r,
                bottom_left: r,
            },
        )
    }

    /// The mesh's sliced integral is `blurred_coverage` at every knot.
    #[test]
    fn a_mesh_holds_the_blurred_coverage_at_its_knots() {
        for (shape, sigma) in [
            (pill(200.0, 56.0), 8.0),
            (pill(120.0, 80.0), 16.0),
            (square(0.0, 0.0, 90.0, 40.0), 2.0),
        ] {
            for inset in [false, true] {
                let hole = square(0.0, 0.0, 1.0, 1.0);
                let other = if inset {
                    square(-10.0, -10.0, 220.0, 100.0)
                } else {
                    hole
                };
                let mesh = build_mesh(&shape, &other, sigma, inset, (0.0, 0.0), Detail::Fine);
                for (j, y) in mesh.ys.iter().enumerate() {
                    for (i, x) in mesh.xs.iter().enumerate() {
                        let want = blurred_coverage(&shape, sigma, *x, *y);
                        let want = if inset { 1.0 - want } else { want };
                        let got = mesh.values[j * mesh.xs.len() + i];
                        assert!(
                            (got - want).abs() < 2e-3,
                            "({x}, {y}) sigma {sigma}: {got} vs {want}"
                        );
                    }
                }
            }
        }
    }

    /// An infinite or NaN blur draws finite positions.
    #[test]
    fn an_infinite_blur_is_clamped() {
        assert_eq!(sigma_of(f32::INFINITY), MAX_SIGMA);
        assert_eq!(sigma_of(f32::NAN), 0.0);
        assert_eq!(sigma_of(-4.0), 0.0);
        let mut cache = MeshCache::default();
        let mesh = cache.get(
            &pill(100.0, 40.0),
            &pill(100.0, 40.0),
            sigma_of(f32::INFINITY),
            false,
            (0.0, 0.0),
        );
        assert!(mesh
            .xs
            .iter()
            .chain(&mesh.ys)
            .chain(&mesh.values)
            .all(|v| v.is_finite()));
    }

    /// 300 distinct shapes, drawn every frame: no frame builds past the
    /// budget (the first build excepted), the rest are coarse, and every
    /// shape is drawn.
    #[test]
    fn three_hundred_shapes_stay_within_the_build_budget() {
        let mut cache = MeshCache::default();
        let shapes: Vec<Shape> = (0..300).map(|i| pill(100.0 + i as f32, 40.0)).collect();
        let mut fine_vertices_max = 0;
        for _frame in 0..10 {
            cache.begin_frame();
            for s in &shapes {
                let m = cache.get(s, s, 8.0, false, (0.0, 0.0));
                assert!(!m.values.is_empty());
            }
            assert_eq!(cache.builds + cache.coarse <= 300, true);
            fine_vertices_max = fine_vertices_max.max(cache.spent);
            let one = {
                let (xs, ys) = knots_of(&shapes[0], &shapes[0], 8.0, false, Detail::Fine);
                xs.len() * ys.len()
            };
            assert!(
                cache.spent <= MESH_BUILD_BUDGET.max(one),
                "spent {} of {}",
                cache.spent,
                MESH_BUILD_BUDGET
            );
            assert!(cache.builds >= 1, "every frame makes progress");
            assert!(cache.len() <= MESH_CACHE_CAPACITY);
        }
        assert!(fine_vertices_max > 0);
    }

    /// A static page of a few shapes builds each once, then hits.
    #[test]
    fn a_static_page_builds_once() {
        let mut cache = MeshCache::default();
        let shapes: Vec<Shape> = (0..5).map(|i| pill(100.0 + i as f32, 40.0)).collect();
        cache.begin_frame();
        for s in &shapes {
            cache.get(s, s, 8.0, false, (0.0, 0.0));
        }
        assert_eq!((cache.builds, cache.coarse), (5, 0));
        for _ in 0..3 {
            cache.begin_frame();
            for s in &shapes {
                cache.get(s, s, 8.0, false, (0.0, 0.0));
            }
            assert_eq!((cache.builds, cache.coarse), (0, 0));
        }
    }

    /// An animating blur misses every frame: one fine build a frame, never
    /// coarse, and the cache does not grow past its capacity.
    #[test]
    fn an_animating_blur_builds_one_mesh_a_frame() {
        let mut cache = MeshCache::default();
        let s = pill(200.0, 56.0);
        for frame in 0..400 {
            cache.begin_frame();
            cache.get(&s, &s, 4.0 + frame as f32 * 0.05, false, (0.0, 0.0));
            assert_eq!((cache.builds, cache.coarse), (1, 0), "frame {frame}");
            assert!(cache.len() <= MESH_CACHE_CAPACITY);
        }
    }

    /// Least recently used goes first: a shape drawn every frame survives
    /// a stream of one-off shapes.
    #[test]
    fn the_cache_evicts_the_least_recently_used() {
        let mut cache = MeshCache::default();
        let kept = pill(77.0, 30.0);
        for i in 0..(MESH_CACHE_CAPACITY * 2) {
            cache.begin_frame();
            cache.get(&kept, &kept, 8.0, false, (0.0, 0.0));
            let other = pill(300.0 + i as f32, 40.0);
            cache.get(&other, &other, 8.0, false, (0.0, 0.0));
        }
        cache.begin_frame();
        cache.get(&kept, &kept, 8.0, false, (0.0, 0.0));
        assert_eq!(cache.builds, 0, "the shape in use was evicted");
    }
}
