//! Canvas 2D paint (HTML §4.12.5, CH2.E10 phase 1): a `<canvas>` lays out
//! as a replaced box (`BoxType::Image` under [`canvas_key`], its natural
//! size the bitmap's), and when the display list is built the image
//! command is replaced by what script drew on the canvas's 2D context
//! (`DomBindings::canvas_snapshot`), as vector commands mapped from bitmap
//! pixels onto the box: fills as tessellated polygons (rustkit-svg's fill),
//! strokes as polylines, text as text runs, the clip as a clip rect.
//!
//! Vector, not a raster: the renderer already draws every one of these
//! commands (inline SVG is painted the same way), text goes through the
//! same shaper and glyph cache as the page's own text, and a canvas drawn
//! at devicePixelRatio stays sharp. The cost is what a vector recording
//! cannot express (see the module's "not done" list in the PR): a partial
//! `clearRect`, compositing modes, pixel reads.

use rustkit_bindings::canvas::{
    CanvasFont, CanvasStyle, DrawCommand, LineDash, LinearGradient, TextAlign, TextBaseline,
    Transform2D,
};
use rustkit_bindings::{CanvasSnapshot, CanvasTextExtent, CanvasTextMeasurer};
use rustkit_css::Color;
use rustkit_layout::{DisplayCommand, Rect};

const KEY_PREFIX: &str = "rustkit-canvas:";

/// The image key a canvas element's box paints under.
pub(crate) fn canvas_key(node: usize) -> String {
    format!("{KEY_PREFIX}{node}")
}

/// The canvas element a box's image key names, if it names one.
pub(crate) fn canvas_node(key: &str) -> Option<usize> {
    key.strip_prefix(KEY_PREFIX)?.parse().ok()
}

/// A canvas's bitmap size from its `width`/`height` attributes (HTML
/// §4.12.5: rules for parsing non-negative integers, 300x150 otherwise).
pub(crate) fn bitmap_size(width: Option<&String>, height: Option<&String>) -> (f32, f32) {
    fn parse(v: Option<&String>, default: f32) -> f32 {
        let Some(v) = v else { return default };
        let t = v.trim_start_matches([' ', '\t', '\n', '\r', '\u{c}']);
        let t = t.strip_prefix('+').unwrap_or(t);
        let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
        match digits.parse::<u64>() {
            Ok(n) if n <= i32::MAX as u64 => n as f32,
            _ => default,
        }
    }
    (parse(width, 300.0), parse(height, 150.0))
}

/// The engine's text shaper for canvas text and `measureText`.
pub(crate) fn text_measurer() -> CanvasTextMeasurer {
    Box::new(|font: &CanvasFont, text: &str| {
        let m = measure(font, text);
        CanvasTextExtent {
            width: m.width,
            ascent: m.ascent,
            descent: m.descent,
        }
    })
}

fn measure(font: &CanvasFont, text: &str) -> rustkit_layout::TextMetrics {
    rustkit_layout::measure_text_advanced(
        text,
        &font.family,
        font.size,
        rustkit_css::FontWeight(font.weight),
        if font.italic {
            rustkit_css::FontStyle::Italic
        } else {
            rustkit_css::FontStyle::Normal
        },
    )
}

fn initial_font() -> CanvasFont {
    CanvasFont::parse("10px sans-serif", 16.0).expect("initial canvas font")
}

thread_local! {
    static PAINTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How many times this thread turned a recording into display commands
/// (`paint_commands`): a test's window on the paint cache.
#[cfg(test)]
pub(crate) fn paints() -> usize {
    PAINTS.with(|p| p.get())
}

/// The commands that paint `canvas` into a box whose fitted image rect is
/// `dest` and whose content box is `content`: bitmap pixel (x, y) lands at
/// `dest.x + x * dest.width / bitmap width`, and nothing paints outside the
/// content box.
pub(crate) fn paint_commands(
    canvas: &CanvasSnapshot,
    dest: Rect,
    content: Rect,
) -> Vec<DisplayCommand> {
    PAINTS.with(|p| p.set(p.get() + 1));
    let mut out = Vec::new();
    if canvas.commands.is_empty() || canvas.width == 0 || canvas.height == 0 {
        return out;
    }
    let to_box = Transform2D::identity().translate(dest.x, dest.y).scale(
        dest.width / canvas.width as f32,
        dest.height / canvas.height as f32,
    );
    let frame = intersect(content, dest);
    out.push(DisplayCommand::PushClip(frame));
    let mut clipped = false;
    for command in &canvas.commands {
        match command {
            DrawCommand::Clip { rect } => {
                if clipped {
                    out.push(DisplayCommand::PopClip);
                    clipped = false;
                }
                if let Some([x0, y0, x1, y1]) = *rect {
                    let (ax, ay) = to_box.apply(x0, y0);
                    let (bx, by) = to_box.apply(x1, y1);
                    let r = Rect::new(ax.min(bx), ay.min(by), (bx - ax).abs(), (by - ay).abs());
                    out.push(DisplayCommand::PushClip(intersect(r, frame)));
                    clipped = true;
                }
            }
            DrawCommand::FillRect {
                x,
                y,
                w,
                h,
                style,
                transform,
            } => {
                let t = to_box.multiply(transform);
                let solid = solid(style);
                if let (Some(color), true) = (solid, t.b == 0.0 && t.c == 0.0) {
                    let (ax, ay) = t.apply(*x, *y);
                    let (bx, by) = t.apply(x + w, y + h);
                    let rect = Rect::new(ax.min(bx), ay.min(by), (bx - ax).abs(), (by - ay).abs());
                    if color.a > 0.0 {
                        out.push(DisplayCommand::FillRect { rect, color });
                    }
                } else {
                    let contour = vec![(*x, *y), (x + w, *y), (x + w, y + h), (*x, y + h)];
                    fill(&mut out, &[contour], false, style, &t);
                }
            }
            DrawCommand::FillPath {
                segments,
                style,
                even_odd,
                transform,
            } => {
                fill(
                    &mut out,
                    segments,
                    *even_odd,
                    style,
                    &to_box.multiply(transform),
                );
            }
            DrawCommand::StrokeRect {
                x,
                y,
                w,
                h,
                style,
                line_width,
                dash,
                transform,
            } => {
                let contour = vec![(*x, *y), (x + w, *y), (x + w, y + h), (*x, y + h), (*x, *y)];
                stroke(
                    &mut out,
                    &[contour],
                    style,
                    *line_width,
                    dash,
                    &to_box.multiply(transform),
                );
            }
            DrawCommand::StrokePath {
                segments,
                style,
                line_width,
                dash,
                transform,
            } => {
                stroke(
                    &mut out,
                    segments,
                    style,
                    *line_width,
                    dash,
                    &to_box.multiply(transform),
                );
            }
            DrawCommand::FillText {
                text,
                x,
                y,
                style,
                font,
                align,
                baseline,
                max_width,
                transform,
            }
            | DrawCommand::StrokeText {
                text,
                x,
                y,
                style,
                font,
                align,
                baseline,
                max_width,
                transform,
                ..
            } => {
                let font = CanvasFont::parse(font, 16.0).unwrap_or_else(initial_font);
                draw_text(
                    &mut out,
                    text,
                    *x,
                    *y,
                    style,
                    &font,
                    *align,
                    *baseline,
                    *max_width,
                    &to_box.multiply(transform),
                );
            }
            // A partial clear cannot take paint back out of a vector
            // recording (a whole-bitmap clear drops the recording instead).
            DrawCommand::ClearRect { .. }
            | DrawCommand::DrawImage { .. }
            | DrawCommand::PutImageData { .. } => {}
        }
    }
    if clipped {
        out.push(DisplayCommand::PopClip);
    }
    out.push(DisplayCommand::PopClip);
    out
}

fn intersect(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.width).min(b.x + b.width);
    let y1 = (a.y + a.height).min(b.y + b.height);
    Rect::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
}

fn solid(style: &CanvasStyle) -> Option<Color> {
    match style {
        CanvasStyle::Color(c) => Some(*c),
        _ => None,
    }
}

/// One colour standing for a style a command cannot paint as such: a
/// gradient's middle (strokes and text with a gradient).
fn representative(style: &CanvasStyle) -> Color {
    match style {
        CanvasStyle::Color(c) => *c,
        CanvasStyle::LinearGradient(g) => g.sample(0.5),
        CanvasStyle::RadialGradient(g) => g
            .stops
            .first()
            .map(|s| s.color)
            .unwrap_or(Color::TRANSPARENT),
        CanvasStyle::Pattern(_) => Color::TRANSPARENT,
    }
}

fn device(segments: &[Vec<(f32, f32)>], t: &Transform2D) -> Vec<Vec<(f32, f32)>> {
    segments
        .iter()
        .map(|s| s.iter().map(|&(x, y)| t.apply(x, y)).collect())
        .collect()
}

fn fill(
    out: &mut Vec<DisplayCommand>,
    segments: &[Vec<(f32, f32)>],
    even_odd: bool,
    style: &CanvasStyle,
    t: &Transform2D,
) {
    let contours = device(segments, t);
    match style {
        CanvasStyle::Color(c) => {
            if c.a > 0.0 {
                out.extend(rustkit_svg::fill_device_contours(&contours, even_odd, *c));
            }
        }
        CanvasStyle::LinearGradient(LinearGradient {
            x0,
            y0,
            x1,
            y1,
            stops,
        }) => {
            if stops.is_empty() {
                return;
            }
            let stops = stops.iter().map(|s| (s.offset, s.color)).collect();
            out.extend(rustkit_svg::fill_device_contours_linear(
                &contours,
                even_odd,
                t.apply(*x0, *y0),
                t.apply(*x1, *y1),
                stops,
            ));
        }
        other => {
            let c = representative(other);
            if c.a > 0.0 {
                out.extend(rustkit_svg::fill_device_contours(&contours, even_odd, c));
            }
        }
    }
}

fn scale_of(t: &Transform2D) -> f32 {
    (t.a * t.d - t.b * t.c).abs().sqrt()
}

fn stroke(
    out: &mut Vec<DisplayCommand>,
    segments: &[Vec<(f32, f32)>],
    style: &CanvasStyle,
    line_width: f32,
    dash: &LineDash,
    t: &Transform2D,
) {
    let color = representative(style);
    if color.a <= 0.0 {
        return;
    }
    let scale = scale_of(t);
    let width = line_width * scale;
    for segment in segments {
        let pieces = if dash.segments.iter().any(|d| *d > 0.0) {
            dashed(segment, &dash.segments, dash.offset)
        } else {
            vec![segment.clone()]
        };
        for piece in pieces {
            if piece.len() < 2 {
                continue;
            }
            let points: Vec<(f32, f32)> = piece.iter().map(|&(x, y)| t.apply(x, y)).collect();
            out.push(DisplayCommand::Polyline {
                points,
                color,
                width,
            });
        }
    }
}

/// Split a polyline (user space) into its dashes: `pattern` alternates
/// on and off lengths, starting `offset` into it.
fn dashed(points: &[(f32, f32)], pattern: &[f32], offset: f32) -> Vec<Vec<(f32, f32)>> {
    let total: f32 = pattern.iter().sum();
    if total <= 0.0 {
        return vec![points.to_vec()];
    }
    let mut index = 0;
    let mut left = pattern[0];
    let mut pos = offset.rem_euclid(total);
    while pos > 0.0 {
        if pos >= left {
            pos -= left;
            index = (index + 1) % pattern.len();
            left = pattern[index];
        } else {
            left -= pos;
            pos = 0.0;
        }
    }
    let mut out = Vec::new();
    let mut current: Vec<(f32, f32)> = Vec::new();
    if index % 2 == 0 {
        current.push(points[0]);
    }
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let len = (b.0 - a.0).hypot(b.1 - a.1);
        let mut at = 0.0;
        while len - at > left {
            at += left;
            let p = (a.0 + (b.0 - a.0) * at / len, a.1 + (b.1 - a.1) * at / len);
            if index % 2 == 0 {
                current.push(p);
                out.push(std::mem::take(&mut current));
            } else {
                current.push(p);
            }
            index = (index + 1) % pattern.len();
            left = pattern[index];
        }
        left -= len - at;
        if index % 2 == 0 {
            current.push(b);
        }
    }
    if index % 2 == 0 && current.len() > 1 {
        out.push(current);
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn draw_text(
    out: &mut Vec<DisplayCommand>,
    text: &str,
    x: f32,
    y: f32,
    style: &CanvasStyle,
    font: &CanvasFont,
    align: TextAlign,
    baseline: TextBaseline,
    max_width: Option<f32>,
    t: &Transform2D,
) {
    let color = representative(style);
    if color.a <= 0.0 || text.is_empty() {
        return;
    }
    let metrics = measure(font, text);
    // maxWidth narrower than the run: the font shrinks to fit (the
    // condensing Chromium does is horizontal only).
    let mut size = font.size;
    let mut width = metrics.width;
    if let Some(max) = max_width {
        if width > max && width > 0.0 {
            size *= max / width;
            width = max;
        }
    }
    let k = if font.size > 0.0 {
        size / font.size
    } else {
        1.0
    };
    let (ascent, descent) = (metrics.ascent * k, metrics.descent * k);
    let dx = match align {
        TextAlign::Left | TextAlign::Start => 0.0,
        TextAlign::Center => -width / 2.0,
        TextAlign::Right | TextAlign::End => -width,
    };
    // The em box split in the font's ascent:descent ratio (HTML canvas
    // §4.12.5.1.13: top/bottom are the em square's edges).
    let em = ascent + descent;
    let (em_up, em_down) = if em > 0.0 {
        (size * ascent / em, size * descent / em)
    } else {
        (size * 0.8, size * 0.2)
    };
    let dy = match baseline {
        TextBaseline::Alphabetic => 0.0,
        TextBaseline::Top => em_up,
        TextBaseline::Hanging => em_up * 0.8,
        TextBaseline::Middle => (em_up - em_down) / 2.0,
        TextBaseline::Bottom | TextBaseline::Ideographic => -em_down,
    };
    // Rotation and skew are not carried by a text run: the run is placed
    // at the transformed baseline origin, scaled by the transform's scale.
    let (px, py) = t.apply(x + dx, y + dy);
    out.push(DisplayCommand::Text {
        text: text.to_string(),
        x: px,
        y: py,
        color,
        font_size: size * scale_of(t),
        font_family: font.family.clone(),
        font_weight: font.weight,
        font_style: if font.italic { 1 } else { 0 },
        advances: None,
        // y is the baseline (see rustkit-svg's text: a zero ascent hands
        // the renderer the baseline directly).
        ascent: Some(0.0),
        run: None,
    });
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn dashes_split_a_line_into_on_runs() {
        let runs = dashed(&[(0.0, 0.0), (10.0, 0.0)], &[2.0, 3.0], 0.0);
        assert_eq!(
            runs,
            vec![vec![(0.0, 0.0), (2.0, 0.0)], vec![(5.0, 0.0), (7.0, 0.0)]]
        );
    }

    #[test]
    fn bitmap_size_reads_attributes_as_html_does() {
        let s = |v: &str| Some(v.to_string());
        assert_eq!(bitmap_size(None, None), (300.0, 150.0));
        assert_eq!(
            bitmap_size(s("40").as_ref(), s(" 20px").as_ref()),
            (40.0, 20.0)
        );
        assert_eq!(
            bitmap_size(s("x").as_ref(), s("-5").as_ref()),
            (300.0, 150.0)
        );
    }
}
