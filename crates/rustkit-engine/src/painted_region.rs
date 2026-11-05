//! The boxes a frame paints over part of its view (CH3.A3b). Stub: the
//! derivation lands in the next commit.

use rustkit_layout::{DisplayList, Rect};

/// The boxes `list`, drawn at `scroll`, paints over `within`.
pub fn painted_boxes(_list: &DisplayList, _scroll: (f32, f32), _within: Rect) -> Vec<Rect> {
    Vec::new()
}
