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

use rustkit_layout::{BorderRadius, CornerRadius, Rect};

/// The Gaussian's standard deviation for a CSS blur radius.
pub(crate) fn sigma_of(blur_radius: f32) -> f32 {
    blur_radius.max(0.0) * 0.5
}

/// How far past the shape the blur is drawn: three standard deviations,
/// where a full-alpha shadow is a fifth of a level from nothing.
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
            [r.y, r.y + v(tl).max(v(tr)), r.bottom() - v(bl).max(v(br)), r.bottom()]
        } else {
            [r.x, r.x + h(tl).max(h(bl)), r.right() - h(tr).max(h(br)), r.right()]
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
    let mut cuts: Vec<f32> = shape.breaks(true).into_iter().filter(|c| *c > lo && *c < hi).collect();
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
) -> Vec<f32> {
    let mut knots = vec![lo, hi];
    if hi <= lo {
        return knots;
    }
    // Two stretches, near and far edge, merged if they meet.
    let mut stretches = vec![(breaks[0] - reach, breaks[1] + reach), (breaks[2] - reach, breaks[3] + reach)];
    if stretches[0].1 >= stretches[1].0 {
        stretches = vec![(stretches[0].0, stretches[1].1)];
    }
    let total: f32 = stretches.iter().map(|(a, b)| (b.min(hi) - a.max(lo)).max(0.0)).sum();
    // Half a standard deviation (reach / 6) keeps linear interpolation
    // within about a percent of the alpha; no finer than a pixel; no more
    // than MAX_KNOTS.
    let step = (reach / 6.0).max(1.0).max(total / MAX_KNOTS as f32);
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
    [c[0].h, c[0].v, c[1].h, c[1].v, c[2].h, c[2].v, c[3].h, c[3].v]
}

/// The mesh of an outer shadow (`inset` false: `shape` blurred, over the
/// shape grown by the reach) or an inset one (`inset` true: one minus
/// `shape` blurred, over `other`, the clip). `other` adds its own edges to
/// the cuts. Positions are relative to `origin`.
pub(crate) fn mesh_key(shape: &Shape, other: &Shape, sigma: f32, inset: bool, origin: (f32, f32)) -> MeshKey {
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

/// Build the mesh `mesh_key` describes, relative to `origin`.
pub(crate) fn build_mesh(shape: &Shape, other: &Shape, sigma: f32, inset: bool, origin: (f32, f32)) -> Mesh {
    let reach = reach_of(sigma);
    let (ox, oy) = origin;
    let (x0, x1, y0, y1) = if inset {
        (other.rect.x, other.rect.right(), other.rect.y, other.rect.bottom())
    } else {
        (
            shape.rect.x - reach,
            shape.rect.right() + reach,
            shape.rect.y - reach,
            shape.rect.bottom() + reach,
        )
    };
    let xs = mesh_knots(x0, x1, shape.breaks(false), reach, &other.breaks(false));
    let ys = mesh_knots(y0, y1, shape.breaks(true), reach, &other.breaks(true));
    let mut values = Vec::with_capacity(xs.len() * ys.len());
    for &y in &ys {
        for &x in &xs {
            let v = blurred_coverage(shape, sigma, x, y);
            values.push(if inset { 1.0 - v } else { v });
        }
    }
    Mesh {
        xs: xs.into_iter().map(|x| x - ox).collect(),
        ys: ys.into_iter().map(|y| y - oy).collect(),
        values,
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
        for (x, y) in [(-10.0, 30.0), (0.0, 0.0), (5.0, -3.0), (50.0, 30.0), (104.0, 62.0)] {
            let fx = phi((100.0 - x) / sigma) - phi((0.0 - x) / sigma);
            let fy = phi((60.0 - y) / sigma) - phi((0.0 - y) / sigma);
            let got = blurred_coverage(&s, sigma, x, y);
            assert!((got - fx * fy).abs() < 1e-3, "({x}, {y}): {got} vs {}", fx * fy);
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
        let round = Shape::new(Rect::new(0.0, 0.0, 100.0, 100.0), BorderRadius {
            top_left: CornerRadius::circular(50.0),
            top_right: CornerRadius::circular(50.0),
            bottom_right: CornerRadius::circular(50.0),
            bottom_left: CornerRadius::circular(50.0),
        });
        assert!(blurred_coverage(&round, 8.0, 5.0, 5.0) < blurred_coverage(&sq, 8.0, 5.0, 5.0) * 0.2);
        let d = 60.0;
        let east = blurred_coverage(&round, 8.0, 50.0 + d, 50.0);
        let diag = blurred_coverage(&round, 8.0, 50.0 + d * 0.707_106_8, 50.0 + d * 0.707_106_8);
        assert!((east - diag).abs() < 0.01, "east {east}, diagonal {diag}");
    }

    #[test]
    fn knots_cover_the_changing_stretches_and_skip_the_flat_middle() {
        let k = mesh_knots(-30.0, 1030.0, [0.0, 0.0, 1000.0, 1000.0], 30.0, &[]);
        assert_eq!(*k.first().unwrap(), -30.0);
        assert_eq!(*k.last().unwrap(), 1030.0);
        assert!(k.windows(2).all(|w| w[1] - w[0] <= 5.0 + 1e-3 || (w[0] >= 30.0 && w[1] <= 970.0)));
        assert!(k.len() < 40, "{} knots", k.len());
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
        assert_ne!(at(0.0, 0.0, 16.0), mesh_key(&shape, &moved_hole, 8.0, false, (0.0, 0.0)));
        let mesh = build_mesh(&shape, &moved_hole, 8.0, false, (0.0, 0.0));
        assert_eq!(mesh.values.len(), mesh.xs.len() * mesh.ys.len());
        assert_eq!((mesh.xs[0], mesh.ys[0]), (-24.0, -24.0), "three sigma out");
    }
}
