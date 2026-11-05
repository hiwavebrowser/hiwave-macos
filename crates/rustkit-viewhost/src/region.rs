//! A view's *region* (CH3.A3): the rects, in view coordinates with a
//! top-left origin, where the view paints and takes the pointer. Outside
//! them the view is absent: not composited (a layer mask) and not hit
//! (`hitTest:` returns nil, so the view below gets the pointer).
//!
//! The RustKit chrome is the whole window, stacked over the page. Its
//! region is the window less the page's column (the *hole*), plus the
//! boxes of the chrome's popups that are showing (*overlays*). See
//! `trench/chrome/A3-composition.md`.

/// A rect in view coordinates, top-left origin, in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RegionRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl RegionRect {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self { x, y, width, height }
    }

    fn is_empty(&self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }

    /// Half-open: a point on the right or bottom edge is outside.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        let _ = (x, y);
        false
    }

    fn intersect(&self, other: &RegionRect) -> RegionRect {
        let _ = other;
        *self
    }
}

/// The region of a `width` x `height` view that is absent over `hole` and
/// present over each of `overlays`: the view less the hole (at most four
/// rects: above, below, left and right of it) and the overlays, each
/// clipped to the view. Empty rects are left out.
pub fn compose_region(
    width: f64,
    height: f64,
    hole: Option<RegionRect>,
    overlays: &[RegionRect],
) -> Vec<RegionRect> {
    let _ = (width, height, hole, overlays);
    Vec::new()
}

/// Whether the point is in the region.
pub fn region_contains(region: &[RegionRect], x: f64, y: f64) -> bool {
    region.iter().any(|r| r.contains(x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: f64, y: f64, w: f64, h: f64) -> RegionRect {
        RegionRect::new(x, y, w, h)
    }

    // The chrome at 1280x800: toolbar 104 high, sidebar 220 wide, no right
    // sidebar. The page column is the hole.
    const HOLE: RegionRect = RegionRect { x: 220.0, y: 104.0, width: 1060.0, height: 696.0 };

    #[test]
    fn the_toolbar_and_sidebar_are_in_the_region_and_the_page_column_is_not() {
        let region = compose_region(1280.0, 800.0, Some(HOLE), &[]);
        assert!(region_contains(&region, 600.0, 50.0), "toolbar");
        assert!(region_contains(&region, 100.0, 500.0), "sidebar");
        assert!(region_contains(&region, 0.0, 0.0), "top-left corner");
        assert!(!region_contains(&region, 600.0, 400.0), "page");
        assert!(!region_contains(&region, 220.0, 104.0), "the hole's top-left point");
        assert!(region_contains(&region, 219.9, 104.0), "just left of the hole");
        assert!(region_contains(&region, 220.0, 103.9), "just above the hole");
        assert!(!region_contains(&region, 1279.0, 799.0), "bottom-right corner is page");
    }

    #[test]
    fn the_view_less_a_hole_is_at_most_four_rects_and_none_is_empty() {
        let region = compose_region(1280.0, 800.0, Some(HOLE), &[]);
        assert!(region.len() <= 4, "{region:?}");
        assert!(region.iter().all(|r| r.width > 0.0 && r.height > 0.0), "{region:?}");
        // The hole touches the right and bottom edges: above and left only.
        assert_eq!(region.len(), 2, "{region:?}");
    }

    #[test]
    fn a_hole_inside_the_view_leaves_a_frame_around_it() {
        let hole = r(220.0, 104.0, 800.0, 600.0);
        let region = compose_region(1280.0, 800.0, Some(hole), &[]);
        assert_eq!(region.len(), 4, "{region:?}");
        assert!(region_contains(&region, 1100.0, 400.0), "right sidebar");
        assert!(region_contains(&region, 600.0, 750.0), "below the hole");
        assert!(!region_contains(&region, 600.0, 400.0));
    }

    #[test]
    fn an_open_popup_over_the_page_is_in_the_region_and_the_page_beside_it_is_not() {
        // The downloads panel: 320 wide, right-aligned, under the toolbar.
        let panel = r(942.0, 98.0, 320.0, 240.0);
        let region = compose_region(1280.0, 800.0, Some(HOLE), &[panel]);
        assert!(region_contains(&region, 1000.0, 200.0), "inside the panel, over the page");
        assert!(!region_contains(&region, 600.0, 200.0), "page left of the panel");
        assert!(!region_contains(&region, 1000.0, 400.0), "page below the panel");
    }

    #[test]
    fn an_overlay_is_clipped_to_the_view_and_an_empty_one_is_left_out() {
        let region = compose_region(
            1280.0,
            800.0,
            Some(HOLE),
            &[r(1200.0, 700.0, 300.0, 300.0), r(500.0, 500.0, 0.0, 40.0)],
        );
        assert!(region.iter().all(|q| q.x + q.width <= 1280.0 && q.y + q.height <= 800.0), "{region:?}");
        assert!(region_contains(&region, 1250.0, 750.0));
        assert!(!region_contains(&region, 500.0, 520.0));
        assert_eq!(region.len(), 3, "{region:?}");
    }

    #[test]
    fn no_hole_is_the_whole_view() {
        let region = compose_region(1280.0, 800.0, None, &[]);
        assert_eq!(region, vec![r(0.0, 0.0, 1280.0, 800.0)]);
    }

    #[test]
    fn a_hole_that_covers_the_view_leaves_only_the_overlays() {
        let region = compose_region(400.0, 300.0, Some(r(-10.0, -10.0, 500.0, 500.0)), &[r(10.0, 10.0, 20.0, 20.0)]);
        assert_eq!(region, vec![r(10.0, 10.0, 20.0, 20.0)]);
    }
}
