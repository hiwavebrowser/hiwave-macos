//! The canvas bitmap (STR-7): every draw on a context is rasterized into a
//! premultiplied RGBA pixmap by tiny-skia (a port of Skia's CPU pipeline,
//! BSD-3-Clause), the pixels `getImageData` reads back. The recording
//! beside it (`DrawCommand`) stays what the engine paints.
//!
//! Paths keep their curves here (the recording flattens them): arcs and
//! ellipses become cubic Béziers of at most a quarter turn each.

use crate::{CanvasStyle, CompositeOperation, LineCap, LineJoin, PathCommand, Transform2D};
use rustkit_css::Color;
use std::f32::consts::{PI, TAU};
use tiny_skia::{
    BlendMode, FillRule, Paint, PathBuilder, PathStroker, Pixmap, Shader, Stroke, StrokeDash, Transform,
};

/// The bitmap: `None` until something is drawn (a canvas never drawn on
/// holds no pixels), or when the bitmap has no area.
#[derive(Debug, Default)]
pub(crate) struct Raster {
    pixmap: Option<Pixmap>,
}

/// How a stroke is drawn: the context's line styles at the time.
pub(crate) struct StrokeStyle<'a> {
    pub width: f32,
    pub cap: LineCap,
    pub join: LineJoin,
    pub miter_limit: f32,
    pub dash: &'a [f32],
    pub dash_offset: f32,
}

pub(crate) fn transform(t: &Transform2D) -> Transform {
    Transform::from_row(t.a, t.b, t.c, t.d, t.e, t.f)
}

fn blend(op: CompositeOperation) -> BlendMode {
    match op {
        CompositeOperation::SourceOver => BlendMode::SourceOver,
        CompositeOperation::SourceIn => BlendMode::SourceIn,
        CompositeOperation::SourceOut => BlendMode::SourceOut,
        CompositeOperation::SourceAtop => BlendMode::SourceAtop,
        CompositeOperation::DestinationOver => BlendMode::DestinationOver,
        CompositeOperation::DestinationIn => BlendMode::DestinationIn,
        CompositeOperation::DestinationOut => BlendMode::DestinationOut,
        CompositeOperation::DestinationAtop => BlendMode::DestinationAtop,
        CompositeOperation::Lighter => BlendMode::Plus,
        CompositeOperation::Copy => BlendMode::Source,
        CompositeOperation::Xor => BlendMode::Xor,
        CompositeOperation::Multiply => BlendMode::Multiply,
        CompositeOperation::Screen => BlendMode::Screen,
        CompositeOperation::Overlay => BlendMode::Overlay,
        CompositeOperation::Darken => BlendMode::Darken,
        CompositeOperation::Lighten => BlendMode::Lighten,
    }
}

fn color(c: Color) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba8(c.r, c.g, c.b, (c.a.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// The paint for `style` (globalAlpha already folded in); `None` for a
/// style this slice does not rasterize yet (gradients, patterns).
fn paint(style: &CanvasStyle, op: CompositeOperation) -> Option<Paint<'static>> {
    let shader = match style {
        CanvasStyle::Color(c) => Shader::SolidColor(color(*c)),
        _ => return None,
    };
    Some(Paint { shader, blend_mode: blend(op), anti_alias: true, ..Paint::default() })
}

impl Raster {
    /// Drops the pixels (a resize, a context reset).
    pub(crate) fn reset(&mut self) {
        self.pixmap = None;
    }

    fn pixmap(&mut self, width: u32, height: u32) -> Option<&mut Pixmap> {
        if self.pixmap.is_none() {
            self.pixmap = Pixmap::new(width, height);
        }
        self.pixmap.as_mut()
    }

    /// Fills `path` (canvas path commands, in user space under `t`).
    pub(crate) fn fill(
        &mut self,
        size: (u32, u32),
        path: &[PathCommand],
        even_odd: bool,
        style: &CanvasStyle,
        op: CompositeOperation,
        t: &Transform2D,
    ) {
        let (Some(path), Some(paint)) = (build_path(path), paint(style, op)) else {
            return;
        };
        let rule = if even_odd { FillRule::EvenOdd } else { FillRule::Winding };
        if let Some(pm) = self.pixmap(size.0, size.1) {
            pm.fill_path(&path, &paint, rule, transform(t), None);
        }
    }

    /// Strokes `path` with `line`.
    pub(crate) fn stroke(
        &mut self,
        size: (u32, u32),
        path: &[PathCommand],
        line: &StrokeStyle,
        style: &CanvasStyle,
        op: CompositeOperation,
        t: &Transform2D,
    ) {
        let (Some(path), Some(paint)) = (build_path(path), paint(style, op)) else {
            return;
        };
        let stroke = Stroke {
            width: line.width,
            miter_limit: line.miter_limit,
            line_cap: match line.cap {
                LineCap::Butt => tiny_skia::LineCap::Butt,
                LineCap::Round => tiny_skia::LineCap::Round,
                LineCap::Square => tiny_skia::LineCap::Square,
            },
            line_join: match line.join {
                LineJoin::Miter => tiny_skia::LineJoin::Miter,
                LineJoin::Round => tiny_skia::LineJoin::Round,
                LineJoin::Bevel => tiny_skia::LineJoin::Bevel,
            },
            dash: if line.dash.is_empty() || line.dash.iter().all(|d| *d == 0.0) {
                None
            } else {
                StrokeDash::new(line.dash.to_vec(), line.dash_offset)
            },
        };
        // The outline is filled rather than handed to `stroke_path`, which
        // draws a stroke no wider than a device pixel as a hairline, without
        // joins or caps; Chrome draws its true outline ("stroke-rect-thin").
        let ts = transform(t);
        let res_scale = PathStroker::compute_resolution_scale(&ts);
        let Some(outline) = path.stroke(&stroke, res_scale) else {
            return;
        };
        if let Some(pm) = self.pixmap(size.0, size.1) {
            pm.fill_path(&outline, &paint, FillRule::Winding, ts, None);
        }
    }

    /// Clears the rect `x, y, w, h` under `t` to transparent black.
    pub(crate) fn clear(&mut self, x: f32, y: f32, w: f32, h: f32, t: &Transform2D) {
        let Some(pm) = self.pixmap.as_mut() else {
            return; // nothing drawn yet: already clear
        };
        let Some(path) = build_path(&[PathCommand::Rect(x, y, w, h)]) else {
            return;
        };
        // Chrome clears whole pixels (no anti-aliasing), as Skia does for a
        // clearRect: canvas_raster_cases.json "clear-rect".
        let paint = Paint { blend_mode: BlendMode::Clear, anti_alias: false, ..Paint::default() };
        pm.fill_path(&path, &paint, FillRule::Winding, transform(t), None);
    }

    /// The pixels of the rect `x, y, w, h` (bitmap pixels; outside the
    /// bitmap reads as transparent black), unpremultiplied RGBA.
    pub(crate) fn read(&self, x: i32, y: i32, w: u32, h: u32) -> Vec<u8> {
        let mut out = vec![0u8; w as usize * h as usize * 4];
        let Some(pm) = &self.pixmap else {
            return out;
        };
        let (pw, ph) = (pm.width() as i64, pm.height() as i64);
        let pixels = pm.pixels();
        for row in 0..h as i64 {
            let sy = y as i64 + row;
            if sy < 0 || sy >= ph {
                continue;
            }
            for col in 0..w as i64 {
                let sx = x as i64 + col;
                if sx < 0 || sx >= pw {
                    continue;
                }
                let p = pixels[(sy * pw + sx) as usize];
                let o = ((row * w as i64 + col) * 4) as usize;
                out[o..o + 4].copy_from_slice(&unpremultiply(p.red(), p.green(), p.blue(), p.alpha()));
            }
        }
        out
    }
}

/// Skia's unpremultiply (SkUnPreMultiply): `c * 255 / a`, rounded.
fn unpremultiply(r: u8, g: u8, b: u8, a: u8) -> [u8; 4] {
    match a {
        0 => [0, 0, 0, 0],
        255 => [r, g, b, 255],
        _ => {
            let scale = ((255u32 << 24) + a as u32 / 2) / a as u32;
            let f = |c: u8| ((c as u32 * scale + (1 << 23)) >> 24).min(255) as u8;
            [f(r), f(g), f(b), a]
        }
    }
}

/// A canvas path as a tiny-skia path, curves kept (HTML §4.12.5.1.6).
/// `None` when it has no segments.
pub(crate) fn build_path(commands: &[PathCommand]) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    // The current point and the start of the sub-path; `None` before the
    // first point ("ensure there is a subpath").
    let mut cur: Option<(f32, f32)> = None;
    let mut start = (0.0, 0.0);
    let ensure = |pb: &mut PathBuilder, cur: &mut Option<(f32, f32)>, start: &mut (f32, f32), p: (f32, f32)| {
        if cur.is_none() {
            pb.move_to(p.0, p.1);
            *cur = Some(p);
            *start = p;
        }
    };
    for cmd in commands {
        match *cmd {
            PathCommand::MoveTo(x, y) => {
                pb.move_to(x, y);
                cur = Some((x, y));
                start = (x, y);
            }
            PathCommand::LineTo(x, y) => {
                ensure(&mut pb, &mut cur, &mut start, (x, y));
                pb.line_to(x, y);
                cur = Some((x, y));
            }
            PathCommand::QuadraticCurveTo(cx, cy, x, y) => {
                ensure(&mut pb, &mut cur, &mut start, (cx, cy));
                pb.quad_to(cx, cy, x, y);
                cur = Some((x, y));
            }
            PathCommand::BezierCurveTo(c1x, c1y, c2x, c2y, x, y) => {
                ensure(&mut pb, &mut cur, &mut start, (c1x, c1y));
                pb.cubic_to(c1x, c1y, c2x, c2y, x, y);
                cur = Some((x, y));
            }
            PathCommand::Arc(cx, cy, r, a0, a1, ccw) => {
                ellipse(&mut pb, &mut cur, &mut start, (cx, cy), (r, r), 0.0, a0, a1, ccw);
            }
            PathCommand::Ellipse(cx, cy, rx, ry, rot, a0, a1, ccw) => {
                ellipse(&mut pb, &mut cur, &mut start, (cx, cy), (rx, ry), rot, a0, a1, ccw);
            }
            PathCommand::ArcTo(x1, y1, x2, y2, r) => {
                ensure(&mut pb, &mut cur, &mut start, (x1, y1));
                let p0 = cur.unwrap_or((x1, y1));
                arc_to(&mut pb, p0, (x1, y1), (x2, y2), r, &mut cur);
            }
            PathCommand::Rect(x, y, w, h) => {
                pb.move_to(x, y);
                pb.line_to(x + w, y);
                pb.line_to(x + w, y + h);
                pb.line_to(x, y + h);
                pb.close();
                pb.move_to(x, y);
                cur = Some((x, y));
                start = (x, y);
            }
            PathCommand::ClosePath => {
                if cur.is_some() {
                    pb.close();
                    // A new sub-path starts at the closed one's start.
                    pb.move_to(start.0, start.1);
                    cur = Some(start);
                }
            }
        }
    }
    pb.finish()
}

/// An arc of the ellipse centred on `c` with radii `r`, rotated `rot`,
/// from angle `a0` to `a1` (HTML §4.12.5.1.6 ellipse()): a line from the
/// current point to its start, then the arc.
#[allow(clippy::too_many_arguments)]
fn ellipse(
    pb: &mut PathBuilder,
    cur: &mut Option<(f32, f32)>,
    start: &mut (f32, f32),
    c: (f32, f32),
    r: (f32, f32),
    rot: f32,
    a0: f32,
    a1: f32,
    ccw: bool,
) {
    let sweep = if !ccw && a1 - a0 >= TAU {
        TAU
    } else if ccw && a0 - a1 >= TAU {
        -TAU
    } else {
        let mut s = (a1 - a0) % TAU;
        if !ccw && s < 0.0 {
            s += TAU;
        } else if ccw && s > 0.0 {
            s -= TAU;
        }
        s
    };
    let (sr, cr) = rot.sin_cos();
    let point = |a: f32| {
        let (s, co) = a.sin_cos();
        let (x, y) = (r.0 * co, r.1 * s);
        (c.0 + x * cr - y * sr, c.1 + x * sr + y * cr)
    };
    // The derivative of `point`, per radian.
    let tangent = |a: f32| {
        let (s, co) = a.sin_cos();
        let (x, y) = (-r.0 * s, r.1 * co);
        (x * cr - y * sr, x * sr + y * cr)
    };
    let p0 = point(a0);
    match cur {
        Some(_) => pb.line_to(p0.0, p0.1),
        None => {
            pb.move_to(p0.0, p0.1);
            *start = p0;
        }
    }
    let n = ((sweep.abs() / (PI / 2.0)) - 1e-4).ceil().max(1.0) as usize;
    let step = sweep / n as f32;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let mut a = a0;
    let mut p = p0;
    for i in 0..n {
        let b = if i + 1 == n { a0 + sweep } else { a + step };
        let q = point(b);
        let (t0, t1) = (tangent(a), tangent(b));
        if sweep != 0.0 {
            pb.cubic_to(p.0 + k * t0.0, p.1 + k * t0.1, q.0 - k * t1.0, q.1 - k * t1.1, q.0, q.1);
        }
        a = b;
        p = q;
    }
    *cur = Some(p);
}

/// arcTo (HTML §4.12.5.1.6): the arc of radius `r` tangent to the lines
/// p0-p1 and p1-p2, joined to the current point by a line.
fn arc_to(
    pb: &mut PathBuilder,
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    r: f32,
    cur: &mut Option<(f32, f32)>,
) {
    let (v1, v2) = ((p0.0 - p1.0, p0.1 - p1.1), (p2.0 - p1.0, p2.1 - p1.1));
    let cross = v1.0 * v2.1 - v1.1 * v2.0;
    let (l1, l2) = ((v1.0 * v1.0 + v1.1 * v1.1).sqrt(), (v2.0 * v2.0 + v2.1 * v2.1).sqrt());
    if p0 == p1 || p1 == p2 || r == 0.0 || cross.abs() <= 1e-6 * l1 * l2 {
        pb.line_to(p1.0, p1.1);
        *cur = Some(p1);
        return;
    }
    let (u1, u2) = ((v1.0 / l1, v1.1 / l1), (v2.0 / l2, v2.1 / l2));
    let cos = (u1.0 * u2.0 + u1.1 * u2.1).clamp(-1.0, 1.0);
    let half = cos.acos() / 2.0;
    let d = r / half.tan();
    let t1 = (p1.0 + u1.0 * d, p1.1 + u1.1 * d);
    let t2 = (p1.0 + u2.0 * d, p1.1 + u2.1 * d);
    let (bx, by) = (u1.0 + u2.0, u1.1 + u2.1);
    let bl = (bx * bx + by * by).sqrt();
    let h = r / half.sin();
    let c = (p1.0 + bx / bl * h, p1.1 + by / bl * h);
    let a0 = (t1.1 - c.1).atan2(t1.0 - c.0);
    let a1 = (t2.1 - c.1).atan2(t2.0 - c.0);
    // The short way round: anticlockwise when the corner turns right.
    let ccw = cross > 0.0;
    let mut start = p0;
    ellipse(pb, cur, &mut start, c, (r, r), 0.0, a0, a1, ccw);
}
