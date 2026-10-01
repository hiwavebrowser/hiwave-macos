"""Queue item 3: convert the renderer's border, clip and row-span code to elliptical corners.
One-shot source edit on the rs-elliptical-corners worktree; asserts every anchor matches once."""
p = '/Users/petecopeland/Repos/.worktrees/rs-control-semantics/crates/rustkit-renderer/src/lib.rs'
s = open(p, newline='').read()


def rep(old, new, count=1):
    global s
    assert s.count(old) == count, (s.count(old), old[:80])
    s = s.replace(old, new)


# ---- border
rep('''    /// `widths`/`colors` are `[top, right, bottom, left]`. Radii are clamped
    /// exactly as `draw_rounded_rect` clamps the background, so the ring and
    /// the fill it sits on share one outer curve. Each corner box is
    /// `max(radius, side width)` on each axis and is painted per pixel:
    /// coverage = outer curve − inner (padding-edge) curve, where the inner
    /// curve is the ellipse `(r − vertical width, r − horizontal width)`
    /// about the same centre''',
    '''    /// `widths`/`colors` are `[top, right, bottom, left]`. Radii are fitted
    /// exactly as `draw_rounded_rect` fits the background, so the ring and
    /// the fill it sits on share one outer curve. Each corner box is
    /// `max(radius, side width)` on each axis and is painted per pixel:
    /// coverage = outer curve − inner (padding-edge) curve, where the outer
    /// curve is the corner's ellipse `(h, v)` and the inner one the ellipse
    /// `(h − vertical width, v − horizontal width)` about the same centre''')
rep('''        let max_r = (rect.width / 2.0).min(rect.height / 2.0);
        let half_w = rect.width / 2.0;
        let half_h = rect.height / 2.0;
        // (radius, vertical side width, horizontal side width,
        //  vertical colour, horizontal colour, corner index)
        let corners = [
            (radius.top_left.min(max_r), l, t, colors[3], colors[0], 0u8),
            (radius.top_right.min(max_r), r, t, colors[1], colors[0], 1u8),
            (radius.bottom_right.min(max_r), r, b, colors[1], colors[2], 2u8),
            (radius.bottom_left.min(max_r), l, b, colors[3], colors[2], 3u8),
        ];
        // Corner box extents: width along x, height along y.
        let cw = |rad: f32, vw: f32| rad.max(vw).min(half_w);
        let ch = |rad: f32, hw: f32| rad.max(hw).min(half_h);
''', '''        let radius = radius.fitted(rect.width, rect.height);
        let half_w = rect.width / 2.0;
        let half_h = rect.height / 2.0;
        // (radii, vertical side width, horizontal side width,
        //  vertical colour, horizontal colour, corner index)
        let corners = [
            (radius.top_left, l, t, colors[3], colors[0], 0u8),
            (radius.top_right, r, t, colors[1], colors[0], 1u8),
            (radius.bottom_right, r, b, colors[1], colors[2], 2u8),
            (radius.bottom_left, l, b, colors[3], colors[2], 3u8),
        ];
        // Corner box extents: width along x, height along y. A side wider
        // than half the box stops at the middle; a fitted radius is already
        // kept clear of its neighbour.
        let cw = |rad: rustkit_layout::CornerRadius, vw: f32| rad.h.max(vw.min(half_w));
        let ch = |rad: rustkit_layout::CornerRadius, hw: f32| rad.v.max(hw.min(half_h));
''')
rep('''            let (rx, ry) = (rad - vw, rad - hw);
''', '''            let (rx, ry) = (rad.h - vw, rad.v - hw);
''')
rep('''                    let outer = if rad > 0.0 && ex < rad && ey < rad {
                        let d = ((rad - ex).powi(2) + (rad - ey).powi(2)).sqrt();
                        ((rad - d) * 0.5 + 0.5).clamp(0.0, 1.0)
                    } else {
                        1.0
                    };
                    let inner = if rx > 0.0 && ry > 0.0 && ex < rad && ey < rad {
                        let k = (((rad - ex) / rx).powi(2) + ((rad - ey) / ry).powi(2)).sqrt();
''', '''                    let outer = if !rad.is_zero() && ex < rad.h && ey < rad.v {
                        let d = ellipse_edge_distance(rad.h - ex, rad.v - ey, rad.h, rad.v);
                        (d * 0.5 + 0.5).clamp(0.0, 1.0)
                    } else {
                        1.0
                    };
                    let inner = if rx > 0.0 && ry > 0.0 && ex < rad.h && ey < rad.v {
                        let k = (((rad.h - ex) / rx).powi(2) + ((rad.v - ey) / ry).powi(2)).sqrt();
''')

# ---- gradient params (disabled GPU path): horizontal radii
for side, short in (('top_left', 'tl'), ('top_right', 'tr'), ('bottom_right', 'br'), ('bottom_left', 'bl')):
    rep(f'            radius_{short}: border_radius.{side},\n', f'            radius_{short}: border_radius.{side}.h,\n', 4)

# ---- clip under transform
rep('''            let scale = (m[0] * m[3]).abs().sqrt();
            let radius = rustkit_layout::BorderRadius {
                top_left: radius.top_left * scale,
                top_right: radius.top_right * scale,
                bottom_right: radius.bottom_right * scale,
                bottom_left: radius.bottom_left * scale,
            };
            clip_entry_for(current, screen, radius)
''', '''            // Axis-aligned, so each axis scales its own radii: a circular
            // corner under a non-uniform scale is an ellipse on screen.
            clip_entry_for(current, screen, radius.scaled(m[0].abs(), m[3].abs()))
''')

# ---- clamped_radii + rounded_row_span
a = s.index('/// Radii clamped so opposite corners cannot overlap, matching\n/// `point_in_rounded_rect` and `draw_rounded_rect`.')
b = s.index("/// Push one row's span as up to three pieces")
new = '''/// How far inside the ellipse with semi-axes `(h, v)` the point `(dx, dy)`
/// from its centre lies, in px: positive inside, negative outside.
///
/// A circle is exact. An ellipse has no closed-form distance, so it uses the
/// first-order estimate `(1 - k) / |grad k|` for `k = sqrt((dx/h)^2 +
/// (dy/v)^2)`, which is exact on the curve and good to a fraction of a pixel
/// in the one-pixel band the antialiasing reads.
///
/// Every rounded rasteriser (fill, border, gradient cells) measures its
/// corners with this, so they agree on where a corner's edge is.
fn ellipse_edge_distance(dx: f32, dy: f32, h: f32, v: f32) -> f32 {
    if h == v {
        return h - (dx * dx + dy * dy).sqrt();
    }
    let (nx, ny) = (dx / h, dy / v);
    let k = (nx * nx + ny * ny).sqrt();
    if k <= f32::EPSILON {
        return h.min(v);
    }
    let grad = ((nx / h).powi(2) + (ny / v).powi(2)).sqrt() / k;
    (1.0 - k) / grad
}

/// Pixel coverage for a signed distance to a corner's edge: half covered on
/// the edge, a two-pixel ramp across it.
fn corner_coverage(signed_dist: f32) -> f32 {
    if signed_dist >= 1.0 {
        1.0
    } else if signed_dist > -1.0 {
        (signed_dist * 0.5 + 0.5).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Top-left of the `h` x `v` box a corner occupies in `rect`.
/// quadrant: 0=top-left, 1=top-right, 2=bottom-right, 3=bottom-left
fn corner_box_origin(rect: Rect, corner: rustkit_layout::CornerRadius, quadrant: u8) -> (f32, f32) {
    let x = if quadrant == 0 || quadrant == 3 {
        rect.x
    } else {
        rect.x + rect.width - corner.h
    };
    let y = if quadrant == 0 || quadrant == 1 {
        rect.y
    } else {
        rect.y + rect.height - corner.v
    };
    (x, y)
}

/// Whether the boxes of two diagonally opposite corners of `rect` overlap.
/// Fitting keeps adjacent corners apart, not opposite ones: `80px 0` on a
/// 100px square is a leaf whose two corner boxes share the middle.
fn corner_boxes_overlap(
    rect: Rect,
    a: rustkit_layout::CornerRadius,
    b: rustkit_layout::CornerRadius,
) -> bool {
    !a.is_zero() && !b.is_zero() && a.h + b.h > rect.width && a.v + b.v > rect.height
}

/// Signed distance of the point `(px, py)` to the edge of `corner`
/// (`quadrant` of `rect`), or `None` when the point is outside that
/// corner's box and the corner says nothing about it.
fn corner_distance_at(
    rect: Rect,
    corner: rustkit_layout::CornerRadius,
    quadrant: u8,
    px: f32,
    py: f32,
) -> Option<f32> {
    if corner.is_zero() {
        return None;
    }
    let ex = if quadrant == 0 || quadrant == 3 {
        px - rect.x
    } else {
        rect.x + rect.width - px
    };
    let ey = if quadrant == 0 || quadrant == 1 {
        py - rect.y
    } else {
        rect.y + rect.height - py
    };
    if ex >= corner.h || ey >= corner.v {
        return None;
    }
    Some(ellipse_edge_distance(corner.h - ex, corner.v - ey, corner.h, corner.v))
}

/// How far a corner's arc reaches from its centre line on the row `dy` away
/// from it: the ellipse's `(h / v) * sqrt(v^2 - dy^2)`.
fn corner_row_reach(corner: rustkit_layout::CornerRadius, dy: f32) -> f32 {
    (corner.h / corner.v) * (corner.v * corner.v - dy * dy).max(0.0).sqrt()
}

/// The horizontal span of a rounded rect at height `y`, or `None` when the row
/// is outside it entirely.
///
/// Analytic rather than sampled: for a row crossing a corner, the ellipse
/// gives `dx = (h / v) * sqrt(v^2 - dy^2)` and the span shrinks by exactly
/// that much. The left
/// bound takes the tighter of the two left corners and the right bound the
/// tighter of the two right ones, so a row crossing both a top-left and a
/// bottom-left arc is handled without special-casing.
fn rounded_row_span(rect: Rect, radius: rustkit_layout::BorderRadius, y: f32) -> Option<(f32, f32)> {
    if y < rect.y || y > rect.bottom() {
        return None;
    }
    let radius = radius.fitted(rect.width, rect.height);
    let (tl, tr, br, bl) = (radius.top_left, radius.top_right, radius.bottom_right, radius.bottom_left);
    let mut left = rect.x;
    let mut right = rect.right();

    if !tl.is_zero() && y < rect.y + tl.v {
        let dx = corner_row_reach(tl, (rect.y + tl.v) - y);
        left = left.max(rect.x + tl.h - dx);
    }
    if !bl.is_zero() && y > rect.bottom() - bl.v {
        let dx = corner_row_reach(bl, y - (rect.bottom() - bl.v));
        left = left.max(rect.x + bl.h - dx);
    }
    if !tr.is_zero() && y < rect.y + tr.v {
        let dx = corner_row_reach(tr, (rect.y + tr.v) - y);
        right = right.min(rect.right() - tr.h + dx);
    }
    if !br.is_zero() && y > rect.bottom() - br.v {
        let dx = corner_row_reach(br, y - (rect.bottom() - br.v));
        right = right.min(rect.right() - br.h + dx);
    }

    if right > left {
        Some((left, right))
    } else {
        None
    }
}

'''
s = s[:a] + new + s[b:]
rep('''        let (tl, tr, br, bl) = clamped_radii(*rect, *radius);
        top_limit = top_limit.max(rect.y + tl.max(tr));
        bottom_limit = bottom_limit.min(rect.bottom() - bl.max(br));
''', '''        let fitted = radius.fitted(rect.width, rect.height);
        top_limit = top_limit.max(rect.y + fitted.top_left.v.max(fitted.top_right.v));
        bottom_limit = bottom_limit.min(rect.bottom() - fitted.bottom_left.v.max(fitted.bottom_right.v));
''')
open(p, 'w', newline='').write(s)
print('ok')
