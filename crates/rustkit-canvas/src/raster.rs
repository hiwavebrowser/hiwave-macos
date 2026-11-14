//! The canvas bitmap (STR-7): every draw on a context is rasterized into a
//! premultiplied RGBA pixmap by tiny-skia (a port of Skia's CPU pipeline,
//! BSD-3-Clause), the pixels `getImageData` reads back. The recording
//! beside it (`DrawCommand`) stays what the engine paints.
//!
//! Paths keep their curves here (the recording flattens them): arcs and
//! ellipses become cubic Béziers of at most a quarter turn each.

use crate::{CanvasStyle, CompositeOperation, LineCap, LineJoin, PathCommand, PatternRepetition, Transform2D};
use rustkit_css::Color;
use std::f32::consts::{PI, TAU};
use std::sync::Arc;
use tiny_skia::{
    BlendMode, FillRule, FilterQuality, GradientStop, Mask, Paint, PathBuilder, PathStroker, Pixmap, Point, Rect,
    Shader, SpreadMode, Stroke, StrokeDash, Transform,
};

/// The bitmap: `None` until something is drawn (a canvas never drawn on
/// holds no pixels), or when the bitmap has no area.
#[derive(Debug, Default)]
pub(crate) struct Raster {
    pixmap: Option<Pixmap>,
}

/// A bitmap a canvas can draw (`drawImage`) or repeat (`createPattern`):
/// another canvas's pixels or a decoded image, premultiplied.
#[derive(Debug, Clone)]
pub struct CanvasBitmap(pub(crate) Pixmap);

impl CanvasBitmap {
    /// From unpremultiplied RGBA rows; `None` when empty or the wrong length.
    pub fn from_rgba(width: u32, height: u32, rgba: &[u8]) -> Option<Self> {
        if rgba.len() as u64 != width as u64 * height as u64 * 4 {
            return None;
        }
        let mut pm = Pixmap::new(width, height)?;
        for (dst, src) in pm.pixels_mut().iter_mut().zip(rgba.chunks_exact(4)) {
            *dst = tiny_skia::ColorU8::from_rgba(src[0], src[1], src[2], src[3]).premultiply();
        }
        Some(CanvasBitmap(pm))
    }

    pub fn width(&self) -> u32 {
        self.0.width()
    }

    pub fn height(&self) -> u32 {
        self.0.height()
    }
}

/// The clip a draw is cut to: the context's clip mask, if any (shared by
/// the saved states that hold it).
pub(crate) type ClipMask = Option<Arc<Mask>>;

/// What every draw takes from the context besides its geometry.
pub(crate) struct Ctx<'a> {
    pub size: (u32, u32),
    pub op: CompositeOperation,
    pub clip: Option<&'a Mask>,
    /// globalAlpha, for what `CanvasStyle::with_alpha` does not fade
    /// (patterns, images).
    pub alpha: f32,
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

fn stops(list: &[crate::ColorStop]) -> Vec<GradientStop> {
    list.iter().map(|s| GradientStop::new(s.offset, color(s.color))).collect()
}

/// A style's shader (globalAlpha already folded into colours and stops;
/// a pattern draws `faded` when given), and for a pattern that does not repeat both
/// ways the band it is cut to, in user space. `None` paints nothing (a
/// gradient with no stops, or a degenerate one).
fn shader<'a>(style: &'a CanvasStyle, faded: Option<&'a Pixmap>) -> Option<(Shader<'a>, Option<Rect>)> {
    match style {
        CanvasStyle::Color(c) => Some((Shader::SolidColor(color(*c)), None)),
        CanvasStyle::LinearGradient(g) => {
            if g.stops.is_empty() || (g.x0 == g.x1 && g.y0 == g.y1) {
                return None;
            }
            if g.stops.len() == 1 {
                return Some((Shader::SolidColor(color(g.stops[0].color)), None));
            }
            let s = tiny_skia::LinearGradient::new(
                Point::from_xy(g.x0, g.y0),
                Point::from_xy(g.x1, g.y1),
                stops(&g.stops),
                SpreadMode::Pad,
                Transform::identity(),
            )?;
            Some((s, None))
        }
        CanvasStyle::RadialGradient(g) => {
            if g.stops.is_empty() || (g.x0 == g.x1 && g.y0 == g.y1 && g.r0 == g.r1) {
                return None;
            }
            if g.stops.len() == 1 {
                return Some((Shader::SolidColor(color(g.stops[0].color)), None));
            }
            // tiny-skia's two-point conical gradient starts at radius 0.
            // A start radius r0 maps offset t to (r0 + t (r1 - r0)) / r1 of
            // the end circle, exact when the circles share a centre, and
            // the inside of the start circle takes the first colour.
            let mut list = stops(&g.stops);
            if g.r0 > 0.0 && g.r1 > 0.0 {
                let k = g.r0 / g.r1;
                list = g
                    .stops
                    .iter()
                    .map(|s| GradientStop::new(k + s.offset * (1.0 - k), color(s.color)))
                    .collect();
                list.insert(0, GradientStop::new(0.0, color(g.stops[0].color)));
            }
            let s = tiny_skia::RadialGradient::new(
                Point::from_xy(g.x0, g.y0),
                Point::from_xy(g.x1, g.y1),
                g.r1,
                list,
                SpreadMode::Pad,
                Transform::identity(),
            )?;
            Some((s, None))
        }
        CanvasStyle::Pattern(p) => {
            let (w, h) = (p.bitmap.width() as f32, p.bitmap.height() as f32);
            const FAR: f32 = 1.0e7;
            let band = match p.repetition {
                PatternRepetition::Repeat => None,
                PatternRepetition::RepeatX => Rect::from_xywh(-FAR, 0.0, 2.0 * FAR, h),
                PatternRepetition::RepeatY => Rect::from_xywh(0.0, -FAR, w, 2.0 * FAR),
                PatternRepetition::NoRepeat => Rect::from_xywh(0.0, 0.0, w, h),
            };
            let s = tiny_skia::Pattern::new(
                faded.unwrap_or(&p.bitmap.0).as_ref(),
                SpreadMode::Repeat,
                FilterQuality::Bilinear,
                1.0,
                Transform::identity(),
            );
            Some((s, band))
        }
    }
}

/// Multiplies every premultiplied pixel by `alpha` (globalAlpha).
fn fade(pm: &mut Pixmap, alpha: f32) {
    if alpha >= 1.0 {
        return;
    }
    let k = (alpha.clamp(0.0, 1.0) * 255.0).round() as u32;
    let m = |c: u8| ((c as u32 * k + 127) / 255) as u8;
    for p in pm.pixels_mut() {
        if let Some(q) = tiny_skia::PremultipliedColorU8::from_rgba(m(p.red()), m(p.green()), m(p.blue()), m(p.alpha())) {
            *p = q;
        }
    }
}

/// `clip` cut further to `band` (user space under `t`).
fn band_clip(size: (u32, u32), clip: Option<&Mask>, band: Rect, t: Transform) -> Option<Mask> {
    let path = PathBuilder::from_rect(band);
    let mut m = match clip {
        Some(c) => c.clone(),
        None => {
            let mut m = Mask::new(size.0, size.1)?;
            m.data_mut().fill(255);
            m
        }
    };
    m.intersect_path(&path, FillRule::Winding, true, t);
    Some(m)
}

impl Raster {
    /// Drops the pixels (a resize, a context reset).
    pub(crate) fn reset(&mut self) {
        self.pixmap = None;
    }

    fn pixmap(&mut self, width: u32, height: u32) -> Option<&mut Pixmap> {
        if self.pixmap.is_none() && crate::bitmap_fits(width, height) {
            self.pixmap = Pixmap::new(width, height);
        }
        self.pixmap.as_mut()
    }

    /// A copy of the pixels, for another canvas to draw; `None` when
    /// nothing was drawn.
    pub(crate) fn bitmap(&self) -> Option<CanvasBitmap> {
        self.pixmap.clone().map(CanvasBitmap)
    }

    /// Fills the tiny-skia path `path` (user space under `t`) with `style`.
    fn fill_skia(&mut self, ctx: &Ctx, path: &tiny_skia::Path, rule: FillRule, style: &CanvasStyle, t: &Transform2D) {
        // A pattern under globalAlpha draws a faded copy (see `fade`).
        let faded = match style {
            CanvasStyle::Pattern(p) if ctx.alpha < 1.0 => {
                let mut pm = p.bitmap.0.clone();
                fade(&mut pm, ctx.alpha);
                Some(pm)
            }
            _ => None,
        };
        let Some((shader, band)) = shader(style, faded.as_ref()) else {
            return;
        };
        let ts = transform(t);
        let banded = band.and_then(|b| band_clip(ctx.size, ctx.clip, b, ts));
        let clip = if band.is_some() { banded.as_ref() } else { ctx.clip };
        if band.is_some() && clip.is_none() {
            return;
        }
        let paint = Paint { shader, blend_mode: blend(ctx.op), anti_alias: true, ..Paint::default() };
        if let Some(pm) = self.pixmap(ctx.size.0, ctx.size.1) {
            pm.fill_path(path, &paint, rule, ts, clip);
        }
    }

    /// Fills `path` (canvas path commands, in user space under `t`).
    pub(crate) fn fill(&mut self, ctx: &Ctx, path: &[PathCommand], even_odd: bool, style: &CanvasStyle, t: &Transform2D) {
        let Some(path) = build_path(path) else {
            return;
        };
        let rule = if even_odd { FillRule::EvenOdd } else { FillRule::Winding };
        self.fill_skia(ctx, &path, rule, style, t);
    }

    /// Strokes `path` with `line`.
    pub(crate) fn stroke(&mut self, ctx: &Ctx, path: &[PathCommand], line: &StrokeStyle, style: &CanvasStyle, t: &Transform2D) {
        let Some(path) = build_path(path) else {
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
        let res_scale = PathStroker::compute_resolution_scale(&transform(t));
        let Some(outline) = path.stroke(&stroke, res_scale) else {
            return;
        };
        self.fill_skia(ctx, &outline, FillRule::Winding, style, t);
    }

    /// Clears the rect `x, y, w, h` under `t` to transparent black.
    pub(crate) fn clear(&mut self, clip: Option<&Mask>, x: f32, y: f32, w: f32, h: f32, t: &Transform2D) {
        let Some(pm) = self.pixmap.as_mut() else {
            return; // nothing drawn yet: already clear
        };
        let Some(path) = build_path(&[PathCommand::Rect(x, y, w, h)]) else {
            return;
        };
        // Chrome clears whole pixels (no anti-aliasing), as Skia does for a
        // clearRect: canvas_raster_cases.json "clear-rect".
        let paint = Paint { blend_mode: BlendMode::Clear, anti_alias: false, ..Paint::default() };
        pm.fill_path(&path, &paint, FillRule::Winding, transform(t), clip);
    }

    /// drawImage: the source rect `src` of `image` drawn into the
    /// destination rect `dst` (user space under `t`), smoothed (bilinear,
    /// Chrome's default "low" quality).
    pub(crate) fn draw_image(&mut self, ctx: &Ctx, image: &CanvasBitmap, src: [f32; 4], dst: [f32; 4], t: &Transform2D) {
        let [sx, sy, sw, sh] = src;
        let [dx, dy, dw, dh] = dst;
        if sw == 0.0 || sh == 0.0 || dw == 0.0 || dh == 0.0 {
            return;
        }
        // The pixels the source rect touches and one more each side: Chrome
        // samples past the source rect into the rest of the image (bilinear
        // at its edges reads the neighbouring pixels, "draw-image-img-data-png"
        // in canvas_img_cases.json), and clamps only at the image's edges.
        let (iw, ih) = (image.width() as f32, image.height() as f32);
        let x0 = (sx.min(sx + sw).floor() - 1.0).max(0.0);
        let y0 = (sy.min(sy + sh).floor() - 1.0).max(0.0);
        let x1 = (sx.max(sx + sw).ceil() + 1.0).min(iw);
        let y1 = (sy.max(sy + sh).ceil() + 1.0).min(ih);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let Some(rect) = tiny_skia::IntRect::from_xywh(x0 as i32, y0 as i32, (x1 - x0) as u32, (y1 - y0) as u32) else {
            return;
        };
        let Some(mut crop) = image.0.clone_rect(rect) else {
            return;
        };
        // globalAlpha is folded into the pixels: a Pattern's own opacity
        // gives anti-aliased edge pixels more alpha than the inside
        // ("draw-image-rotated-alpha" read 195 at edges for 153 inside).
        fade(&mut crop, ctx.alpha);
        // Source space to destination space, then cropped space to source.
        let (kx, ky) = (dw / sw, dh / sh);
        let to_dst = Transform::from_row(kx, 0.0, 0.0, ky, dx - sx * kx, dy - sy * ky).pre_translate(x0, y0);
        // The source rect cut to the image, in destination space.
        let (cx0, cy0) = (sx.min(sx + sw).max(0.0), sy.min(sy + sh).max(0.0));
        let (cx1, cy1) = (sx.max(sx + sw).min(iw), sy.max(sy + sh).min(ih));
        let a = (dx + (cx0 - sx) * kx, dy + (cy0 - sy) * ky);
        let b = (dx + (cx1 - sx) * kx, dy + (cy1 - sy) * ky);
        let Some(area) = Rect::from_ltrb(a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1)) else {
            return;
        };
        let identity = kx == 1.0 && ky == 1.0 && t.a == 1.0 && t.b == 0.0 && t.c == 0.0 && t.d == 1.0;
        let integral = |v: f32| v.fract() == 0.0;
        let quality = if identity && integral(dx + t.e) && integral(dy + t.f) && integral(sx) && integral(sy) {
            FilterQuality::Nearest
        } else {
            FilterQuality::Bilinear
        };
        let shader = tiny_skia::Pattern::new(crop.as_ref(), SpreadMode::Pad, quality, 1.0, to_dst);
        let paint = Paint { shader, blend_mode: blend(ctx.op), anti_alias: true, ..Paint::default() };
        if let Some(pm) = self.pixmap(ctx.size.0, ctx.size.1) {
            pm.fill_path(&PathBuilder::from_rect(area), &paint, FillRule::Winding, transform(t), ctx.clip);
        }
    }

    /// putImageData: unpremultiplied `rgba` rows of `width` written as they
    /// are (no transform, clip, alpha or compositing) at `dx, dy`, the
    /// part inside `dirty` (`x, y, w, h` in the image, already cut to it).
    pub(crate) fn put(&mut self, size: (u32, u32), rgba: &[u8], width: u32, dx: i32, dy: i32, dirty: [i32; 4]) {
        let Some(pm) = self.pixmap(size.0, size.1) else {
            return;
        };
        let (pw, ph) = (pm.width() as i64, pm.height() as i64);
        let [x, y, w, h] = dirty;
        let pixels = pm.pixels_mut();
        for row in y as i64..(y + h) as i64 {
            let ty = dy as i64 + row;
            if ty < 0 || ty >= ph {
                continue;
            }
            for col in x as i64..(x + w) as i64 {
                let tx = dx as i64 + col;
                if tx < 0 || tx >= pw {
                    continue;
                }
                let i = ((row * width as i64 + col) * 4) as usize;
                let Some(p) = rgba.get(i..i + 4) else { continue };
                pixels[(ty * pw + tx) as usize] = tiny_skia::ColorU8::from_rgba(p[0], p[1], p[2], p[3]).premultiply();
            }
        }
    }

    /// The pixels of the rect `x, y, w, h` (bitmap pixels; outside the
    /// bitmap reads as transparent black), unpremultiplied RGBA.
    /// The whole bitmap as premultiplied RGBA rows (`size` when nothing
    /// has drawn yet: transparent black).
    pub(crate) fn premultiplied(&self, size: (u32, u32)) -> Vec<u8> {
        match &self.pixmap {
            Some(pm) => pm.data().to_vec(),
            None => match crate::rgba_len(size.0, size.1) {
                Some(len) => vec![0u8; len],
                None => Vec::new(),
            },
        }
    }

    /// The bytes the bitmap holds (none before the first draw).
    pub(crate) fn bytes(&self) -> usize {
        self.pixmap.as_ref().map_or(0, |pm| pm.data().len())
    }

    pub(crate) fn read(&self, x: i32, y: i32, w: u32, h: u32) -> Vec<u8> {
        // Past the read limit: nothing (the bindings throw RangeError first).
        let Some(len) = crate::rgba_len(w, h) else {
            return Vec::new();
        };
        let mut out = vec![0u8; len];
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

/// The clip mask after clipping to `path` (HTML §4.12.5.1.10: the new
/// clip is the old one intersected with the path), on a `size` bitmap.
pub(crate) fn clip_to(size: (u32, u32), old: Option<&Mask>, path: &[PathCommand], even_odd: bool, t: &Transform2D) -> ClipMask {
    let rule = if even_odd { FillRule::EvenOdd } else { FillRule::Winding };
    let ts = transform(t);
    let path = build_path(path);
    let mut m = match old {
        Some(m) => m.clone(),
        None => {
            let mut m = Mask::new(size.0, size.1)?;
            if let Some(p) = &path {
                m.fill_path(p, rule, true, ts);
            }
            return Some(Arc::new(m));
        }
    };
    match &path {
        Some(p) => m.intersect_path(p, rule, true, ts),
        None => m.clear(),
    }
    Some(Arc::new(m))
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
