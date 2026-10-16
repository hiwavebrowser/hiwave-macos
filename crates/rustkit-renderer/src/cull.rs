//! Viewport culling: a command whose paint lands wholly outside the target
//! is skipped before it is turned into quads.
//!
//! Until this, every command of the page was shaped, rasterized, batched
//! and uploaded on every frame, and the GPU clipped all but a viewport's
//! worth away: 99.8% of the quads of a 20,000-command text page, which
//! is what a scrolled frame cost (see `scroll_cost_tests`). On an
//! 80,000-command page the glyph batch outgrew wgpu's buffer limit and
//! the frame failed.
//!
//! Only commands whose paint is bounded by their own fields are culled,
//! with a margin for antialiasing and glyph overhang; everything else
//! (state pushes and pops, shadows, filters, gradients, form controls,
//! ...) is always processed, so the clip, transform, opacity and stacking
//! stacks are exactly what they were. Fixed content needs nothing special:
//! `PushViewportFixed` undoes the scroll in the transform stack, and the
//! cull tests the command where the current transform puts it. A clip only
//! ever removes pixels, so a command outside the viewport stays outside
//! it whatever clip is open.
//!
//! The fast path's mid-list flushes (a fill over batched glyphs) are the
//! one thing culling can change: a flush whose overlap was off-screen no
//! longer happens. That flush ordered off-screen pixels only, so the frame
//! is the same, with one exception the fast path already has: images are
//! not part of the overlap check, so an on-screen fill drawn after an
//! on-screen image it overlaps relied on such a flush to land on top.

use rustkit_layout::{DisplayCommand, Rect};

/// Antialiasing fringe added to every bound, in CSS px.
const FRINGE: f32 = 2.0;

/// The rect, before the current transform, that holds every pixel `cmd`
/// paints; `None` for a command that is not bounded this way (it is
/// never culled).
pub(crate) fn paint_bounds(cmd: &DisplayCommand) -> Option<Rect> {
    let rect = match cmd {
        DisplayCommand::SolidColor(_, rect)
        | DisplayCommand::RoundedRect { rect, .. }
        | DisplayCommand::Border { rect, .. }
        | DisplayCommand::RoundedBorder { rect, .. } => *rect,
        DisplayCommand::Text {
            x,
            y,
            font_size,
            advances: Some(advances),
            ascent,
            ..
        } => {
            // A glyph's ink can start left of its pen position and reach
            // past its advance (italics, overhang) and below the
            // baseline: one font size of slack on every side covers it.
            let width: f32 = advances.iter().map(|a| a.abs()).sum();
            let size = font_size.abs().max(1.0);
            let ascent = ascent.map_or(size, |a| a.abs().max(size));
            Rect::new(x - size, y - size, width + 2.0 * size, ascent + 3.0 * size)
        }
        DisplayCommand::TextDecoration {
            x,
            y,
            width,
            thickness,
            ..
        } => {
            // A wavy or double line swings a few thicknesses each way.
            let swing = 3.0 * thickness.abs() + 4.0;
            Rect::new(*x, y - swing, *width, thickness.abs() + 2.0 * swing)
        }
        // `ObjectFit::paint_quad` clips the image to its content box.
        DisplayCommand::Image { content_box, .. } => *content_box,
        _ => return None,
    };
    let finite = rect.x.is_finite()
        && rect.y.is_finite()
        && rect.width.is_finite()
        && rect.height.is_finite();
    finite.then(|| {
        Rect::new(
            rect.x - FRINGE,
            rect.y - FRINGE,
            rect.width.max(0.0) + 2.0 * FRINGE,
            rect.height.max(0.0) + 2.0 * FRINGE,
        )
    })
}

/// True when `bounds`, mapped through the affine `m`, misses the
/// `width` x `height` target.
pub(crate) fn outside(m: [f32; 6], bounds: Rect, width: f32, height: f32) -> bool {
    let map = |x: f32, y: f32| (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]);
    let corners = [
        map(bounds.x, bounds.y),
        map(bounds.x + bounds.width, bounds.y),
        map(bounds.x, bounds.y + bounds.height),
        map(bounds.x + bounds.width, bounds.y + bounds.height),
    ];
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (x, y) in corners {
        if !(x.is_finite() && y.is_finite()) {
            return false;
        }
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    x1 <= 0.0 || y1 <= 0.0 || x0 >= width || y0 >= height
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustkit_css::Color;

    const ID: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

    #[test]
    fn a_fill_below_the_viewport_is_outside_until_scrolled_to() {
        let fill = DisplayCommand::SolidColor(
            Color::new(0, 0, 0, 1.0),
            Rect::new(0.0, 2000.0, 100.0, 20.0),
        );
        let b = paint_bounds(&fill).unwrap();
        assert!(outside(ID, b, 800.0, 600.0));
        // Scrolled by 1500: the page translate brings it to y = 500.
        assert!(!outside(
            [1.0, 0.0, 0.0, 1.0, 0.0, -1500.0],
            b,
            800.0,
            600.0
        ));
    }

    #[test]
    fn the_fringe_keeps_an_edge_touching_fill() {
        let fill = DisplayCommand::SolidColor(
            Color::new(0, 0, 0, 1.0),
            Rect::new(0.0, 600.5, 100.0, 20.0),
        );
        assert!(!outside(ID, paint_bounds(&fill).unwrap(), 800.0, 600.0));
    }

    #[test]
    fn text_without_advances_and_state_commands_are_never_culled() {
        let text = DisplayCommand::Text {
            text: "x".into(),
            x: 0.0,
            y: 1e6,
            color: Color::new(0, 0, 0, 1.0),
            font_size: 14.0,
            font_family: String::new(),
            font_weight: 400,
            font_style: 0,
            advances: None,
            ascent: None,
            run: None,
        };
        assert!(paint_bounds(&text).is_none());
        assert!(paint_bounds(&DisplayCommand::PushClip(Rect::new(0.0, 1e6, 1.0, 1.0))).is_none());
        assert!(paint_bounds(&DisplayCommand::PopClip).is_none());
    }

    #[test]
    fn a_rotation_is_tested_on_the_mapped_corners() {
        // 90° about the origin: x' = -y, y' = x. A box at x 100..120,
        // y -50..-30 lands at x' 30..50, y' 100..120: inside.
        let fill = DisplayCommand::SolidColor(
            Color::new(0, 0, 0, 1.0),
            Rect::new(100.0, -50.0, 20.0, 20.0),
        );
        let b = paint_bounds(&fill).unwrap();
        assert!(outside(ID, b, 800.0, 600.0));
        assert!(!outside([0.0, 1.0, -1.0, 0.0, 0.0, 0.0], b, 800.0, 600.0));
    }
}
