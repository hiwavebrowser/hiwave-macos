//! Scrolling and overflow handling for RustKit.
//!
//! This module implements:
//! - Scroll container detection
//! - Scroll position management
//! - Scrollbar rendering
//! - Scroll APIs (scrollTo, scrollBy, scrollIntoView)
//! - Smooth scrolling animation
//! - Position: sticky handling

use crate::{DisplayCommand, Rect};
use rustkit_css::{Color, Overflow, ScrollbarGutter, ScrollbarWidth};
use std::time::{Duration, Instant};

/// Scroll state for a scroll container.
#[derive(Debug, Clone)]
pub struct ScrollState {
    /// Current scroll position (horizontal).
    pub scroll_x: f32,

    /// Current scroll position (vertical).
    pub scroll_y: f32,

    /// Total scrollable width (content width - viewport width).
    pub scroll_width: f32,

    /// Total scrollable height (content height - viewport height).
    pub scroll_height: f32,

    /// Viewport width.
    pub viewport_width: f32,

    /// Viewport height.
    pub viewport_height: f32,

    /// Content width.
    pub content_width: f32,

    /// Content height.
    pub content_height: f32,

    /// Whether currently animating.
    pub animating: bool,

    /// Animation target position.
    pub target_x: f32,
    pub target_y: f32,

    /// Animation start position.
    pub start_x: f32,
    pub start_y: f32,

    /// Animation start time.
    pub animation_start: Option<Instant>,

    /// Animation duration.
    pub animation_duration: Duration,

    /// Momentum scrolling state.
    pub momentum: Option<ScrollMomentum>,
}

impl ScrollState {
    /// Create a new scroll state.
    pub fn new(viewport_width: f32, viewport_height: f32) -> Self {
        Self {
            scroll_x: 0.0,
            scroll_y: 0.0,
            scroll_width: 0.0,
            scroll_height: 0.0,
            viewport_width,
            viewport_height,
            content_width: viewport_width,
            content_height: viewport_height,
            animating: false,
            target_x: 0.0,
            target_y: 0.0,
            start_x: 0.0,
            start_y: 0.0,
            animation_start: None,
            animation_duration: Duration::from_millis(300),
            momentum: None,
        }
    }

    /// Update content size.
    pub fn set_content_size(&mut self, width: f32, height: f32) {
        self.content_width = width;
        self.content_height = height;
        self.scroll_width = (width - self.viewport_width).max(0.0);
        self.scroll_height = (height - self.viewport_height).max(0.0);

        // Clamp current scroll position
        self.scroll_x = self.scroll_x.clamp(0.0, self.scroll_width);
        self.scroll_y = self.scroll_y.clamp(0.0, self.scroll_height);
    }

    /// Update viewport size.
    pub fn set_viewport_size(&mut self, width: f32, height: f32) {
        self.viewport_width = width;
        self.viewport_height = height;
        self.scroll_width = (self.content_width - width).max(0.0);
        self.scroll_height = (self.content_height - height).max(0.0);

        // Clamp current scroll position
        self.scroll_x = self.scroll_x.clamp(0.0, self.scroll_width);
        self.scroll_y = self.scroll_y.clamp(0.0, self.scroll_height);
    }

    /// Scroll to a position (instant).
    pub fn scroll_to(&mut self, x: f32, y: f32) {
        self.scroll_x = x.clamp(0.0, self.scroll_width);
        self.scroll_y = y.clamp(0.0, self.scroll_height);
        self.animating = false;
        self.momentum = None;
    }

    /// Scroll to a position with smooth animation.
    pub fn scroll_to_smooth(&mut self, x: f32, y: f32, duration: Duration) {
        self.target_x = x.clamp(0.0, self.scroll_width);
        self.target_y = y.clamp(0.0, self.scroll_height);
        self.start_x = self.scroll_x;
        self.start_y = self.scroll_y;
        self.animation_start = Some(Instant::now());
        self.animation_duration = duration;
        self.animating = true;
        self.momentum = None;
    }

    /// Scroll by a delta amount (instant).
    pub fn scroll_by(&mut self, dx: f32, dy: f32) {
        self.scroll_to(self.scroll_x + dx, self.scroll_y + dy);
    }

    /// Scroll by a delta with smooth animation.
    pub fn scroll_by_smooth(&mut self, dx: f32, dy: f32, duration: Duration) {
        self.scroll_to_smooth(self.scroll_x + dx, self.scroll_y + dy, duration);
    }

    /// Update animation state (call each frame).
    pub fn update(&mut self) -> bool {
        // Handle smooth scrolling animation
        if self.animating {
            if let Some(start) = self.animation_start {
                let elapsed = start.elapsed();
                let progress = (elapsed.as_secs_f32() / self.animation_duration.as_secs_f32()).min(1.0);

                // Ease out cubic
                let eased = 1.0 - (1.0 - progress).powi(3);

                self.scroll_x = self.start_x + (self.target_x - self.start_x) * eased;
                self.scroll_y = self.start_y + (self.target_y - self.start_y) * eased;

                if progress >= 1.0 {
                    self.scroll_x = self.target_x;
                    self.scroll_y = self.target_y;
                    self.animating = false;
                    self.animation_start = None;
                }

                return true; // Needs repaint
            }
        }

        // Handle momentum scrolling
        if let Some(ref mut momentum) = self.momentum {
            let decay = 0.95; // Velocity decay per frame
            let threshold = 0.5; // Stop threshold

            self.scroll_x = (self.scroll_x + momentum.velocity_x).clamp(0.0, self.scroll_width);
            self.scroll_y = (self.scroll_y + momentum.velocity_y).clamp(0.0, self.scroll_height);

            momentum.velocity_x *= decay;
            momentum.velocity_y *= decay;

            // Stop if velocity is low or hit bounds
            let at_bounds_x = self.scroll_x <= 0.0 || self.scroll_x >= self.scroll_width;
            let at_bounds_y = self.scroll_y <= 0.0 || self.scroll_y >= self.scroll_height;

            if (momentum.velocity_x.abs() < threshold && momentum.velocity_y.abs() < threshold)
                || (at_bounds_x && at_bounds_y)
            {
                self.momentum = None;
            }

            return true; // Needs repaint
        }

        false
    }

    /// Start momentum scrolling.
    pub fn start_momentum(&mut self, velocity_x: f32, velocity_y: f32) {
        self.animating = false;
        self.momentum = Some(ScrollMomentum {
            velocity_x,
            velocity_y,
        });
    }

    /// Stop all scrolling animation.
    pub fn stop(&mut self) {
        self.animating = false;
        self.momentum = None;
    }

    /// Check if scrollable horizontally.
    pub fn can_scroll_x(&self) -> bool {
        self.scroll_width > 0.0
    }

    /// Check if scrollable vertically.
    pub fn can_scroll_y(&self) -> bool {
        self.scroll_height > 0.0
    }

    /// Get scroll progress (0.0 - 1.0) for horizontal.
    pub fn progress_x(&self) -> f32 {
        if self.scroll_width > 0.0 {
            self.scroll_x / self.scroll_width
        } else {
            0.0
        }
    }

    /// Get scroll progress (0.0 - 1.0) for vertical.
    pub fn progress_y(&self) -> f32 {
        if self.scroll_height > 0.0 {
            self.scroll_y / self.scroll_height
        } else {
            0.0
        }
    }
}

impl Default for ScrollState {
    fn default() -> Self {
        Self::new(0.0, 0.0)
    }
}

/// Momentum scrolling state.
#[derive(Debug, Clone)]
pub struct ScrollMomentum {
    /// Horizontal velocity (pixels per frame).
    pub velocity_x: f32,

    /// Vertical velocity (pixels per frame).
    pub velocity_y: f32,
}

/// Scrollbar orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollbarOrientation {
    Horizontal,
    Vertical,
}

/// Scrollbar state for rendering.
#[derive(Debug, Clone)]
pub struct Scrollbar {
    /// Orientation.
    pub orientation: ScrollbarOrientation,

    /// Track rectangle.
    pub track_rect: Rect,

    /// Thumb rectangle.
    pub thumb_rect: Rect,

    /// Whether the scrollbar is hovered.
    pub hovered: bool,

    /// Whether the thumb is being dragged.
    pub dragging: bool,

    /// Drag start position.
    pub drag_start: Option<f32>,

    /// Scroll position at drag start.
    pub drag_start_scroll: f32,
}

impl Scrollbar {
    /// Create a new scrollbar.
    pub fn new(orientation: ScrollbarOrientation, track_rect: Rect) -> Self {
        Self {
            orientation,
            track_rect,
            thumb_rect: Rect::default(),
            hovered: false,
            dragging: false,
            drag_start: None,
            drag_start_scroll: 0.0,
        }
    }

    /// Update thumb position based on scroll state.
    pub fn update_thumb(&mut self, scroll_state: &ScrollState, scrollbar_width: f32) {
        let (content_size, viewport_size, scroll_pos, track_size) = match self.orientation {
            ScrollbarOrientation::Horizontal => (
                scroll_state.content_width,
                scroll_state.viewport_width,
                scroll_state.scroll_x,
                self.track_rect.width,
            ),
            ScrollbarOrientation::Vertical => (
                scroll_state.content_height,
                scroll_state.viewport_height,
                scroll_state.scroll_y,
                self.track_rect.height,
            ),
        };

        if content_size <= viewport_size {
            // No scrolling needed, hide thumb
            self.thumb_rect = Rect::default();
            return;
        }

        // Calculate thumb size (proportional to viewport/content ratio)
        let thumb_size = (viewport_size / content_size * track_size).max(20.0).min(track_size);

        // Calculate thumb position
        let scroll_range = content_size - viewport_size;
        let track_range = track_size - thumb_size;
        let thumb_pos = if scroll_range > 0.0 {
            scroll_pos / scroll_range * track_range
        } else {
            0.0
        };

        self.thumb_rect = match self.orientation {
            ScrollbarOrientation::Horizontal => Rect {
                x: self.track_rect.x + thumb_pos,
                y: self.track_rect.y,
                width: thumb_size,
                height: scrollbar_width,
            },
            ScrollbarOrientation::Vertical => Rect {
                x: self.track_rect.x,
                y: self.track_rect.y + thumb_pos,
                width: scrollbar_width,
                height: thumb_size,
            },
        };
    }

    /// Check if a point is over the thumb.
    pub fn hit_test_thumb(&self, x: f32, y: f32) -> bool {
        x >= self.thumb_rect.x
            && x < self.thumb_rect.x + self.thumb_rect.width
            && y >= self.thumb_rect.y
            && y < self.thumb_rect.y + self.thumb_rect.height
    }

    /// Check if a point is over the track.
    pub fn hit_test_track(&self, x: f32, y: f32) -> bool {
        x >= self.track_rect.x
            && x < self.track_rect.x + self.track_rect.width
            && y >= self.track_rect.y
            && y < self.track_rect.y + self.track_rect.height
    }

    /// Start dragging the thumb.
    pub fn start_drag(&mut self, pos: f32, current_scroll: f32) {
        self.dragging = true;
        self.drag_start = Some(pos);
        self.drag_start_scroll = current_scroll;
    }

    /// Update drag position and return new scroll position.
    pub fn update_drag(&self, pos: f32, scroll_state: &ScrollState) -> Option<f32> {
        if !self.dragging {
            return None;
        }

        let drag_start = self.drag_start?;
        let delta = pos - drag_start;

        let (content_size, viewport_size, track_size) = match self.orientation {
            ScrollbarOrientation::Horizontal => (
                scroll_state.content_width,
                scroll_state.viewport_width,
                self.track_rect.width,
            ),
            ScrollbarOrientation::Vertical => (
                scroll_state.content_height,
                scroll_state.viewport_height,
                self.track_rect.height,
            ),
        };

        let thumb_size = (viewport_size / content_size * track_size).max(20.0).min(track_size);
        let track_range = track_size - thumb_size;
        let scroll_range = content_size - viewport_size;

        if track_range > 0.0 {
            let scroll_delta = delta / track_range * scroll_range;
            Some((self.drag_start_scroll + scroll_delta).clamp(0.0, scroll_range))
        } else {
            None
        }
    }

    /// End dragging.
    pub fn end_drag(&mut self) {
        self.dragging = false;
        self.drag_start = None;
    }
}

/// Render scrollbars for a scroll container.
pub fn render_scrollbars(
    scroll_state: &ScrollState,
    container_rect: Rect,
    scrollbar_width_setting: ScrollbarWidth,
    _scrollbar_gutter: ScrollbarGutter,
    scrollbar_color: Option<(Color, Color)>,
    overflow_x: Overflow,
    overflow_y: Overflow,
) -> Vec<DisplayCommand> {
    let mut commands = Vec::new();

    let scrollbar_width = match scrollbar_width_setting {
        ScrollbarWidth::Auto => 12.0,
        ScrollbarWidth::Thin => 8.0,
        ScrollbarWidth::None => return commands,
    };

    let (thumb_color, track_color) = scrollbar_color.unwrap_or((
        Color::new(128, 128, 128, 0.6),
        Color::new(200, 200, 200, 0.3),
    ));

    let show_vertical = overflow_y.is_scrollable() && scroll_state.can_scroll_y();
    let show_horizontal = overflow_x.is_scrollable() && scroll_state.can_scroll_x();

    // Vertical scrollbar
    if show_vertical {
        let track_rect = Rect {
            x: container_rect.x + container_rect.width - scrollbar_width,
            y: container_rect.y,
            width: scrollbar_width,
            height: container_rect.height - if show_horizontal { scrollbar_width } else { 0.0 },
        };

        // Track background
        commands.push(DisplayCommand::SolidColor(track_color, track_rect));

        // Thumb
        let mut scrollbar = Scrollbar::new(ScrollbarOrientation::Vertical, track_rect);
        scrollbar.update_thumb(scroll_state, scrollbar_width);

        if scrollbar.thumb_rect.height > 0.0 {
            commands.push(DisplayCommand::SolidColor(thumb_color, scrollbar.thumb_rect));
        }
    }

    // Horizontal scrollbar
    if show_horizontal {
        let track_rect = Rect {
            x: container_rect.x,
            y: container_rect.y + container_rect.height - scrollbar_width,
            width: container_rect.width - if show_vertical { scrollbar_width } else { 0.0 },
            height: scrollbar_width,
        };

        // Track background
        commands.push(DisplayCommand::SolidColor(track_color, track_rect));

        // Thumb
        let mut scrollbar = Scrollbar::new(ScrollbarOrientation::Horizontal, track_rect);
        scrollbar.update_thumb(scroll_state, scrollbar_width);

        if scrollbar.thumb_rect.width > 0.0 {
            commands.push(DisplayCommand::SolidColor(thumb_color, scrollbar.thumb_rect));
        }
    }

    // Corner piece if both scrollbars are shown
    if show_vertical && show_horizontal {
        let corner_rect = Rect {
            x: container_rect.x + container_rect.width - scrollbar_width,
            y: container_rect.y + container_rect.height - scrollbar_width,
            width: scrollbar_width,
            height: scrollbar_width,
        };
        commands.push(DisplayCommand::SolidColor(track_color, corner_rect));
    }

    commands
}

/// Check if an element should create a scroll container.
pub fn is_scroll_container(overflow_x: Overflow, overflow_y: Overflow) -> bool {
    overflow_x.is_scrollable() || overflow_y.is_scrollable()
}

/// Calculate scroll into view position.
pub fn calculate_scroll_into_view(
    element_rect: Rect,
    viewport_rect: Rect,
    scroll_state: &ScrollState,
    align_x: ScrollAlignment,
    align_y: ScrollAlignment,
) -> (f32, f32) {
    let mut new_scroll_x = scroll_state.scroll_x;
    let mut new_scroll_y = scroll_state.scroll_y;

    // Horizontal alignment
    let elem_left = element_rect.x - viewport_rect.x + scroll_state.scroll_x;
    let elem_right = elem_left + element_rect.width;
    let view_right = viewport_rect.width;

    match align_x {
        ScrollAlignment::Start => {
            new_scroll_x = elem_left;
        }
        ScrollAlignment::Center => {
            new_scroll_x = elem_left - (viewport_rect.width - element_rect.width) / 2.0;
        }
        ScrollAlignment::End => {
            new_scroll_x = elem_right - viewport_rect.width;
        }
        ScrollAlignment::Nearest => {
            if elem_left < scroll_state.scroll_x {
                new_scroll_x = elem_left;
            } else if elem_right > scroll_state.scroll_x + view_right {
                new_scroll_x = elem_right - view_right;
            }
        }
    }

    // Vertical alignment
    let elem_top = element_rect.y - viewport_rect.y + scroll_state.scroll_y;
    let elem_bottom = elem_top + element_rect.height;
    let view_bottom = viewport_rect.height;

    match align_y {
        ScrollAlignment::Start => {
            new_scroll_y = elem_top;
        }
        ScrollAlignment::Center => {
            new_scroll_y = elem_top - (viewport_rect.height - element_rect.height) / 2.0;
        }
        ScrollAlignment::End => {
            new_scroll_y = elem_bottom - viewport_rect.height;
        }
        ScrollAlignment::Nearest => {
            if elem_top < scroll_state.scroll_y {
                new_scroll_y = elem_top;
            } else if elem_bottom > scroll_state.scroll_y + view_bottom {
                new_scroll_y = elem_bottom - view_bottom;
            }
        }
    }

    (
        new_scroll_x.clamp(0.0, scroll_state.scroll_width),
        new_scroll_y.clamp(0.0, scroll_state.scroll_height),
    )
}

/// Scroll alignment for scrollIntoView.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollAlignment {
    Start,
    Center,
    End,
    #[default]
    Nearest,
}

/// Handle wheel event and return scroll delta.
pub fn handle_wheel_event(
    delta_x: f32,
    delta_y: f32,
    delta_mode: WheelDeltaMode,
    line_height: f32,
    page_height: f32,
) -> (f32, f32) {
    let multiplier = match delta_mode {
        WheelDeltaMode::Pixel => 1.0,
        WheelDeltaMode::Line => line_height,
        WheelDeltaMode::Page => page_height,
    };

    (delta_x * multiplier, delta_y * multiplier)
}

/// Wheel event delta mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WheelDeltaMode {
    #[default]
    Pixel,
    Line,
    Page,
}

/// Sticky position state.
#[derive(Debug, Clone)]
pub struct StickyState {
    /// Original position before sticking.
    pub original_rect: Rect,

    /// Computed stuck position.
    pub stuck_rect: Option<Rect>,

    /// Sticky offsets (top, right, bottom, left).
    pub offsets: StickyOffsets,

    /// Whether currently stuck.
    pub is_stuck: bool,
}

/// Sticky offsets.
#[derive(Debug, Clone, Copy, Default)]
pub struct StickyOffsets {
    pub top: Option<f32>,
    pub right: Option<f32>,
    pub bottom: Option<f32>,
    pub left: Option<f32>,
}

impl StickyState {
    /// Create a new sticky state.
    pub fn new(original_rect: Rect, offsets: StickyOffsets) -> Self {
        Self {
            original_rect,
            stuck_rect: None,
            offsets,
            is_stuck: false,
        }
    }

    /// Update sticky position based on scroll state.
    pub fn update(&mut self, scroll_y: f32, container_rect: Rect) {
        // Simplified sticky: only handle top sticky
        if let Some(top_offset) = self.offsets.top {
            let threshold = self.original_rect.y - top_offset;

            if scroll_y > threshold {
                // Element should stick
                let max_y = container_rect.y + container_rect.height - self.original_rect.height;
                let sticky_y = (container_rect.y + top_offset).min(max_y);

                self.stuck_rect = Some(Rect {
                    x: self.original_rect.x,
                    y: sticky_y,
                    width: self.original_rect.width,
                    height: self.original_rect.height,
                });
                self.is_stuck = true;
            } else {
                self.stuck_rect = None;
                self.is_stuck = false;
            }
        }
    }

    /// Get the effective rect (stuck or original).
    pub fn effective_rect(&self) -> Rect {
        self.stuck_rect.unwrap_or(self.original_rect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scroll_state_creation() {
        let state = ScrollState::new(800.0, 600.0);
        assert_eq!(state.viewport_width, 800.0);
        assert_eq!(state.viewport_height, 600.0);
        assert_eq!(state.scroll_x, 0.0);
        assert_eq!(state.scroll_y, 0.0);
    }

    #[test]
    fn test_scroll_to() {
        let mut state = ScrollState::new(800.0, 600.0);
        state.set_content_size(1600.0, 1200.0);

        state.scroll_to(100.0, 200.0);
        assert_eq!(state.scroll_x, 100.0);
        assert_eq!(state.scroll_y, 200.0);

        // Test clamping
        state.scroll_to(2000.0, 2000.0);
        assert_eq!(state.scroll_x, 800.0); // 1600 - 800
        assert_eq!(state.scroll_y, 600.0); // 1200 - 600
    }

    #[test]
    fn test_scroll_by() {
        let mut state = ScrollState::new(800.0, 600.0);
        state.set_content_size(1600.0, 1200.0);

        state.scroll_by(50.0, 100.0);
        assert_eq!(state.scroll_x, 50.0);
        assert_eq!(state.scroll_y, 100.0);

        state.scroll_by(50.0, 100.0);
        assert_eq!(state.scroll_x, 100.0);
        assert_eq!(state.scroll_y, 200.0);
    }

    #[test]
    fn test_scroll_progress() {
        let mut state = ScrollState::new(100.0, 100.0);
        state.set_content_size(200.0, 200.0);

        state.scroll_to(50.0, 75.0);
        assert!((state.progress_x() - 0.5).abs() < 0.01);
        assert!((state.progress_y() - 0.75).abs() < 0.01);
    }

    #[test]
    fn test_scrollbar_thumb_position() {
        let mut state = ScrollState::new(800.0, 600.0);
        state.set_content_size(800.0, 1200.0);

        let track_rect = Rect {
            x: 788.0,
            y: 0.0,
            width: 12.0,
            height: 600.0,
        };

        let mut scrollbar = Scrollbar::new(ScrollbarOrientation::Vertical, track_rect);
        scrollbar.update_thumb(&state, 12.0);

        // Thumb should be at top, size should be proportional
        assert_eq!(scrollbar.thumb_rect.y, 0.0);
        assert!(scrollbar.thumb_rect.height > 0.0);
        assert!(scrollbar.thumb_rect.height < 600.0);
    }

    #[test]
    fn test_overflow_is_scrollable() {
        assert!(Overflow::Scroll.is_scrollable());
        assert!(Overflow::Auto.is_scrollable());
        assert!(!Overflow::Hidden.is_scrollable());
        assert!(!Overflow::Visible.is_scrollable());
    }

    #[test]
    fn test_wheel_delta_modes() {
        let line_height = 20.0;
        let page_height = 600.0;

        let (_dx, dy) = handle_wheel_event(0.0, 3.0, WheelDeltaMode::Line, line_height, page_height);
        assert_eq!(dy, 60.0);

        let (_dx, dy) = handle_wheel_event(0.0, 1.0, WheelDeltaMode::Page, line_height, page_height);
        assert_eq!(dy, 600.0);
    }

    #[test]
    fn test_sticky_state() {
        let original = Rect { x: 0.0, y: 100.0, width: 200.0, height: 50.0 };
        let offsets = StickyOffsets { top: Some(0.0), ..Default::default() };
        let mut sticky = StickyState::new(original, offsets);

        let container = Rect { x: 0.0, y: 0.0, width: 800.0, height: 600.0 };

        // Not scrolled - should not be stuck
        sticky.update(0.0, container);
        assert!(!sticky.is_stuck);

        // Scrolled past threshold - should stick
        sticky.update(150.0, container);
        assert!(sticky.is_stuck);
        assert_eq!(sticky.effective_rect().y, 0.0);
    }
}

