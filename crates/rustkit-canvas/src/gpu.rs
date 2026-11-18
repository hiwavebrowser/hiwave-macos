//! GPU canvas A1 (docs/design/GPU_CANVAS_2D.md, "Plan of record: option
//! A"): the recording handed to the presenter, which replays it into a
//! texture per canvas, and the lowering of each op into what the GPU draws
//! (device-space triangles for a stencil-then-cover fill, a colour, a
//! blend, the clip chain), done on the presenter's thread.
//!
//! Behind `HIWAVE_GPU_CANVAS=1` (or [`set_gpu_canvas`] on a thread), read
//! when a context is created; off (the default), nothing here runs.
//!
//! What the GPU replay draws at A1: fills and strokes with a solid colour,
//! clears, `putImageData`, under the composite operations fixed-function
//! blending can do (source-over, the Porter-Duff set, lighter, copy, xor)
//! and under a clip chain of at most 32 `clip()`s. Anything else (a
//! gradient or pattern style, `drawImage`, the blend modes that read the
//! destination) makes the batch a reset: the CPU bitmap is brought up to
//! date and uploaded, today's path, for that frame.

use crate::raster::{build_path, skia_stroke, transform, ClipLink, Op};
use crate::{CanvasStyle, CompositeOperation, PathCommand};
use std::sync::{Arc, OnceLock};
use tiny_skia::PathSegment;

thread_local! {
    static OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Whether a context created now on this thread keeps a GPU picture: the
/// thread's [`set_gpu_canvas`] if set, else `HIWAVE_GPU_CANVAS=1`.
pub fn gpu_canvas_enabled() -> bool {
    static ENV: OnceLock<bool> = OnceLock::new();
    OVERRIDE.with(|o| o.get()).unwrap_or_else(|| *ENV.get_or_init(|| std::env::var("HIWAVE_GPU_CANVAS").is_ok_and(|v| v == "1")))
}

/// Tests: the switch for contexts created on this thread from now on
/// (`None`: back to the environment).
pub fn set_gpu_canvas(on: Option<bool>) {
    OVERRIDE.with(|o| o.set(on));
}

/// What the GPU picture of a canvas starts over from.
#[derive(Debug, Clone)]
pub enum GpuReset {
    /// Transparent black (nothing drawn yet).
    Blank,
    /// The CPU bitmap, premultiplied RGBA rows.
    Pixels(Vec<u8>),
}

/// The ops of one canvas handed from the page thread to the presenter at
/// one display-list build: the ops since the last batch, or a reset.
#[derive(Debug, Clone)]
pub struct GpuBatch {
    pub width: u32,
    pub height: u32,
    /// Set: the texture starts over from this before the batch's ops (a
    /// reset from the bitmap holds no ops).
    pub reset: Option<GpuReset>,
    ops: Vec<Arc<Op>>,
}

/// The composite operations the GPU replay draws, as fixed-function
/// blends of premultiplied colour (`Clear` is a clearRect).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GpuBlend {
    Clear,
    SourceOver,
    SourceIn,
    SourceOut,
    SourceAtop,
    DestinationOver,
    DestinationIn,
    DestinationOut,
    DestinationAtop,
    Lighter,
    Copy,
    Xor,
}

impl GpuBlend {
    fn of(op: CompositeOperation) -> Option<Self> {
        Some(match op {
            CompositeOperation::SourceOver => Self::SourceOver,
            CompositeOperation::SourceIn => Self::SourceIn,
            CompositeOperation::SourceOut => Self::SourceOut,
            CompositeOperation::SourceAtop => Self::SourceAtop,
            CompositeOperation::DestinationOver => Self::DestinationOver,
            CompositeOperation::DestinationIn => Self::DestinationIn,
            CompositeOperation::DestinationOut => Self::DestinationOut,
            CompositeOperation::DestinationAtop => Self::DestinationAtop,
            CompositeOperation::Lighter => Self::Lighter,
            CompositeOperation::Copy => Self::Copy,
            CompositeOperation::Xor => Self::Xor,
            CompositeOperation::Multiply
            | CompositeOperation::Screen
            | CompositeOperation::Overlay
            | CompositeOperation::Darken
            | CompositeOperation::Lighten => return None,
        })
    }
}

/// A path as the GPU fills it: a triangle fan per contour (device pixels),
/// drawn into the stencil, then its bounds covered where the stencil says
/// inside.
#[derive(Debug, Clone, Default)]
pub struct GpuPath {
    pub tris: Vec<[f32; 2]>,
    /// `[x0, y0, x1, y1]`, device pixels.
    pub bounds: [f32; 4],
    pub even_odd: bool,
}

/// A clip chain: the stencil's clip bit is the intersection of its links.
#[derive(Debug)]
pub struct GpuClip {
    /// Unique while the batch that lowered it is alive.
    pub id: usize,
    pub links: Vec<GpuPath>,
}

/// One thing the GPU replay draws.
#[derive(Debug, Clone)]
pub enum GpuDraw {
    /// `path` covered with the premultiplied `color` under `blend`.
    Fill { path: GpuPath, color: [f32; 4], blend: GpuBlend, clip: Option<Arc<GpuClip>> },
    /// putImageData: premultiplied rows written as they are.
    Put { x: u32, y: u32, width: u32, height: u32, rgba: Vec<u8> },
}

/// Whether the GPU replay draws `op` at A1.
pub(crate) fn supported(op: &Op) -> bool {
    let clip_ok = |c: Option<&Arc<crate::raster::Clip>>| c.is_none_or(|c| c.chain.is_some());
    match op {
        Op::Fill { d, style, .. } | Op::Stroke { d, style, .. } => {
            matches!(style, CanvasStyle::Color(_)) && GpuBlend::of(d.op).is_some() && clip_ok(d.clip.as_ref())
        }
        Op::Clear { clip, .. } => clip_ok(clip.as_ref()),
        Op::Put { .. } => true,
        Op::Image { .. } => false,
    }
}

/// Curves are cut into lines no further than this from the curve, in
/// device pixels.
const TOLERANCE: f32 = 0.2;

impl GpuBatch {
    pub(crate) fn reset(size: (u32, u32), pixels: Option<Vec<u8>>) -> Self {
        let reset = match pixels {
            Some(p) => GpuReset::Pixels(p),
            None => GpuReset::Blank,
        };
        Self { width: size.0, height: size.1, reset: Some(reset), ops: Vec::new() }
    }

    pub(crate) fn ops(size: (u32, u32), ops: Vec<Arc<Op>>) -> Self {
        Self { width: size.0, height: size.1, reset: None, ops }
    }

    /// Ops in the batch.
    pub fn op_count(&self) -> usize {
        self.ops.len()
    }

    /// The bytes the batch holds alone, estimated (ops and reset pixels;
    /// what ops share with the recording is not counted).
    pub fn bytes(&self) -> usize {
        let reset = match &self.reset {
            Some(GpuReset::Pixels(p)) => p.len(),
            _ => 0,
        };
        reset + self.ops.iter().map(|op| op.bytes()).sum::<usize>()
    }

    /// The batch's ops as the GPU draws them, in order. Run on the
    /// presenter's thread: this is the path building, stroking and curve
    /// flattening the page thread no longer does.
    pub fn lower(&self) -> Vec<GpuDraw> {
        if true {
            return Vec::new(); // RED: stub
        }
        let mut clips: std::collections::HashMap<usize, Arc<GpuClip>> = Default::default();
        let mut clip_of = |c: Option<&Arc<crate::raster::Clip>>| -> Option<Arc<GpuClip>> {
            let c = c?;
            let chain = c.chain.as_ref()?;
            let id = Arc::as_ptr(c) as usize;
            Some(
                clips
                    .entry(id)
                    .or_insert_with(|| Arc::new(GpuClip { id, links: chain.iter().map(|l| lower_link(l)).collect() }))
                    .clone(),
            )
        };
        let mut out = Vec::with_capacity(self.ops.len());
        for op in &self.ops {
            match &**op {
                Op::Fill { d, path, even_odd, style } => {
                    let (Some(blend), Some(color)) = (GpuBlend::of(d.op), solid(style)) else { continue };
                    let Some(p) = build_path(path).and_then(|p| p.transform(transform(&d.t))) else { continue };
                    let path = flatten(&p, *even_odd);
                    if path.tris.is_empty() {
                        continue;
                    }
                    out.push(GpuDraw::Fill { path, color, blend, clip: clip_of(d.clip.as_ref()) });
                }
                Op::Stroke { d, path, line, style } => {
                    let (Some(blend), Some(color)) = (GpuBlend::of(d.op), solid(style)) else { continue };
                    let Some(p) = build_path(path) else { continue };
                    let ts = transform(&d.t);
                    let res_scale = tiny_skia::PathStroker::compute_resolution_scale(&ts);
                    let Some(outline) = p.stroke(&skia_stroke(&line.style()), res_scale).and_then(|o| o.transform(ts)) else {
                        continue;
                    };
                    let path = flatten(&outline, false);
                    if path.tris.is_empty() {
                        continue;
                    }
                    out.push(GpuDraw::Fill { path, color, blend, clip: clip_of(d.clip.as_ref()) });
                }
                Op::Clear { clip, rect, t } => {
                    let [x, y, w, h] = *rect;
                    let path = if t.b == 0.0 && t.c == 0.0 {
                        // Whole pixels, as the CPU's non-anti-aliased clear:
                        // tiny-skia rounds a rect's edges to the nearest
                        // pixel (halves up).
                        let (ax, ay) = t.apply(x, y);
                        let (bx, by) = t.apply(x + w, y + h);
                        let snap = |v: f32| v.round();
                        let r = [snap(ax.min(bx)), snap(ay.min(by)), snap(ax.max(bx)), snap(ay.max(by))];
                        if r[0] >= r[2] || r[1] >= r[3] {
                            continue;
                        }
                        rect_path(r)
                    } else {
                        let Some(p) = build_path(&[PathCommand::Rect(x, y, w, h)]).and_then(|p| p.transform(transform(t))) else {
                            continue;
                        };
                        flatten(&p, false)
                    };
                    out.push(GpuDraw::Fill { path, color: [0.0; 4], blend: GpuBlend::Clear, clip: clip_of(clip.as_ref()) });
                }
                Op::Put { rgba, width, dx, dy, .. } => {
                    if *width == 0 || *dx < 0 || *dy < 0 {
                        continue;
                    }
                    let height = (rgba.len() / 4 / *width as usize) as u32;
                    let mut px = Vec::with_capacity(rgba.len());
                    for p in rgba.chunks_exact(4) {
                        let c = tiny_skia::ColorU8::from_rgba(p[0], p[1], p[2], p[3]).premultiply();
                        px.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
                    }
                    out.push(GpuDraw::Put { x: *dx as u32, y: *dy as u32, width: *width, height, rgba: px });
                }
                Op::Image { .. } => {}
            }
        }
        out
    }
}

fn lower_link(l: &ClipLink) -> GpuPath {
    match build_path(&l.path).and_then(|p| p.transform(transform(&l.t))) {
        Some(p) => flatten(&p, l.even_odd),
        // No segments: the clip keeps nothing.
        None => GpuPath { tris: Vec::new(), bounds: [0.0; 4], even_odd: l.even_odd },
    }
}

/// A solid style's premultiplied colour (globalAlpha is already in it).
fn solid(style: &CanvasStyle) -> Option<[f32; 4]> {
    let CanvasStyle::Color(c) = style else { return None };
    let a = (c.a.clamp(0.0, 1.0) * 255.0).round() / 255.0;
    let k = |v: u8| v as f32 / 255.0 * a;
    Some([k(c.r), k(c.g), k(c.b), a])
}

fn rect_path(r: [f32; 4]) -> GpuPath {
    let [x0, y0, x1, y1] = r;
    GpuPath { tris: vec![[x0, y0], [x1, y0], [x1, y1], [x0, y0], [x1, y1], [x0, y1]], bounds: r, even_odd: false }
}

/// `path` (device space) as triangle fans, one per contour, curves cut to
/// lines within [`TOLERANCE`].
fn flatten(path: &tiny_skia::Path, even_odd: bool) -> GpuPath {
    let mut tris = Vec::new();
    let mut contour: Vec<[f32; 2]> = Vec::new();
    let mut bounds = [f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];
    let fan = |contour: &mut Vec<[f32; 2]>, tris: &mut Vec<[f32; 2]>| {
        if contour.len() >= 3 {
            let p0 = contour[0];
            for w in contour[1..].windows(2) {
                tris.extend_from_slice(&[p0, w[0], w[1]]);
            }
        }
        contour.clear();
    };
    let mut last = [0.0f32; 2];
    for seg in path.segments() {
        match seg {
            PathSegment::MoveTo(p) => {
                fan(&mut contour, &mut tris);
                last = [p.x, p.y];
                contour.push(last);
            }
            PathSegment::LineTo(p) => {
                last = [p.x, p.y];
                contour.push(last);
            }
            PathSegment::QuadTo(c, p) => {
                let (p0, c, p1) = (last, [c.x, c.y], [p.x, p.y]);
                let dd = len([p0[0] - 2.0 * c[0] + p1[0], p0[1] - 2.0 * c[1] + p1[1]]);
                let n = steps((dd / (8.0 * TOLERANCE)).sqrt());
                for i in 1..=n {
                    let t = i as f32 / n as f32;
                    let u = 1.0 - t;
                    contour.push([
                        u * u * p0[0] + 2.0 * u * t * c[0] + t * t * p1[0],
                        u * u * p0[1] + 2.0 * u * t * c[1] + t * t * p1[1],
                    ]);
                }
                last = p1;
            }
            PathSegment::CubicTo(c1, c2, p) => {
                let (p0, c1, c2, p1) = (last, [c1.x, c1.y], [c2.x, c2.y], [p.x, p.y]);
                let d1 = len([p0[0] - 2.0 * c1[0] + c2[0], p0[1] - 2.0 * c1[1] + c2[1]]);
                let d2 = len([c1[0] - 2.0 * c2[0] + p1[0], c1[1] - 2.0 * c2[1] + p1[1]]);
                let n = steps((3.0 * d1.max(d2) / (4.0 * TOLERANCE)).sqrt());
                for i in 1..=n {
                    let t = i as f32 / n as f32;
                    let u = 1.0 - t;
                    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                    contour.push([
                        a * p0[0] + b * c1[0] + c * c2[0] + d * p1[0],
                        a * p0[1] + b * c1[1] + c * c2[1] + d * p1[1],
                    ]);
                }
                last = p1;
            }
            PathSegment::Close => {}
        }
    }
    fan(&mut contour, &mut tris);
    for p in &tris {
        bounds = [bounds[0].min(p[0]), bounds[1].min(p[1]), bounds[2].max(p[0]), bounds[3].max(p[1])];
    }
    if tris.is_empty() {
        bounds = [0.0; 4];
    }
    GpuPath { tris, bounds, even_odd }
}

fn len(v: [f32; 2]) -> f32 {
    (v[0] * v[0] + v[1] * v[1]).sqrt()
}

fn steps(n: f32) -> usize {
    if n.is_finite() {
        (n.ceil() as usize).clamp(1, 256)
    } else {
        1
    }
}

/// How far one picture is from another (A3's hook; not a gate at A1): two
/// `width` x `height` premultiplied RGBA images, unpremultiplied and
/// compared per pixel. A pixel is *interior* when in `reference` it and its
/// 8 neighbours are all the same colour (A3's rule says all alpha 255 or
/// all alpha 0; that counts the edge of one opaque shape drawn over another
/// as interior, so the colour is compared too); every other pixel is an
/// *edge* pixel.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PixelDiff {
    pub pixels: usize,
    /// Pixels whose largest channel difference is over the tolerance.
    pub differing: usize,
    pub interior_over: usize,
    pub edge_pixels: usize,
    pub edge_over: usize,
    /// Mean |Δα| over edge pixels.
    pub edge_mean_alpha: f64,
    /// The largest channel difference anywhere.
    pub max: u8,
    /// |Σα − Σα_ref| / Σα_ref (0 when both are empty).
    pub coverage_delta: f64,
}

impl PixelDiff {
    pub fn differing_pct(&self) -> f64 {
        if self.pixels == 0 {
            0.0
        } else {
            100.0 * self.differing as f64 / self.pixels as f64
        }
    }
}

/// [`PixelDiff`] of `image` against `reference`, `tolerance` per channel.
pub fn compare_premultiplied(image: &[u8], reference: &[u8], width: u32, height: u32, tolerance: u8) -> PixelDiff {
    let (w, h) = (width as usize, height as usize);
    let mut d = PixelDiff { pixels: w * h, ..Default::default() };
    if image.len() != w * h * 4 || reference.len() != w * h * 4 {
        d.differing = d.pixels;
        d.max = 255;
        return d;
    }
    let un = |p: &[u8]| crate::raster::unpremultiply_px(p[0], p[1], p[2], p[3]);
    let px = |x: usize, y: usize| &reference[(y * w + x) * 4..(y * w + x) * 4 + 4];
    let (mut sum_a, mut sum_r, mut edge_sum) = (0u64, 0u64, 0u64);
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let (a, r) = (un(&image[i..i + 4]), un(&reference[i..i + 4]));
            let m = (0..4).map(|c| a[c].abs_diff(r[c])).max().unwrap_or(0);
            sum_a += a[3] as u64;
            sum_r += r[3] as u64;
            d.max = d.max.max(m);
            let over = m > tolerance;
            d.differing += over as usize;
            let mut flat = true;
            for yy in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                for xx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                    flat &= px(xx, yy) == px(x, y);
                }
            }
            if flat {
                d.interior_over += over as usize;
            } else {
                d.edge_pixels += 1;
                d.edge_over += over as usize;
                edge_sum += a[3].abs_diff(r[3]) as u64;
            }
        }
    }
    if d.edge_pixels > 0 {
        d.edge_mean_alpha = edge_sum as f64 / d.edge_pixels as f64;
    }
    if sum_r > 0 {
        d.coverage_delta = (sum_a as f64 - sum_r as f64).abs() / sum_r as f64;
    } else if sum_a > 0 {
        d.coverage_delta = 1.0;
    }
    d
}

#[cfg(test)]
mod gpu_recording_tests {
    use super::*;
    use crate::CanvasRenderingContext2D;

    fn gpu_ctx(w: u32, h: u32) -> CanvasRenderingContext2D {
        set_gpu_canvas(Some(true));
        let c = CanvasRenderingContext2D::new(w, h);
        set_gpu_canvas(None);
        c
    }

    /// A read brings the bitmap up to date but keeps the ops the GPU has
    /// not taken; once both passed them they go.
    #[test]
    fn an_op_is_dropped_only_when_both_replays_passed_it() {
        let mut c = gpu_ctx(20, 20);
        assert!(c.gpu_canvas());
        let first = c.take_gpu_batch(false);
        assert!(matches!(first.reset, Some(GpuReset::Blank)), "a new context's first batch starts the texture blank");
        c.fill_rect(1.0, 1.0, 5.0, 5.0);
        c.stroke_rect(2.0, 2.0, 6.0, 6.0);
        assert_eq!(c.gpu_pending_ops(), 2);
        let _ = c.premultiplied_pixels();
        assert_eq!(c.pending_ops(), 0, "the CPU replayed them");
        assert_eq!(c.gpu_pending_ops(), 2, "the GPU has not taken them: kept");
        let b = c.take_gpu_batch(false);
        assert!(b.reset.is_none());
        assert_eq!(b.op_count(), 2);
        assert_eq!(c.gpu_pending_ops(), 0);
        assert_eq!(c.pending_bytes(), 0, "both passed: dropped");
        // The other order: the GPU first, then a read.
        c.fill_rect(0.0, 0.0, 3.0, 3.0);
        assert_eq!(c.take_gpu_batch(false).op_count(), 1);
        assert_eq!(c.pending_ops(), 1, "the CPU has not replayed it: kept");
        let _ = c.premultiplied_pixels();
        assert_eq!(c.pending_bytes(), 0);
    }

    /// The pixels a read sees are the same whatever the GPU took, and the
    /// same as a context without the switch.
    #[test]
    fn reads_are_byte_identical_with_the_gpu_position() {
        let draw = |c: &mut CanvasRenderingContext2D, take: bool| {
            c.set_fill_style(CanvasStyle::Color(rustkit_css::Color::new(200, 30, 40, 0.7)));
            c.fill_rect(1.5, 2.5, 9.0, 7.0);
            if take {
                c.take_gpu_batch(false);
            }
            c.begin_path();
            c.arc(10.0, 10.0, 6.0, 0.0, 5.0, false);
            c.clip();
            c.set_line_width(3.0);
            c.stroke_rect(3.0, 3.0, 12.0, 12.0);
            c.clear_rect(4.0, 4.0, 2.0, 2.0);
            c.premultiplied_pixels()
        };
        let plain = draw(&mut CanvasRenderingContext2D::new(20, 20), false);
        assert_eq!(draw(&mut gpu_ctx(20, 20), true), plain);
        assert_eq!(draw(&mut gpu_ctx(20, 20), false), plain);
    }

    /// A clear of the whole bitmap drops what came before it in both
    /// pictures (MotionMark clears every frame: the CPU never replays a
    /// frame nobody reads), and a read after it still sees what it should.
    #[test]
    fn a_full_clear_drops_everything_before_it_for_both() {
        let mut c = gpu_ctx(16, 16);
        c.take_gpu_batch(false);
        for i in 0..50 {
            c.fill_rect(i as f32 % 10.0, 0.0, 4.0, 4.0);
        }
        c.clear_rect(0.0, 0.0, 16.0, 16.0);
        assert_eq!(c.gpu_pending_ops(), 1, "only the clear is left for the GPU");
        assert_eq!(c.pending_ops(), 1);
        c.fill_rect(2.0, 2.0, 3.0, 3.0);
        let mut plain = CanvasRenderingContext2D::new(16, 16);
        for i in 0..50 {
            plain.fill_rect(i as f32 % 10.0, 0.0, 4.0, 4.0);
        }
        plain.clear_rect(0.0, 0.0, 16.0, 16.0);
        plain.fill_rect(2.0, 2.0, 3.0, 3.0);
        assert_eq!(c.premultiplied_pixels(), plain.premultiplied_pixels());
        // A clear under a clip is not a full clear.
        let mut c = gpu_ctx(16, 16);
        c.take_gpu_batch(false);
        c.fill_rect(0.0, 0.0, 4.0, 4.0);
        c.begin_path();
        c.rect(0.0, 0.0, 8.0, 8.0);
        c.clip();
        c.clear_rect(0.0, 0.0, 16.0, 16.0);
        assert_eq!(c.gpu_pending_ops(), 2);
    }

    /// Past the cap with the GPU behind, the CPU replays and drops as at
    /// A0, and the GPU's next batch is a reset from the bitmap.
    #[test]
    fn the_gpu_behind_past_the_cap_starts_over_from_the_bitmap() {
        let mut c = gpu_ctx(64, 64);
        c.take_gpu_batch(false);
        let mut n = 0;
        while c.gpu_pending_ops() == n {
            c.fill_rect((n % 50) as f32, 1.0, 3.0, 3.0);
            n += 1;
            assert!(n < 1_000_000, "the cap never tripped");
        }
        assert!(c.pending_bytes() <= crate::PENDING_BYTE_BUDGET);
        let b = c.take_gpu_batch(false);
        let Some(GpuReset::Pixels(px)) = &b.reset else { panic!("a reset with the bitmap") };
        assert_eq!(px, &c.premultiplied_pixels());
    }

    /// A new context's first batch starts the texture blank and carries
    /// its draws: nothing is rasterized on the CPU for a first paint, even
    /// when the presenter asks for a reset then. A resize starts blank too.
    #[test]
    fn a_new_or_resized_context_starts_blank_with_its_draws() {
        let mut c = gpu_ctx(8, 8);
        c.fill_rect(0.0, 0.0, 2.0, 2.0);
        let b = c.take_gpu_batch(true);
        assert!(matches!(b.reset, Some(GpuReset::Blank)));
        assert_eq!(b.op_count(), 1);
        assert_eq!(c.pending_ops(), 1, "not rasterized on the CPU");
        c.fill_rect(1.0, 1.0, 2.0, 2.0);
        let b = c.take_gpu_batch(false);
        assert!(b.reset.is_none() && b.op_count() == 1);
        c.reset_bitmap(16, 16);
        c.fill_rect(1.0, 1.0, 2.0, 2.0);
        let b = c.take_gpu_batch(false);
        assert!(matches!(b.reset, Some(GpuReset::Blank)));
        assert_eq!((b.width, b.height, b.op_count()), (16, 16, 1));
    }

    /// An op the GPU does not draw at A1 makes the batch a reset; a forced
    /// reset (the presenter lost the texture) too.
    #[test]
    fn an_op_the_gpu_does_not_draw_makes_a_reset() {
        let mut c = gpu_ctx(8, 8);
        c.take_gpu_batch(false);
        c.fill_rect(0.0, 0.0, 2.0, 2.0);
        c.set_global_composite_operation(CompositeOperation::Multiply);
        c.fill_rect(1.0, 1.0, 2.0, 2.0);
        assert!(matches!(c.take_gpu_batch(false).reset, Some(GpuReset::Pixels(_))));
        c.set_global_composite_operation(CompositeOperation::SourceOver);
        c.fill_rect(0.0, 0.0, 2.0, 2.0);
        let b = c.take_gpu_batch(true);
        assert!(b.reset.is_some() && b.op_count() == 0);
        // A gradient too.
        c.set_fill_style(CanvasStyle::LinearGradient(crate::LinearGradient::new(0.0, 0.0, 8.0, 0.0)));
        c.fill_rect(0.0, 0.0, 2.0, 2.0);
        assert!(c.take_gpu_batch(false).reset.is_some());
    }

    /// Off (the default), a context keeps no GPU position: nothing is
    /// kept for it and a read drops everything, as at A0.
    #[test]
    fn with_the_switch_off_nothing_is_kept_for_the_gpu() {
        set_gpu_canvas(Some(false));
        let mut c = CanvasRenderingContext2D::new(8, 8);
        set_gpu_canvas(None);
        assert!(!c.gpu_canvas());
        c.fill_rect(0.0, 0.0, 2.0, 2.0);
        assert_eq!(c.gpu_pending_ops(), 0);
        let _ = c.premultiplied_pixels();
        assert_eq!(c.pending_bytes(), 0);
    }

    /// The lowering: a filled rect is two triangles covering it; a stroke
    /// becomes its outline; a clip chain is carried; curves are flattened.
    #[test]
    fn ops_lower_to_device_space_fans() {
        let mut c = gpu_ctx(40, 40);
        c.take_gpu_batch(false);
        c.translate(10.0, 5.0);
        c.fill_rect(0.0, 0.0, 4.0, 3.0);
        c.begin_path();
        c.rect(0.0, 0.0, 20.0, 20.0);
        c.clip();
        c.begin_path();
        c.arc(10.0, 10.0, 8.0, 0.0, std::f32::consts::TAU, false);
        c.stroke();
        let draws = c.take_gpu_batch(false).lower();
        assert_eq!(draws.len(), 2);
        let GpuDraw::Fill { path, clip, blend, .. } = &draws[0] else { panic!() };
        assert_eq!(path.bounds, [10.0, 5.0, 14.0, 8.0]);
        assert!(clip.is_none());
        assert_eq!(*blend, GpuBlend::SourceOver);
        let GpuDraw::Fill { path, clip, .. } = &draws[1] else { panic!() };
        let clip = clip.as_ref().expect("the clip chain");
        assert_eq!(clip.links.len(), 1);
        assert!(path.tris.len() > 60, "the circle's outline is flattened: {}", path.tris.len());
        assert!(path.bounds[0] >= 10.0 + 2.0 - 1.0 && path.bounds[2] <= 10.0 + 18.0 + 1.0);
    }

    #[test]
    fn the_compare_metric_tells_a_shift_from_a_match() {
        let (w, h) = (8u32, 8u32);
        let mut a = vec![0u8; 256];
        for y in 2..6 {
            for x in 2..6 {
                a[(y * 8 + x) * 4..(y * 8 + x) * 4 + 4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        let same = compare_premultiplied(&a, &a, w, h, 2);
        assert_eq!((same.differing, same.max), (0, 0));
        let mut shifted = vec![0u8; 256];
        for y in 2..6 {
            for x in 3..7 {
                shifted[(y * 8 + x) * 4..(y * 8 + x) * 4 + 4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        let d = compare_premultiplied(&shifted, &a, w, h, 2);
        assert_eq!(d.differing, 8);
        assert_eq!(d.max, 255);
    }
}
