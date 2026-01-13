//! # CSS Grid Layout
//!
//! Implementation of the CSS Grid Layout algorithm.
//!
//! ## Overview
//!
//! Grid layout is a two-dimensional layout system that places items in rows and columns.
//! It supports:
//! - Explicit tracks (grid-template-columns/rows)
//! - Implicit tracks (grid-auto-columns/rows)
//! - Named lines and areas
//! - Flexible sizing (fr units)
//! - Auto-placement algorithm
//!
//! ## References
//!
//! - [CSS Grid Layout Module Level 1](https://www.w3.org/TR/css-grid-1/)
//! - [CSS Grid Layout Module Level 2](https://www.w3.org/TR/css-grid-2/)

use rustkit_css::{
    AlignItems, AlignSelf, BoxSizing, Display, GridAutoFlow, GridLine, GridPlacement,
    GridTemplate, JustifyItems, JustifySelf, Length, TrackDefinition, TrackRepeat, TrackSize,
};
use tracing::{debug, trace};

use crate::{LayoutBox, Rect};

// ==================== Grid Container ====================

/// A resolved grid track (computed from template).
#[derive(Debug, Clone)]
pub struct GridTrack {
    /// Base size (minimum).
    pub base_size: f32,
    /// Growth limit (maximum).
    pub growth_limit: f32,
    /// Whether this track has flexible sizing.
    pub is_flexible: bool,
    /// Flex factor (fr value).
    pub flex_factor: f32,
    /// Percentage value if this is a percentage track (0.0-100.0).
    /// For minmax(%, x), this stores the min percentage.
    pub percent: Option<f32>,
    /// Percentage value for the max bound in minmax(x, %).
    pub max_percent: Option<f32>,
    /// Whether this track uses min-content sizing.
    pub is_min_content: bool,
    /// Whether this track uses max-content sizing.
    pub is_max_content: bool,
    /// For fit-content(length), the maximum length constraint.
    pub fit_content_limit: Option<f32>,
    /// Whether this track is from auto-fit (should collapse if empty).
    pub is_auto_fit: bool,
    /// Final computed size.
    pub size: f32,
    /// Position (offset from container start).
    pub position: f32,
    /// Line names before this track.
    pub line_names: Vec<String>,
}

impl GridTrack {
    /// Create a new track with default sizing.
    pub fn new(size: &TrackSize) -> Self {
        // Extract percentage values if present (for min and max bounds)
        let (percent, max_percent) = match size {
            TrackSize::Percent(p) => (Some(*p), None),
            TrackSize::MinMax(min, max) => {
                let min_pct = if let TrackSize::Percent(p) = min.as_ref() {
                    Some(*p)
                } else {
                    None
                };
                let max_pct = if let TrackSize::Percent(p) = max.as_ref() {
                    Some(*p)
                } else {
                    None
                };
                (min_pct, max_pct)
            }
            _ => (None, None),
        };

        // Determine if this track uses intrinsic sizing
        let (is_min_content, is_max_content) = match size {
            TrackSize::MinContent => (true, false),
            TrackSize::MaxContent => (false, true),
            TrackSize::MinMax(min, max) => {
                let min_is_min = matches!(min.as_ref(), TrackSize::MinContent);
                let max_is_max = matches!(max.as_ref(), TrackSize::MaxContent);
                (min_is_min, max_is_max)
            }
            TrackSize::FitContent(_) => (true, false), // fit-content uses min-content as minimum
            TrackSize::Auto => (true, true), // auto behaves like minmax(min-content, max-content)
            _ => (false, false),
        };

        let (base_size, growth_limit, flex_factor) = match size {
            TrackSize::Px(v) => (*v, *v, 0.0),
            TrackSize::Percent(_) => (0.0, f32::INFINITY, 0.0), // Will be resolved later
            TrackSize::Fr(fr) => (0.0, f32::INFINITY, *fr),
            TrackSize::MinContent => (0.0, 0.0, 0.0), // Will be computed from content
            TrackSize::MaxContent => (0.0, f32::INFINITY, 0.0), // Will be computed from content
            TrackSize::Auto => (0.0, f32::INFINITY, 0.0), // Will be computed from content
            TrackSize::MinMax(min, max) => {
                // For min: use 0 if it's intrinsic or percentage (resolved later)
                let min_size = match min.as_ref() {
                    TrackSize::Percent(_) | TrackSize::MinContent | TrackSize::MaxContent | TrackSize::Auto => 0.0,
                    _ => Self::new(min).base_size,
                };
                // For max: use INFINITY if it's intrinsic, percentage, or flexible
                let max_size = match max.as_ref() {
                    TrackSize::Percent(_) | TrackSize::MinContent | TrackSize::MaxContent | TrackSize::Auto => f32::INFINITY,
                    _ => Self::new(max).growth_limit,
                };
                let flex = if max.is_flexible() {
                    if let TrackSize::Fr(fr) = max.as_ref() {
                        *fr
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };
                (min_size, max_size, flex)
            }
            TrackSize::FitContent(max) => (0.0, *max, 0.0),
        };

        // Extract fit-content limit if present
        let fit_content_limit = match size {
            TrackSize::FitContent(max) => Some(*max),
            _ => None,
        };

        Self {
            base_size,
            // For flexible tracks, keep growth_limit as INFINITY
            // For non-flexible tracks with INFINITY growth limit, clamp to base_size
            growth_limit: if flex_factor > 0.0 {
                f32::INFINITY
            } else if growth_limit == f32::INFINITY {
                base_size
            } else {
                growth_limit
            },
            is_flexible: flex_factor > 0.0,
            flex_factor,
            percent,
            max_percent,
            is_min_content,
            is_max_content,
            fit_content_limit,
            is_auto_fit: false,
            size: base_size,
            position: 0.0,
            line_names: Vec::new(),
        }
    }

    /// Create an implicit track.
    pub fn implicit(size: &TrackSize) -> Self {
        Self::new(size)
    }
}

/// A grid item with placement information.
#[derive(Debug, Clone)]
pub struct GridItem<'a> {
    /// Reference to the layout box.
    pub layout_box: &'a LayoutBox,
    /// Column start line (1-based).
    pub column_start: i32,
    /// Column end line (1-based).
    pub column_end: i32,
    /// Row start line (1-based).
    pub row_start: i32,
    /// Row end line (1-based).
    pub row_end: i32,
    /// Whether this item needs auto-placement for columns.
    pub auto_column: bool,
    /// Whether this item needs auto-placement for rows.
    pub auto_row: bool,
    /// Computed column span.
    pub column_span: u32,
    /// Computed row span.
    pub row_span: u32,
    /// Computed position and size.
    pub rect: Rect,
}

impl<'a> GridItem<'a> {
    /// Create a new grid item from a layout box.
    pub fn new(layout_box: &'a LayoutBox) -> Self {
        Self {
            layout_box,
            column_start: 0,
            column_end: 0,
            row_start: 0,
            row_end: 0,
            auto_column: true,
            auto_row: true,
            column_span: 1,
            row_span: 1,
            rect: Rect::default(),
        }
    }

    /// Whether this item needs any auto-placement.
    pub fn needs_auto_placement(&self) -> bool {
        self.auto_column || self.auto_row
    }

    /// Whether this item is fully explicitly placed (no auto-placement needed).
    pub fn is_fully_placed(&self) -> bool {
        !self.auto_column && !self.auto_row
    }

    /// Get the item's contribution to row sizing.
    /// This considers explicit heights, min-heights, and intrinsic content.
    pub fn get_height_contribution(&self, container_height: f32) -> f32 {
        let style = &self.layout_box.style;
        
        // Check for explicit height
        match &style.height {
            Length::Px(h) => return *h,
            Length::Percent(p) if container_height > 0.0 => {
                return container_height * p / 100.0;
            }
            _ => {}
        }
        
        // Check for min-height
        let min_height = match &style.min_height {
            Length::Px(h) => *h,
            Length::Percent(p) if container_height > 0.0 => container_height * p / 100.0,
            _ => 0.0,
        };
        
        // For auto height, estimate based on content
        // This is a simplified calculation - a full implementation would
        // do a layout pass to determine content height
        let content_height = self.estimate_content_height();
        
        min_height.max(content_height)
    }

    /// Estimate content height (simplified).
    fn estimate_content_height(&self) -> f32 {
        // Get font size for text content
        let font_size = match self.layout_box.style.font_size {
            Length::Px(px) => px,
            _ => 16.0,
        };

        // Get line height
        let line_height = if self.layout_box.style.line_height > 0.0 {
            self.layout_box.style.line_height
        } else {
            1.2
        };

        // Count text children (simplified)
        let text_lines = self.count_text_lines();

        // Padding contribution
        let padding_top = self.layout_box.style.padding_top.to_px(font_size, font_size, 0.0);
        let padding_bottom = self.layout_box.style.padding_bottom.to_px(font_size, font_size, 0.0);

        let text_content = if text_lines > 0 {
            font_size * line_height * text_lines as f32
        } else {
            0.0
        };

        // Also consider children's min-heights and actual content
        let children_height = self.estimate_children_height(font_size);

        // Use max of text content or children content
        let content = text_content.max(children_height);

        content + padding_top + padding_bottom
    }

    /// Estimate height from children (including min-height).
    fn estimate_children_height(&self, font_size: f32) -> f32 {
        let mut total_height = 0.0f32;
        let mut max_height = 0.0f32;

        for child in &self.layout_box.children {
            let child_style = &child.style;

            // Check child's min-height
            let min_height = match child_style.min_height {
                Length::Px(h) => h,
                Length::Em(em) => em * font_size,
                _ => 0.0,
            };

            // Check child's explicit height
            let explicit_height = match child_style.height {
                Length::Px(h) => h,
                Length::Em(em) => em * font_size,
                _ => 0.0,
            };

            // If child has display: flex, it might have content
            let child_font_size = match child_style.font_size {
                Length::Px(px) => px,
                _ => font_size,
            };
            let child_line_height = if child_style.line_height > 0.0 {
                child_style.line_height
            } else {
                1.2
            };

            // Estimate child's content height
            let child_content_height = if let crate::BoxType::Text(_) = &child.box_type {
                child_font_size * child_line_height
            } else {
                // Recursively estimate children
                let nested_height: f32 = child.children.iter()
                    .map(|c| {
                        let c_min = match c.style.min_height {
                            Length::Px(h) => h,
                            _ => 0.0,
                        };
                        let c_height = match c.style.height {
                            Length::Px(h) => h,
                            _ => 0.0,
                        };
                        c_min.max(c_height)
                    })
                    .sum();
                nested_height
            };

            let child_height = min_height.max(explicit_height).max(child_content_height);

            // For absolute positioned children, don't count their height in flow
            if matches!(child_style.position, rustkit_css::Position::Absolute | rustkit_css::Position::Fixed) {
                // Skip absolutely positioned children for height calculation
                continue;
            }

            // Accumulate heights (block-level) or take max (inline-level)
            if matches!(child_style.display, rustkit_css::Display::Block | rustkit_css::Display::Flex | rustkit_css::Display::Grid) {
                total_height += child_height;
            } else {
                max_height = max_height.max(child_height);
            }
        }

        total_height.max(max_height)
    }

    /// Count approximate text lines in this item.
    fn count_text_lines(&self) -> usize {
        fn count_text(layout_box: &LayoutBox) -> usize {
            let mut count = 0;
            if let crate::BoxType::Text(_) = &layout_box.box_type {
                count += 1;
            }
            for child in &layout_box.children {
                count += count_text(child);
            }
            count
        }
        count_text(self.layout_box)
    }

    /// Get the item's contribution to column sizing.
    pub fn get_width_contribution(&self, container_width: f32) -> f32 {
        let style = &self.layout_box.style;
        
        // Check for explicit width
        match &style.width {
            Length::Px(w) => return *w,
            Length::Percent(p) if container_width > 0.0 => {
                return container_width * p / 100.0;
            }
            _ => {}
        }
        
        // Check for min-width
        let min_width = match &style.min_width {
            Length::Px(w) => *w,
            Length::Percent(p) if container_width > 0.0 => container_width * p / 100.0,
            _ => 0.0,
        };
        
        min_width
    }

    /// Set explicit placement from style.
    pub fn set_placement(&mut self, placement: &GridPlacement) {
        // Start with both dimensions needing auto-placement
        self.auto_column = true;
        self.auto_row = true;

        // Resolve column placement
        match (&placement.column_start, &placement.column_end) {
            (GridLine::Number(start), GridLine::Number(end)) => {
                self.column_start = *start;
                self.column_end = *end;
                self.auto_column = false;
            }
            (GridLine::Number(start), GridLine::Auto) => {
                self.column_start = *start;
                self.column_end = start + 1;
                self.auto_column = false;
            }
            (GridLine::Number(start), GridLine::Span(span)) => {
                self.column_start = *start;
                self.column_end = start + *span as i32;
                self.auto_column = false;
            }
            (GridLine::Auto, GridLine::Number(end)) => {
                self.column_end = *end;
                self.column_start = end - 1;
                self.auto_column = false;
            }
            (GridLine::Span(span), _) => {
                self.column_span = *span;
                // Still needs auto-placement, but with specified span
            }
            _ => {
                // Auto placement for columns
            }
        }

        // Resolve row placement
        match (&placement.row_start, &placement.row_end) {
            (GridLine::Number(start), GridLine::Number(end)) => {
                self.row_start = *start;
                self.row_end = *end;
                self.auto_row = false;
            }
            (GridLine::Number(start), GridLine::Auto) => {
                self.row_start = *start;
                self.row_end = start + 1;
                self.auto_row = false;
            }
            (GridLine::Number(start), GridLine::Span(span)) => {
                self.row_start = *start;
                self.row_end = start + *span as i32;
                self.auto_row = false;
            }
            (GridLine::Auto, GridLine::Number(end)) => {
                self.row_end = *end;
                self.row_start = end - 1;
                self.auto_row = false;
            }
            (GridLine::Span(span), _) => {
                self.row_span = *span;
                // Still needs auto-placement, but with specified span
            }
            _ => {
                // Auto placement
            }
        }

        // Update spans from placement
        if self.column_start != 0 && self.column_end != 0 {
            self.column_span = (self.column_end - self.column_start).unsigned_abs();
        }
        if self.row_start != 0 && self.row_end != 0 {
            self.row_span = (self.row_end - self.row_start).unsigned_abs();
        }
    }
}

/// Stored auto-repeat pattern for layout-time expansion.
#[derive(Debug, Clone)]
pub struct AutoRepeatPattern {
    /// Track definitions to repeat.
    pub tracks: Vec<TrackDefinition>,
    /// Whether this is auto-fit (collapse empty) vs auto-fill.
    pub is_auto_fit: bool,
    /// Insert position in the track list.
    pub insert_position: usize,
}

/// Grid layout state.
#[derive(Debug)]
pub struct GridLayout {
    /// Column tracks.
    pub columns: Vec<GridTrack>,
    /// Row tracks.
    pub rows: Vec<GridTrack>,
    /// Column gap.
    pub column_gap: f32,
    /// Row gap.
    pub row_gap: f32,
    /// Auto-flow direction.
    pub auto_flow: GridAutoFlow,
    /// Auto-placement cursor (column, row).
    pub cursor: (usize, usize),
    /// Number of explicit columns.
    pub explicit_columns: usize,
    /// Number of explicit rows.
    pub explicit_rows: usize,
    /// Pending auto-repeat for columns (resolved at layout time).
    pub column_auto_repeat: Option<AutoRepeatPattern>,
    /// Pending auto-repeat for rows (resolved at layout time).
    pub row_auto_repeat: Option<AutoRepeatPattern>,
}

impl GridLayout {
    /// Create a new grid layout from style.
    pub fn new(
        template_columns: &GridTemplate,
        template_rows: &GridTemplate,
        _auto_columns: &TrackSize,
        _auto_rows: &TrackSize,
        column_gap: f32,
        row_gap: f32,
        auto_flow: GridAutoFlow,
    ) -> Self {
        // Expand repeat() patterns in column template
        let (expanded_columns, col_auto_repeat) = template_columns.expand_tracks();

        // Extract auto-repeat pattern for columns if present
        let column_auto_repeat = Self::extract_auto_repeat(template_columns, col_auto_repeat);

        // Create explicit column tracks from expanded template
        let columns: Vec<GridTrack> = expanded_columns
            .iter()
            .map(|def| {
                let mut track = GridTrack::new(&def.size);
                track.line_names = def.line_names.clone();
                track
            })
            .collect();

        // Expand repeat() patterns in row template
        let (expanded_rows, row_auto_repeat) = template_rows.expand_tracks();

        // Extract auto-repeat pattern for rows if present
        let row_auto_repeat = Self::extract_auto_repeat(template_rows, row_auto_repeat);

        // Create explicit row tracks from expanded template
        let rows: Vec<GridTrack> = expanded_rows
            .iter()
            .map(|def| {
                let mut track = GridTrack::new(&def.size);
                track.line_names = def.line_names.clone();
                track
            })
            .collect();

        let explicit_columns = columns.len();
        let explicit_rows = rows.len();

        Self {
            columns,
            rows,
            column_gap,
            row_gap,
            auto_flow,
            cursor: (0, 0),
            explicit_columns,
            explicit_rows,
            column_auto_repeat,
            row_auto_repeat,
        }
    }

    /// Extract auto-repeat pattern from template.
    fn extract_auto_repeat(
        template: &GridTemplate,
        auto_repeat: Option<&TrackRepeat>,
    ) -> Option<AutoRepeatPattern> {
        auto_repeat.and_then(|repeat| {
            // Find insert position from template repeats
            let insert_pos = template
                .repeats
                .iter()
                .find_map(|(pos, r)| {
                    if matches!(r, TrackRepeat::AutoFill(_) | TrackRepeat::AutoFit(_)) {
                        Some(*pos)
                    } else {
                        None
                    }
                })
                .unwrap_or(0);

            match repeat {
                TrackRepeat::AutoFill(tracks) => Some(AutoRepeatPattern {
                    tracks: tracks.clone(),
                    is_auto_fit: false,
                    insert_position: insert_pos,
                }),
                TrackRepeat::AutoFit(tracks) => Some(AutoRepeatPattern {
                    tracks: tracks.clone(),
                    is_auto_fit: true,
                    insert_position: insert_pos,
                }),
                TrackRepeat::Count(_, _) => None, // Already expanded
            }
        })
    }

    /// Expand auto-fill/auto-fit patterns now that we have container size.
    ///
    /// Per CSS Grid spec:
    /// - Calculate how many repetitions fit in the available space
    /// - Insert the repeated tracks at the stored insert position
    /// - For auto-fit, empty tracks will be collapsed to 0 during sizing
    pub fn expand_auto_repeats(&mut self, container_width: f32, container_height: f32) {
        // Expand column auto-repeat
        if let Some(pattern) = self.column_auto_repeat.take() {
            let available = container_width - (self.columns.len().saturating_sub(1)) as f32 * self.column_gap;
            let new_tracks = Self::calculate_auto_repeat_tracks(&pattern, available, self.column_gap);

            // Insert at the stored position
            let insert_at = pattern.insert_position.min(self.columns.len());
            for (i, track) in new_tracks.into_iter().enumerate() {
                self.columns.insert(insert_at + i, track);
            }
            self.explicit_columns = self.columns.len();
        }

        // Expand row auto-repeat
        if let Some(pattern) = self.row_auto_repeat.take() {
            let available = container_height - (self.rows.len().saturating_sub(1)) as f32 * self.row_gap;
            let new_tracks = Self::calculate_auto_repeat_tracks(&pattern, available, self.row_gap);

            // Insert at the stored position
            let insert_at = pattern.insert_position.min(self.rows.len());
            for (i, track) in new_tracks.into_iter().enumerate() {
                self.rows.insert(insert_at + i, track);
            }
            self.explicit_rows = self.rows.len();
        }
    }

    /// Calculate how many tracks to create for auto-fill/auto-fit.
    ///
    /// Returns a Vec of GridTrack to insert.
    fn calculate_auto_repeat_tracks(
        pattern: &AutoRepeatPattern,
        available_space: f32,
        gap: f32,
    ) -> Vec<GridTrack> {
        if pattern.tracks.is_empty() {
            return Vec::new();
        }

        // Calculate the fixed size of one repetition of the pattern
        let pattern_fixed_size: f32 = pattern
            .tracks
            .iter()
            .map(|def| Self::get_track_definite_size(&def.size))
            .sum();

        // Include gaps between tracks in one repetition
        let pattern_gaps = if pattern.tracks.len() > 1 {
            (pattern.tracks.len() - 1) as f32 * gap
        } else {
            0.0
        };

        let single_repetition_size = pattern_fixed_size + pattern_gaps;

        // If pattern has no definite size (all fr units), create exactly 1 repetition
        if single_repetition_size <= 0.0 {
            let tracks: Vec<GridTrack> = pattern
                .tracks
                .iter()
                .map(|def| {
                    let mut track = GridTrack::new(&def.size);
                    track.line_names = def.line_names.clone();
                    track
                })
                .collect();
            return tracks;
        }

        // Calculate how many repetitions fit
        // Account for gaps between repetitions
        let mut repetitions = 1u32;
        let mut total_size = single_repetition_size;

        while total_size + gap + single_repetition_size <= available_space {
            repetitions += 1;
            total_size += gap + single_repetition_size;
        }

        // Per spec, at least 1 repetition
        repetitions = repetitions.max(1);

        trace!(
            "auto-repeat: {} repetitions fit in {}px (pattern size: {}px)",
            repetitions,
            available_space,
            single_repetition_size
        );

        // Create the tracks
        let mut result = Vec::with_capacity(repetitions as usize * pattern.tracks.len());
        for _ in 0..repetitions {
            for def in &pattern.tracks {
                let mut track = GridTrack::new(&def.size);
                track.line_names = def.line_names.clone();
                // Mark for auto-fit collapsing (handled during sizing)
                if pattern.is_auto_fit {
                    track.is_auto_fit = true;
                }
                result.push(track);
            }
        }

        result
    }

    /// Get the definite (fixed) size of a track for auto-repeat calculations.
    /// Returns 0 for flexible tracks (fr) since they don't contribute to fixed size.
    fn get_track_definite_size(size: &TrackSize) -> f32 {
        match size {
            TrackSize::Px(px) => *px,
            TrackSize::MinMax(min, max) => {
                // Use the definite bound
                let min_size = Self::get_track_definite_size(min);
                let max_size = Self::get_track_definite_size(max);
                // If max is definite, use it; otherwise use min
                if max_size > 0.0 {
                    max_size
                } else {
                    min_size
                }
            }
            TrackSize::FitContent(max) => *max,
            // Flexible and intrinsic sizes are not definite
            TrackSize::Fr(_)
            | TrackSize::Percent(_)
            | TrackSize::MinContent
            | TrackSize::MaxContent
            | TrackSize::Auto => 0.0,
        }
    }

    /// Collapse empty auto-fit column tracks.
    ///
    /// For auto-fit, empty tracks (tracks with no items spanning them)
    /// should be treated as having a fixed sizing function of 0px.
    pub fn collapse_empty_auto_fit_columns(&mut self, column_occupied: &[bool]) {
        for (i, track) in self.columns.iter_mut().enumerate() {
            if track.is_auto_fit {
                let has_items = column_occupied.get(i).copied().unwrap_or(false);
                if !has_items {
                    // Collapse this track to 0
                    track.base_size = 0.0;
                    track.growth_limit = 0.0;
                    track.size = 0.0;
                    track.is_flexible = false;
                    track.flex_factor = 0.0;
                    trace!("Collapsed empty auto-fit column track {}", i);
                }
            }
        }
    }

    /// Collapse empty auto-fit row tracks.
    pub fn collapse_empty_auto_fit_rows(&mut self, row_occupied: &[bool]) {
        for (i, track) in self.rows.iter_mut().enumerate() {
            if track.is_auto_fit {
                let has_items = row_occupied.get(i).copied().unwrap_or(false);
                if !has_items {
                    // Collapse this track to 0
                    track.base_size = 0.0;
                    track.growth_limit = 0.0;
                    track.size = 0.0;
                    track.is_flexible = false;
                    track.flex_factor = 0.0;
                    trace!("Collapsed empty auto-fit row track {}", i);
                }
            }
        }
    }

    /// Ensure we have enough tracks for an item.
    pub fn ensure_tracks(&mut self, col_end: usize, row_end: usize, auto_columns: &TrackSize, auto_rows: &TrackSize) {
        while self.columns.len() < col_end {
            self.columns.push(GridTrack::implicit(auto_columns));
        }
        while self.rows.len() < row_end {
            self.rows.push(GridTrack::implicit(auto_rows));
        }
    }

    /// Get number of columns.
    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    /// Get number of rows.
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Find next available cell for auto-placement.
    pub fn find_next_cell(&self, col_span: usize, row_span: usize, occupied: &[Vec<bool>]) -> (usize, usize) {
        let (mut col, mut row) = self.cursor;

        if self.auto_flow.is_row() {
            // Row-major placement
            loop {
                if col + col_span <= self.column_count() {
                    // Check if cells are available
                    let available = (0..row_span).all(|dr| {
                        (0..col_span).all(|dc| {
                            let r = row + dr;
                            let c = col + dc;
                            r >= occupied.len() || c >= occupied.get(r).map_or(0, |row| row.len()) || !occupied[r][c]
                        })
                    });

                    if available {
                        return (col, row);
                    }
                }

                col += 1;
                if col + col_span > self.column_count().max(1) {
                    col = 0;
                    row += 1;
                }

                // Safety limit
                if row > 1000 {
                    break;
                }
            }
        } else {
            // Column-major placement
            loop {
                if row + row_span <= self.row_count() {
                    let available = (0..row_span).all(|dr| {
                        (0..col_span).all(|dc| {
                            let r = row + dr;
                            let c = col + dc;
                            r >= occupied.len() || c >= occupied.get(r).map_or(0, |row| row.len()) || !occupied[r][c]
                        })
                    });

                    if available {
                        return (col, row);
                    }
                }

                row += 1;
                if row + row_span > self.row_count().max(1) {
                    row = 0;
                    col += 1;
                }

                if col > 1000 {
                    break;
                }
            }
        }

        (col, row)
    }

    /// Find next available row at a specific column for items with explicit column placement.
    pub fn find_next_row_at_column(&self, col_start: usize, col_span: usize, row_span: usize, occupied: &[Vec<bool>]) -> usize {
        let mut row = 0;
        
        loop {
            // Check if cells are available at this row for the given column range
            let available = (0..row_span).all(|dr| {
                (0..col_span).all(|dc| {
                    let r = row + dr;
                    let c = col_start + dc;
                    r >= occupied.len() || c >= occupied.get(r).map_or(0, |row_vec| row_vec.len()) || !occupied[r][c]
                })
            });

            if available {
                return row;
            }

            row += 1;

            // Safety limit
            if row > 1000 {
                break;
            }
        }

        row
    }

    /// Find next available column at a specific row for items with explicit row placement.
    pub fn find_next_column_at_row(&self, row_start: usize, col_span: usize, row_span: usize, occupied: &[Vec<bool>]) -> usize {
        let mut col = 0;
        
        loop {
            if col + col_span <= self.column_count() {
                // Check if cells are available at this column for the given row range
                let available = (0..row_span).all(|dr| {
                    (0..col_span).all(|dc| {
                        let r = row_start + dr;
                        let c = col + dc;
                        r >= occupied.len() || c >= occupied.get(r).map_or(0, |row_vec| row_vec.len()) || !occupied[r][c]
                    })
                });

                if available {
                    return col;
                }
            }

            col += 1;

            // Safety limit
            if col > 1000 {
                break;
            }
        }

        col
    }
}

// ==================== Layout Algorithm ====================

/// Lay out a grid container and its items.
pub fn layout_grid_container(
    container: &mut LayoutBox,
    container_width: f32,
    container_height: f32,
) {
    let style = &container.style;

    // Skip if not a grid container
    if !style.display.is_grid() {
        return;
    }

    debug!(
        "Grid layout: container {}x{}, {} children",
        container_width,
        container_height,
        container.children.len()
    );

    // Compute gaps
    let column_gap = style.column_gap.to_px(16.0, 16.0, container_width);
    let row_gap = style.row_gap.to_px(16.0, 16.0, container_height);

    // Create grid layout
    let mut grid = GridLayout::new(
        &style.grid_template_columns,
        &style.grid_template_rows,
        &style.grid_auto_columns,
        &style.grid_auto_rows,
        column_gap,
        row_gap,
        style.grid_auto_flow,
    );

    // Expand auto-fill/auto-fit patterns now that we have container size
    grid.expand_auto_repeats(container_width, container_height);

    // Ensure at least one column and row
    if grid.columns.is_empty() {
        grid.columns.push(GridTrack::implicit(&TrackSize::Auto));
    }
    if grid.rows.is_empty() {
        grid.rows.push(GridTrack::implicit(&TrackSize::Auto));
    }

    // Collect items with placement info
    let mut items: Vec<GridItem> = container
        .children
        .iter()
        .filter(|child| child.style.display != Display::None)
        .map(|child| {
            let mut item = GridItem::new(child);
            // Set placement from style
            let placement = GridPlacement {
                column_start: child.style.grid_column_start.clone(),
                column_end: child.style.grid_column_end.clone(),
                row_start: child.style.grid_row_start.clone(),
                row_end: child.style.grid_row_end.clone(),
            };
            item.set_placement(&placement);
            item
        })
        .collect();

    // Helper to resolve negative grid lines (e.g., -1 = last line)
    // In CSS Grid, negative indices count from the end: -1 is the last line
    let resolve_line = |line: i32, track_count: usize| -> i32 {
        if line < 0 {
            // -1 means last line, which is track_count + 1 in 1-based indexing
            // -2 means second-to-last, etc.
            (track_count as i32 + 1) + line + 1
        } else {
            line
        }
    };

    // Phase 1: Place items with explicit placement in BOTH dimensions
    let mut occupied: Vec<Vec<bool>> = Vec::new();

    for item in items.iter_mut().filter(|i| i.is_fully_placed()) {
        // Resolve negative line numbers before converting to 0-based indices
        let resolved_col_start = resolve_line(item.column_start, grid.column_count());
        let resolved_col_end = resolve_line(item.column_end, grid.column_count());
        let resolved_row_start = resolve_line(item.row_start, grid.row_count());
        let resolved_row_end = resolve_line(item.row_end, grid.row_count());

        // Convert to 0-based indices
        let col_start = (resolved_col_start - 1).max(0) as usize;
        let col_end = resolved_col_end.max(resolved_col_start + 1) as usize;
        let row_start = (resolved_row_start - 1).max(0) as usize;
        let row_end = resolved_row_end.max(resolved_row_start + 1) as usize;

        // Ensure grid has enough tracks
        grid.ensure_tracks(col_end, row_end, &style.grid_auto_columns, &style.grid_auto_rows);

        // Mark cells as occupied
        while occupied.len() < row_end {
            occupied.push(vec![false; grid.column_count()]);
        }
        for row in &mut occupied {
            while row.len() < grid.column_count() {
                row.push(false);
            }
        }

        for r in row_start..row_end {
            for c in col_start..col_end {
                if r < occupied.len() && c < occupied[r].len() {
                    occupied[r][c] = true;
                }
            }
        }

        // Update item with resolved placement
        item.column_start = col_start as i32 + 1;
        item.column_end = col_end as i32 + 1;
        item.row_start = row_start as i32 + 1;
        item.row_end = row_end as i32 + 1;
    }

    // Phase 2: Place items with explicit column but auto row (e.g., grid-column: 1 / -1)
    for item in items.iter_mut().filter(|i| !i.auto_column && i.auto_row) {
        // Resolve negative line numbers for columns
        let resolved_col_start = resolve_line(item.column_start, grid.column_count());
        let resolved_col_end = resolve_line(item.column_end, grid.column_count());
        
        // Convert to 0-based indices
        let col_start = (resolved_col_start - 1).max(0) as usize;
        let col_end = resolved_col_end.max(resolved_col_start + 1) as usize;
        let col_span = col_end.saturating_sub(col_start).max(1);
        let row_span = item.row_span.max(1) as usize;

        // Ensure grid has enough column tracks
        grid.ensure_tracks(col_end, grid.row_count(), &style.grid_auto_columns, &style.grid_auto_rows);

        // Find the first available row at this column position
        let row = grid.find_next_row_at_column(col_start, col_span, row_span, &occupied);
        let row_end = row + row_span;

        // Ensure grid has enough row tracks
        grid.ensure_tracks(col_end, row_end, &style.grid_auto_columns, &style.grid_auto_rows);

        // Ensure occupied grid is large enough
        while occupied.len() < row_end {
            occupied.push(vec![false; grid.column_count()]);
        }
        for occ_row in &mut occupied {
            while occ_row.len() < grid.column_count() {
                occ_row.push(false);
            }
        }

        // Mark cells as occupied
        for r in row..row_end {
            for c in col_start..col_end {
                if r < occupied.len() && c < occupied[r].len() {
                    occupied[r][c] = true;
                }
            }
        }

        // Update item placement (1-based)
        item.column_start = col_start as i32 + 1;
        item.column_end = col_end as i32 + 1;
        item.row_start = row as i32 + 1;
        item.row_end = row_end as i32 + 1;
        item.column_span = col_span as u32;
        item.row_span = row_span as u32;

        trace!(
            "Placed item with explicit columns ({}-{}) at row {}",
            col_start, col_end, row
        );
    }

    // Phase 3: Place items with explicit row but auto column
    for item in items.iter_mut().filter(|i| i.auto_column && !i.auto_row) {
        // Resolve negative line numbers for rows
        let resolved_row_start = resolve_line(item.row_start, grid.row_count());
        let resolved_row_end = resolve_line(item.row_end, grid.row_count());
        
        // Convert to 0-based indices
        let row_start = (resolved_row_start - 1).max(0) as usize;
        let row_end = resolved_row_end.max(resolved_row_start + 1) as usize;
        let row_span = row_end.saturating_sub(row_start).max(1);
        let col_span = item.column_span.max(1) as usize;

        // Ensure grid has enough row tracks
        grid.ensure_tracks(grid.column_count(), row_end, &style.grid_auto_columns, &style.grid_auto_rows);

        // Find the first available column at this row position
        let col = grid.find_next_column_at_row(row_start, col_span, row_span, &occupied);
        let col_end = col + col_span;

        // Ensure grid has enough column tracks
        grid.ensure_tracks(col_end, row_end, &style.grid_auto_columns, &style.grid_auto_rows);

        // Ensure occupied grid is large enough
        while occupied.len() < row_end {
            occupied.push(vec![false; grid.column_count()]);
        }
        for occ_row in &mut occupied {
            while occ_row.len() < grid.column_count() {
                occ_row.push(false);
            }
        }

        // Mark cells as occupied
        for r in row_start..row_end {
            for c in col..col_end {
                if r < occupied.len() && c < occupied[r].len() {
                    occupied[r][c] = true;
                }
            }
        }

        // Update item placement (1-based)
        item.column_start = col as i32 + 1;
        item.column_end = col_end as i32 + 1;
        item.row_start = row_start as i32 + 1;
        item.row_end = row_end as i32 + 1;
        item.column_span = col_span as u32;
        item.row_span = row_span as u32;

        trace!(
            "Placed item with explicit rows ({}-{}) at column {}",
            row_start, row_end, col
        );
    }

    // Phase 4: Auto-place remaining items (no explicit placement in either dimension)
    for item in items.iter_mut().filter(|i| i.auto_column && i.auto_row) {
        let col_span = item.column_span.max(1) as usize;
        let row_span = item.row_span.max(1) as usize;

        // Ensure grid has enough tracks
        grid.ensure_tracks(
            grid.column_count().max(col_span),
            grid.row_count().max(row_span),
            &style.grid_auto_columns,
            &style.grid_auto_rows,
        );

        // Find next available position
        let (col, row) = grid.find_next_cell(col_span, row_span, &occupied);

        // Ensure tracks exist
        grid.ensure_tracks(col + col_span, row + row_span, &style.grid_auto_columns, &style.grid_auto_rows);

        // Ensure occupied grid is large enough
        while occupied.len() < row + row_span {
            occupied.push(vec![false; grid.column_count()]);
        }
        for occ_row in &mut occupied {
            while occ_row.len() < grid.column_count() {
                occ_row.push(false);
            }
        }

        // Mark cells as occupied
        for r in row..row + row_span {
            for c in col..col + col_span {
                if r < occupied.len() && c < occupied[r].len() {
                    occupied[r][c] = true;
                }
            }
        }

        // Update item placement (1-based)
        item.column_start = col as i32 + 1;
        item.column_end = (col + col_span) as i32 + 1;
        item.row_start = row as i32 + 1;
        item.row_end = (row + row_span) as i32 + 1;
        item.column_span = col_span as u32;
        item.row_span = row_span as u32;

        // Update cursor
        grid.cursor = if grid.auto_flow.is_row() {
            (col + col_span, row)
        } else {
            (col, row + row_span)
        };

        trace!(
            "Auto-placed item at ({}, {}) span ({}, {})",
            col, row, col_span, row_span
        );
    }

    // Phase 4.5: Collapse empty auto-fit tracks
    // For auto-fit, tracks with no items spanning them collapse to 0
    {
        // Calculate which columns have items
        let mut column_occupied = vec![false; grid.column_count()];
        let mut row_occupied = vec![false; grid.row_count()];

        for item in &items {
            let col_start = (item.column_start - 1).max(0) as usize;
            let col_end = (item.column_end - 1).max(0) as usize;
            let row_start = (item.row_start - 1).max(0) as usize;
            let row_end = (item.row_end - 1).max(0) as usize;

            for c in col_start..col_end.min(column_occupied.len()) {
                column_occupied[c] = true;
            }
            for r in row_start..row_end.min(row_occupied.len()) {
                row_occupied[r] = true;
            }
        }

        // Collapse empty auto-fit tracks
        grid.collapse_empty_auto_fit_columns(&column_occupied);
        grid.collapse_empty_auto_fit_rows(&row_occupied);
    }

    // Phase 5: Size tracks with item contributions
    // First, collect item contributions for each row
    let mut row_contributions: Vec<f32> = vec![0.0; grid.row_count()];
    let mut col_contributions: Vec<f32> = vec![0.0; grid.column_count()];
    
    for item in &items {
        let row_start_idx = (item.row_start - 1).max(0) as usize;
        let row_span = item.row_span.max(1) as usize;
        let col_start_idx = (item.column_start - 1).max(0) as usize;
        let col_span = item.column_span.max(1) as usize;
        
        // Get item height contribution
        let height_contrib = item.get_height_contribution(container_height);
        let width_contrib = item.get_width_contribution(container_width);
        
        // Distribute height across spanned rows
        if row_span > 0 && height_contrib > 0.0 {
            let per_row = height_contrib / row_span as f32;
            for r in row_start_idx..(row_start_idx + row_span).min(row_contributions.len()) {
                row_contributions[r] = row_contributions[r].max(per_row);
            }
        }
        
        // Distribute width across spanned columns
        if col_span > 0 && width_contrib > 0.0 {
            let per_col = width_contrib / col_span as f32;
            for c in col_start_idx..(col_start_idx + col_span).min(col_contributions.len()) {
                col_contributions[c] = col_contributions[c].max(per_col);
            }
        }
    }
    
    // Apply contributions to tracks
    for (i, track) in grid.rows.iter_mut().enumerate() {
        if i < row_contributions.len() && row_contributions[i] > track.base_size {
            track.base_size = row_contributions[i];
        }
    }
    for (i, track) in grid.columns.iter_mut().enumerate() {
        if i < col_contributions.len() && col_contributions[i] > track.base_size {
            track.base_size = col_contributions[i];
        }
    }
    
    size_grid_tracks(&mut grid.columns, container_width, column_gap);
    size_grid_tracks(&mut grid.rows, container_height, row_gap);

    // Phase 6: Position items
    let content_x = container.dimensions.content.x;
    let content_y = container.dimensions.content.y;

    for item in &mut items {
        // Get track positions
        let col_start_idx = (item.column_start - 1).max(0) as usize;
        let col_end_idx = (item.column_end - 1).max(0) as usize;
        let row_start_idx = (item.row_start - 1).max(0) as usize;
        let row_end_idx = (item.row_end - 1).max(0) as usize;

        // Calculate position
        let x = if col_start_idx < grid.columns.len() {
            grid.columns[col_start_idx].position
        } else {
            0.0
        };

        let y = if row_start_idx < grid.rows.len() {
            grid.rows[row_start_idx].position
        } else {
            0.0
        };

        // Calculate size (sum of tracks + gaps)
        let width: f32 = (col_start_idx..col_end_idx.min(grid.columns.len()))
            .map(|i| grid.columns[i].size)
            .sum::<f32>()
            + (col_end_idx.saturating_sub(col_start_idx).saturating_sub(1)) as f32 * column_gap;

        let height: f32 = (row_start_idx..row_end_idx.min(grid.rows.len()))
            .map(|i| grid.rows[i].size)
            .sum::<f32>()
            + (row_end_idx.saturating_sub(row_start_idx).saturating_sub(1)) as f32 * row_gap;

        item.rect = Rect {
            x: content_x + x,
            y: content_y + y,
            width,
            height,
        };

        trace!(
            "Item at ({}-{}, {}-{}) -> rect {:?}",
            item.column_start, item.column_end,
            item.row_start, item.row_end,
            item.rect
        );
    }

    // Phase 7: Collect final positions (drops immutable borrow of children)
    let item_count = items.len();
    let positions: Vec<Rect> = items.iter().map(|item| item.rect.clone()).collect();
    drop(items); // Explicitly drop to release borrow

    // Phase 8: Apply positions to children
    let mut position_idx = 0;
    for child in container.children.iter_mut() {
        if child.style.display == Display::None {
            continue;
        }

        if let Some(rect) = positions.get(position_idx) {
            // Apply alignment - returns border-box dimensions
            let (x, border_box_width) = apply_justify_self(
                &child.style.justify_self,
                &style.justify_items,
                rect.x,
                rect.width,
                child,
            );

            let (y, border_box_height) = apply_align_self(
                &child.style.align_self,
                &style.align_items,
                rect.y,
                rect.height,
                child,
            );

            // Calculate padding and border
            let font_size = match child.style.font_size {
                Length::Px(px) => px,
                _ => 16.0,
            };
            let padding_left = child.style.padding_left.to_px(font_size, font_size, border_box_width);
            let padding_right = child.style.padding_right.to_px(font_size, font_size, border_box_width);
            let padding_top = child.style.padding_top.to_px(font_size, font_size, border_box_height);
            let padding_bottom = child.style.padding_bottom.to_px(font_size, font_size, border_box_height);
            let border_left = child.style.border_left_width.to_px(font_size, font_size, border_box_width);
            let border_right = child.style.border_right_width.to_px(font_size, font_size, border_box_width);
            let border_top = child.style.border_top_width.to_px(font_size, font_size, border_box_height);
            let border_bottom = child.style.border_bottom_width.to_px(font_size, font_size, border_box_height);

            // Set padding and border dimensions
            child.dimensions.padding.left = padding_left;
            child.dimensions.padding.right = padding_right;
            child.dimensions.padding.top = padding_top;
            child.dimensions.padding.bottom = padding_bottom;
            child.dimensions.border.left = border_left;
            child.dimensions.border.right = border_right;
            child.dimensions.border.top = border_top;
            child.dimensions.border.bottom = border_bottom;

            // Calculate content dimensions based on box-sizing
            let is_border_box = child.style.box_sizing == BoxSizing::BorderBox;
            let (content_width, content_height) = if is_border_box {
                // With border-box, the specified size includes padding and border
                let content_w = (border_box_width - padding_left - padding_right - border_left - border_right).max(0.0);
                let content_h = (border_box_height - padding_top - padding_bottom - border_top - border_bottom).max(0.0);
                (content_w, content_h)
            } else {
                // With content-box, the specified size is just the content
                (border_box_width, border_box_height)
            };

            // Position includes padding and border offset
            child.dimensions.content.x = x + padding_left + border_left;
            child.dimensions.content.y = y + padding_top + border_top;
            child.dimensions.content.width = content_width;
            child.dimensions.content.height = content_height;
        }
        position_idx += 1;
    }

    // Phase 9: Recursively layout children of grid items
    for child in container.children.iter_mut() {
        if child.style.display == Display::None {
            continue;
        }
        
        if !child.children.is_empty() {
            if child.style.display.is_flex() {
                // Nested flex container
                let child_containing = child.dimensions.clone();
                crate::flex::layout_flex_container(child, &child_containing);
            } else if child.style.display.is_grid() {
                // Nested grid container
                layout_grid_container(
                    child,
                    child.dimensions.content.width,
                    child.dimensions.content.height,
                );
            } else {
                // Block container: lay out children normally
                for grandchild in &mut child.children {
                    let cb = child.dimensions.clone();
                    grandchild.layout(&cb);
                }
            }
        }
    }

    debug!(
        "Grid layout complete: {} columns, {} rows, {} items",
        grid.column_count(),
        grid.row_count(),
        item_count
    );
}

/// Size grid tracks using the track sizing algorithm.
fn size_grid_tracks(tracks: &mut [GridTrack], container_size: f32, gap: f32) {
    if tracks.is_empty() {
        return;
    }

    // Count non-collapsed tracks for gap calculation
    // Collapsed (auto-fit empty) tracks are explicitly marked is_auto_fit and have all sizing zeroed
    // A track is collapsed only if it's an auto-fit track with no content
    let non_collapsed_count = tracks
        .iter()
        .filter(|t| {
            // A track is NOT collapsed if:
            // - It's not an auto-fit track, OR
            // - It has some size (base_size, growth_limit, percent, flex)
            !t.is_auto_fit
                || t.base_size > 0.0
                || t.growth_limit > 0.0
                || t.is_flexible
                || t.percent.is_some()
                || t.max_percent.is_some()
        })
        .count();
    let total_gaps = non_collapsed_count.saturating_sub(1) as f32 * gap;
    let available_space = (container_size - total_gaps).max(0.0);

    // Step 1: Initialize base sizes
    for track in tracks.iter_mut() {
        track.size = track.base_size;
    }

    // Step 2: Resolve percentage tracks against container size
    // Per spec, percentage tracks are resolved against the content box of the grid container
    for track in tracks.iter_mut() {
        // Resolve min percentage (base_size)
        if let Some(pct) = track.percent {
            let resolved_size = container_size * (pct / 100.0);
            track.base_size = resolved_size;
            track.size = resolved_size;
            // If no max percentage and not flexible, growth_limit = base_size
            if track.max_percent.is_none() && !track.is_flexible {
                track.growth_limit = resolved_size;
            }
        }
        // Resolve max percentage (growth_limit)
        if let Some(pct) = track.max_percent {
            let resolved_size = container_size * (pct / 100.0);
            track.growth_limit = resolved_size;
            // If track hasn't been sized yet, use base_size
            if track.size < track.base_size {
                track.size = track.base_size;
            }
        }
    }

    // Step 2.5: Handle intrinsic tracks (min-content, max-content, auto, fit-content)
    // Item contributions should already be set in base_size from layout_grid_container
    for track in tracks.iter_mut() {
        if track.is_min_content {
            // min-content track: size is the minimum content size
            // base_size should already have the item contribution
            track.size = track.base_size;
            // For pure min-content, growth_limit = base_size (no growth allowed)
            if !track.is_max_content && track.fit_content_limit.is_none() {
                track.growth_limit = track.base_size;
            }
        }
        if track.is_max_content {
            // max-content track: can grow to fit content
            // base_size has min contribution, allow growth
            track.size = track.base_size;
            // growth_limit stays at INFINITY or is set based on max-content
            // For pure max-content, we want to expand to fill
            if track.growth_limit == 0.0 {
                track.growth_limit = f32::INFINITY;
            }
        }
        // Handle fit-content(length): clamp growth_limit to the specified length
        // fit-content behaves like minmax(min-content, min(max-content, length))
        if let Some(limit) = track.fit_content_limit {
            // Base size is already set from min-content contribution
            track.size = track.base_size;
            // Cap growth at the specified limit
            track.growth_limit = limit.min(track.growth_limit);
            // But growth_limit should be at least base_size
            track.growth_limit = track.growth_limit.max(track.base_size);
        }
    }

    // Step 3: Distribute remaining space to flexible tracks
    let fixed_size: f32 = tracks.iter().filter(|t| !t.is_flexible).map(|t| t.size).sum();
    let flex_space = (available_space - fixed_size).max(0.0);

    let total_flex: f32 = tracks.iter().filter(|t| t.is_flexible).map(|t| t.flex_factor).sum();

    if total_flex > 0.0 {
        let flex_unit = flex_space / total_flex;
        for track in tracks.iter_mut().filter(|t| t.is_flexible) {
            track.size = (track.flex_factor * flex_unit).max(track.base_size);
            // Respect growth limit
            if track.growth_limit < f32::INFINITY {
                track.size = track.size.min(track.growth_limit);
            }
        }
    }

    // Step 4: Distribute remaining space to auto tracks if any space left
    let used_space: f32 = tracks.iter().map(|t| t.size).sum();
    let remaining = (available_space - used_space).max(0.0);

    if remaining > 0.0 {
        let auto_tracks: Vec<usize> = tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| !t.is_flexible && t.growth_limit > t.size)
            .map(|(i, _)| i)
            .collect();

        if !auto_tracks.is_empty() {
            let per_track = remaining / auto_tracks.len() as f32;
            for i in auto_tracks {
                tracks[i].size += per_track;
            }
        }
    }

    // Step 5: Calculate positions
    // For auto-fit, collapsed tracks (size = 0) should not have gaps
    let mut position = 0.0;
    let mut prev_was_collapsed = true; // Start true to skip gap before first track
    for track in tracks.iter_mut() {
        // Add gap only if previous track was not collapsed and current track is not collapsed
        if !prev_was_collapsed && track.size > 0.0 {
            position += gap;
        }
        track.position = position;
        position += track.size;
        prev_was_collapsed = track.size == 0.0;
    }
}

/// Apply justify-self alignment.
fn apply_justify_self(
    self_align: &JustifySelf,
    items_align: &JustifyItems,
    cell_x: f32,
    cell_width: f32,
    child: &LayoutBox,
) -> (f32, f32) {
    let align = match self_align {
        JustifySelf::Auto => match items_align {
            JustifyItems::Start => JustifySelf::Start,
            JustifyItems::End => JustifySelf::End,
            JustifyItems::Center => JustifySelf::Center,
            JustifyItems::Stretch => JustifySelf::Stretch,
        },
        other => *other,
    };

    // Check if width is explicitly set (not auto)
    let has_explicit_width = !matches!(child.style.width, Length::Auto);
    let child_width = match child.style.width {
        Length::Auto => cell_width,
        Length::Px(w) => w,
        Length::Percent(p) => cell_width * p / 100.0,
        _ => cell_width,
    };

    match align {
        JustifySelf::Start | JustifySelf::Auto => (cell_x, child_width),
        JustifySelf::End => (cell_x + cell_width - child_width, child_width),
        JustifySelf::Center => (cell_x + (cell_width - child_width) / 2.0, child_width),
        // Per CSS spec: stretch only applies when width is auto
        JustifySelf::Stretch => {
            if has_explicit_width {
                (cell_x, child_width)
            } else {
                (cell_x, cell_width)
            }
        },
    }
}

/// Apply align-self alignment.
fn apply_align_self(
    self_align: &AlignSelf,
    items_align: &AlignItems,
    cell_y: f32,
    cell_height: f32,
    child: &LayoutBox,
) -> (f32, f32) {
    let align = match self_align {
        AlignSelf::Auto => match items_align {
            AlignItems::FlexStart => AlignSelf::FlexStart,
            AlignItems::FlexEnd => AlignSelf::FlexEnd,
            AlignItems::Center => AlignSelf::Center,
            AlignItems::Stretch => AlignSelf::Stretch,
            AlignItems::Baseline => AlignSelf::Baseline,
        },
        other => *other,
    };

    // Check if height is explicitly set (not auto)
    let has_explicit_height = !matches!(child.style.height, Length::Auto);
    let child_height = match child.style.height {
        Length::Auto => cell_height,
        Length::Px(h) => h,
        Length::Percent(p) => cell_height * p / 100.0,
        _ => cell_height,
    };

    match align {
        AlignSelf::FlexStart | AlignSelf::Auto => (cell_y, child_height),
        AlignSelf::FlexEnd => (cell_y + cell_height - child_height, child_height),
        AlignSelf::Center => (cell_y + (cell_height - child_height) / 2.0, child_height),
        // Per CSS spec: stretch only applies when height is auto
        AlignSelf::Stretch => {
            if has_explicit_height {
                (cell_y, child_height)
            } else {
                (cell_y, cell_height)
            }
        },
        AlignSelf::Baseline => (cell_y, child_height), // Simplified
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BoxType;
    use rustkit_css::{ComputedStyle, GridTemplateAreas};

    #[allow(dead_code)]
    fn create_test_container() -> LayoutBox {
        let mut style = ComputedStyle::new();
        style.display = Display::Grid;
        style.grid_template_columns = GridTemplate::from_sizes(vec![
            TrackSize::Fr(1.0),
            TrackSize::Fr(1.0),
        ]);
        style.grid_template_rows = GridTemplate::from_sizes(vec![
            TrackSize::Px(100.0),
            TrackSize::Px(100.0),
        ]);

        LayoutBox::new(BoxType::Block, style)
    }

    #[test]
    fn test_grid_track_creation() {
        let track = GridTrack::new(&TrackSize::Px(100.0));
        assert_eq!(track.base_size, 100.0);
        assert_eq!(track.size, 100.0);
        assert!(!track.is_flexible);

        let fr_track = GridTrack::new(&TrackSize::Fr(2.0));
        assert!(fr_track.is_flexible);
        assert_eq!(fr_track.flex_factor, 2.0);
    }

    #[test]
    fn test_grid_layout_creation() {
        let template_cols = GridTemplate::from_sizes(vec![
            TrackSize::Fr(1.0),
            TrackSize::Fr(2.0),
        ]);
        let template_rows = GridTemplate::from_sizes(vec![
            TrackSize::Px(100.0),
        ]);

        let grid = GridLayout::new(
            &template_cols,
            &template_rows,
            &TrackSize::Auto,
            &TrackSize::Auto,
            10.0,
            10.0,
            GridAutoFlow::Row,
        );

        assert_eq!(grid.column_count(), 2);
        assert_eq!(grid.row_count(), 1);
    }

    #[test]
    fn test_track_sizing() {
        let mut tracks = vec![
            GridTrack::new(&TrackSize::Fr(1.0)),
            GridTrack::new(&TrackSize::Fr(2.0)),
        ];

        size_grid_tracks(&mut tracks, 300.0, 0.0);

        // 1fr + 2fr = 3fr, so 1fr = 100px, 2fr = 200px
        assert_eq!(tracks[0].size, 100.0);
        assert_eq!(tracks[1].size, 200.0);
    }

    #[test]
    fn test_track_sizing_with_fixed() {
        let mut tracks = vec![
            GridTrack::new(&TrackSize::Px(50.0)),
            GridTrack::new(&TrackSize::Fr(1.0)),
        ];

        size_grid_tracks(&mut tracks, 300.0, 0.0);

        assert_eq!(tracks[0].size, 50.0);
        assert_eq!(tracks[1].size, 250.0);
    }

    #[test]
    fn test_track_positions() {
        let mut tracks = vec![
            GridTrack::new(&TrackSize::Px(100.0)),
            GridTrack::new(&TrackSize::Px(100.0)),
            GridTrack::new(&TrackSize::Px(100.0)),
        ];

        size_grid_tracks(&mut tracks, 320.0, 10.0);

        assert_eq!(tracks[0].position, 0.0);
        assert_eq!(tracks[1].position, 110.0);
        assert_eq!(tracks[2].position, 220.0);
    }

    #[test]
    fn test_auto_placement() {
        let template_cols = GridTemplate::from_sizes(vec![
            TrackSize::Fr(1.0),
            TrackSize::Fr(1.0),
        ]);
        let template_rows = GridTemplate::from_sizes(vec![
            TrackSize::Auto,
        ]);

        let grid = GridLayout::new(
            &template_cols,
            &template_rows,
            &TrackSize::Auto,
            &TrackSize::Auto,
            0.0,
            0.0,
            GridAutoFlow::Row,
        );

        let occupied: Vec<Vec<bool>> = Vec::new();

        let (col, row) = grid.find_next_cell(1, 1, &occupied);
        assert_eq!((col, row), (0, 0));
    }

    #[test]
    fn test_grid_template_areas() {
        let areas = GridTemplateAreas::parse(
            "\"header header\"
             \"nav main\"
             \"footer footer\""
        ).unwrap();

        assert_eq!(areas.rows.len(), 3);
        
        let header = areas.get_area("header").unwrap();
        assert_eq!(header.column_start, 1);
        assert_eq!(header.column_end, 3);
        assert_eq!(header.row_start, 1);
        assert_eq!(header.row_end, 2);
    }

    #[test]
    fn test_grid_item_placement() {
        let style = ComputedStyle::new();
        let layout_box = LayoutBox::new(BoxType::Block, style);
        let mut item = GridItem::new(&layout_box);

        let placement = GridPlacement::from_lines(1, 3, 1, 2);
        item.set_placement(&placement);

        assert!(item.is_fully_placed());
        assert_eq!(item.column_start, 1);
        assert_eq!(item.column_end, 3);
        assert_eq!(item.column_span, 2);
    }

    #[test]
    fn test_grid_item_column_only_placement() {
        let style = ComputedStyle::new();
        let layout_box = LayoutBox::new(BoxType::Block, style);
        let mut item = GridItem::new(&layout_box);

        // Simulate grid-column: 1 / 3 with no row placement
        let placement = GridPlacement {
            column_start: GridLine::Number(1),
            column_end: GridLine::Number(3),
            row_start: GridLine::Auto,
            row_end: GridLine::Auto,
        };
        item.set_placement(&placement);

        assert!(!item.auto_column); // Column is explicitly placed
        assert!(item.auto_row);     // Row needs auto-placement
        assert!(item.needs_auto_placement()); // Overall needs auto-placement
        assert!(!item.is_fully_placed());     // Not fully placed
    }

    #[test]
    fn test_track_sizing_percentage() {
        // 50% track in a 400px container should be 200px
        let mut tracks = vec![
            GridTrack::new(&TrackSize::Percent(50.0)),
            GridTrack::new(&TrackSize::Fr(1.0)),
        ];

        size_grid_tracks(&mut tracks, 400.0, 0.0);

        assert_eq!(tracks[0].size, 200.0);
        // Remaining 200px goes to 1fr
        assert_eq!(tracks[1].size, 200.0);
    }

    #[test]
    fn test_track_sizing_percentage_with_gap() {
        // Two 25% tracks with 20px gap in a 400px container
        // 25% of 400 = 100px each
        let mut tracks = vec![
            GridTrack::new(&TrackSize::Percent(25.0)),
            GridTrack::new(&TrackSize::Percent(25.0)),
            GridTrack::new(&TrackSize::Fr(1.0)),
        ];

        size_grid_tracks(&mut tracks, 400.0, 20.0);

        // Percentages resolve to 25% of container
        assert_eq!(tracks[0].size, 100.0);
        assert_eq!(tracks[1].size, 100.0);
        // Available space = 400 - 40 (gaps) = 360
        // After fixed (200px), remaining = 160px for 1fr
        // But wait, percentage tracks are considered fixed
        assert_eq!(tracks[2].size, 160.0);
    }

    #[test]
    fn test_track_sizing_multiple_percentages() {
        // 30% + 20% + 1fr in 500px container
        let mut tracks = vec![
            GridTrack::new(&TrackSize::Percent(30.0)),
            GridTrack::new(&TrackSize::Percent(20.0)),
            GridTrack::new(&TrackSize::Fr(1.0)),
        ];

        size_grid_tracks(&mut tracks, 500.0, 0.0);

        assert_eq!(tracks[0].size, 150.0); // 30% of 500
        assert_eq!(tracks[1].size, 100.0); // 20% of 500
        assert_eq!(tracks[2].size, 250.0); // Remaining space
    }

    #[test]
    fn test_track_sizing_minmax_with_percentage_min() {
        // minmax(25%, 1fr) in 400px container
        // min = 25% of 400 = 100px
        // The track should get at least 100px from the fr distribution
        let mut tracks = vec![
            GridTrack::new(&TrackSize::MinMax(
                Box::new(TrackSize::Percent(25.0)),
                Box::new(TrackSize::Fr(1.0)),
            )),
            GridTrack::new(&TrackSize::Fr(1.0)),
        ];

        size_grid_tracks(&mut tracks, 400.0, 0.0);

        // Both are 1fr, but first has 100px minimum
        // With 400px available, 200px each, but first is clamped to at least 100px
        assert!(tracks[0].size >= 100.0);
        assert_eq!(tracks[0].size, 200.0); // Gets half of 400px
        assert_eq!(tracks[1].size, 200.0);
    }

    #[test]
    fn test_track_sizing_minmax_with_percentage_max() {
        // minmax(100px, 50%) in 400px container
        // min = 100px, max = 50% of 400 = 200px
        let mut tracks = vec![
            GridTrack::new(&TrackSize::MinMax(
                Box::new(TrackSize::Px(100.0)),
                Box::new(TrackSize::Percent(50.0)),
            )),
            GridTrack::new(&TrackSize::Fr(1.0)),
        ];

        size_grid_tracks(&mut tracks, 400.0, 0.0);

        // First track has min=100px, max=200px
        // It should start at 100px, and fr gets remaining 300px
        assert_eq!(tracks[0].size, 100.0); // Gets base size
        assert_eq!(tracks[1].size, 300.0); // Remaining goes to fr
    }

    #[test]
    fn test_track_min_content_flag() {
        let track = GridTrack::new(&TrackSize::MinContent);
        assert!(track.is_min_content);
        assert!(!track.is_max_content);
    }

    #[test]
    fn test_track_max_content_flag() {
        let track = GridTrack::new(&TrackSize::MaxContent);
        assert!(!track.is_min_content);
        assert!(track.is_max_content);
    }

    #[test]
    fn test_track_auto_is_intrinsic() {
        let track = GridTrack::new(&TrackSize::Auto);
        assert!(track.is_min_content); // auto behaves like minmax(min-content, max-content)
        assert!(track.is_max_content);
    }

    #[test]
    fn test_track_sizing_min_content() {
        // min-content track with contribution of 100px
        let mut track = GridTrack::new(&TrackSize::MinContent);
        track.base_size = 100.0; // Simulating item contribution

        let mut tracks = vec![track, GridTrack::new(&TrackSize::Fr(1.0))];
        size_grid_tracks(&mut tracks, 500.0, 0.0);

        // min-content track should stay at its base size
        assert_eq!(tracks[0].size, 100.0);
        // fr track gets remaining space
        assert_eq!(tracks[1].size, 400.0);
    }

    #[test]
    fn test_track_sizing_auto() {
        // auto track with contribution of 150px
        let mut track = GridTrack::new(&TrackSize::Auto);
        track.base_size = 150.0; // Simulating item contribution

        let mut tracks = vec![track, GridTrack::new(&TrackSize::Px(100.0))];
        size_grid_tracks(&mut tracks, 500.0, 0.0);

        // auto track should use its base size as minimum
        // remaining space should be distributed
        assert!(tracks[0].size >= 150.0);
        assert_eq!(tracks[1].size, 100.0);
    }

    #[test]
    fn test_track_fit_content_flag() {
        let track = GridTrack::new(&TrackSize::FitContent(200.0));
        assert!(track.is_min_content); // fit-content uses min-content as minimum
        assert!(!track.is_max_content);
        assert_eq!(track.fit_content_limit, Some(200.0));
    }

    #[test]
    fn test_track_sizing_fit_content_within_limit() {
        // fit-content(300px) with content that needs 100px
        let mut track = GridTrack::new(&TrackSize::FitContent(300.0));
        track.base_size = 100.0; // Simulating item contribution (min-content)

        let mut tracks = vec![track, GridTrack::new(&TrackSize::Fr(1.0))];
        size_grid_tracks(&mut tracks, 500.0, 0.0);

        // fit-content should clamp to min-content (100px) since that's less than limit
        assert_eq!(tracks[0].size, 100.0);
        // fr track gets remaining space
        assert_eq!(tracks[1].size, 400.0);
    }

    #[test]
    fn test_track_sizing_fit_content_at_limit() {
        // fit-content(150px) with content that would need more
        let mut track = GridTrack::new(&TrackSize::FitContent(150.0));
        track.base_size = 200.0; // Content needs 200px but we cap at 150px

        let mut tracks = vec![track, GridTrack::new(&TrackSize::Fr(1.0))];
        size_grid_tracks(&mut tracks, 500.0, 0.0);

        // fit-content should use base_size since it exceeds the limit
        // (In a real scenario, base_size would be clamped to the limit,
        // but we're simulating the case where content already exceeds)
        assert_eq!(tracks[0].size, 200.0);
        assert_eq!(tracks[1].size, 300.0);
    }

    // ==================== Phase 2: Auto-fill/Auto-fit Tests ====================

    #[test]
    fn test_auto_repeat_pattern_creation() {
        // Create a template with auto-fill
        let mut template = GridTemplate::default();
        template.repeats.push((
            0,
            TrackRepeat::AutoFill(vec![TrackDefinition::simple(TrackSize::Px(100.0))]),
        ));

        let (expanded, auto_repeat) = template.expand_tracks();

        // Expanded tracks should be empty (auto-fill not expanded yet)
        assert_eq!(expanded.len(), 0);
        // Auto-repeat should be present
        assert!(auto_repeat.is_some());
    }

    #[test]
    fn test_auto_fill_expansion_basic() {
        // repeat(auto-fill, 100px) in 500px container should create 5 tracks
        let pattern = AutoRepeatPattern {
            tracks: vec![TrackDefinition::simple(TrackSize::Px(100.0))],
            is_auto_fit: false,
            insert_position: 0,
        };

        let tracks = GridLayout::calculate_auto_repeat_tracks(&pattern, 500.0, 0.0);

        assert_eq!(tracks.len(), 5);
        for track in &tracks {
            assert_eq!(track.base_size, 100.0);
            assert!(!track.is_auto_fit);
        }
    }

    #[test]
    fn test_auto_fill_expansion_with_gap() {
        // repeat(auto-fill, 100px) in 500px container with 20px gap
        // 100 + 20 + 100 + 20 + 100 + 20 + 100 = 460, can fit 4 tracks
        let pattern = AutoRepeatPattern {
            tracks: vec![TrackDefinition::simple(TrackSize::Px(100.0))],
            is_auto_fit: false,
            insert_position: 0,
        };

        let tracks = GridLayout::calculate_auto_repeat_tracks(&pattern, 500.0, 20.0);

        assert_eq!(tracks.len(), 4);
    }

    #[test]
    fn test_auto_fill_expansion_minmax() {
        // repeat(auto-fill, minmax(100px, 1fr)) in 500px container
        // The definite size is 100px (min), so we get 5 tracks
        let pattern = AutoRepeatPattern {
            tracks: vec![TrackDefinition::simple(TrackSize::MinMax(
                Box::new(TrackSize::Px(100.0)),
                Box::new(TrackSize::Fr(1.0)),
            ))],
            is_auto_fit: false,
            insert_position: 0,
        };

        let tracks = GridLayout::calculate_auto_repeat_tracks(&pattern, 500.0, 0.0);

        assert_eq!(tracks.len(), 5);
        // Each track should be flexible
        for track in &tracks {
            assert!(track.is_flexible);
            assert_eq!(track.base_size, 100.0);
        }
    }

    #[test]
    fn test_auto_fill_expansion_multiple_tracks() {
        // repeat(auto-fill, 100px 50px) in 500px container
        // One repetition = 150px, can fit 3 repetitions = 450px
        let pattern = AutoRepeatPattern {
            tracks: vec![
                TrackDefinition::simple(TrackSize::Px(100.0)),
                TrackDefinition::simple(TrackSize::Px(50.0)),
            ],
            is_auto_fit: false,
            insert_position: 0,
        };

        let tracks = GridLayout::calculate_auto_repeat_tracks(&pattern, 500.0, 0.0);

        // 3 repetitions * 2 tracks = 6 tracks
        assert_eq!(tracks.len(), 6);
        assert_eq!(tracks[0].base_size, 100.0);
        assert_eq!(tracks[1].base_size, 50.0);
        assert_eq!(tracks[2].base_size, 100.0);
        assert_eq!(tracks[3].base_size, 50.0);
    }

    #[test]
    fn test_auto_fill_minimum_one_repetition() {
        // repeat(auto-fill, 200px) in 100px container should still create 1 track
        let pattern = AutoRepeatPattern {
            tracks: vec![TrackDefinition::simple(TrackSize::Px(200.0))],
            is_auto_fit: false,
            insert_position: 0,
        };

        let tracks = GridLayout::calculate_auto_repeat_tracks(&pattern, 100.0, 0.0);

        // At least 1 repetition per spec
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].base_size, 200.0);
    }

    #[test]
    fn test_auto_fit_flag_set() {
        // auto-fit tracks should have is_auto_fit = true
        let pattern = AutoRepeatPattern {
            tracks: vec![TrackDefinition::simple(TrackSize::Px(100.0))],
            is_auto_fit: true,
            insert_position: 0,
        };

        let tracks = GridLayout::calculate_auto_repeat_tracks(&pattern, 500.0, 0.0);

        assert_eq!(tracks.len(), 5);
        for track in &tracks {
            assert!(track.is_auto_fit);
        }
    }

    #[test]
    fn test_auto_fill_with_fr_only() {
        // repeat(auto-fill, 1fr) - no definite size, should create 1 repetition
        let pattern = AutoRepeatPattern {
            tracks: vec![TrackDefinition::simple(TrackSize::Fr(1.0))],
            is_auto_fit: false,
            insert_position: 0,
        };

        let tracks = GridLayout::calculate_auto_repeat_tracks(&pattern, 500.0, 0.0);

        // With no definite size, we get exactly 1 repetition
        assert_eq!(tracks.len(), 1);
        assert!(tracks[0].is_flexible);
    }

    #[test]
    fn test_grid_layout_expand_auto_repeats() {
        // Test that GridLayout properly expands auto-repeat patterns
        let mut template = GridTemplate::default();
        template.repeats.push((
            0,
            TrackRepeat::AutoFill(vec![TrackDefinition::simple(TrackSize::Px(100.0))]),
        ));

        let mut grid = GridLayout::new(
            &template,
            &GridTemplate::from_sizes(vec![TrackSize::Auto]),
            &TrackSize::Auto,
            &TrackSize::Auto,
            0.0,
            0.0,
            GridAutoFlow::Row,
        );

        // Before expansion, columns should be empty (auto-fill not expanded)
        assert_eq!(grid.columns.len(), 0);

        // Expand with 500px container width
        grid.expand_auto_repeats(500.0, 100.0);

        // Now should have 5 columns
        assert_eq!(grid.columns.len(), 5);
        for col in &grid.columns {
            assert_eq!(col.base_size, 100.0);
        }
    }

    #[test]
    fn test_auto_fit_collapse_empty_tracks() {
        // Test that auto-fit collapses empty tracks
        let pattern = AutoRepeatPattern {
            tracks: vec![TrackDefinition::simple(TrackSize::Px(100.0))],
            is_auto_fit: true,
            insert_position: 0,
        };

        let tracks = GridLayout::calculate_auto_repeat_tracks(&pattern, 500.0, 0.0);

        // Should have 5 tracks, all marked as auto-fit
        assert_eq!(tracks.len(), 5);
        for track in &tracks {
            assert!(track.is_auto_fit);
        }
    }

    #[test]
    fn test_auto_fit_collapse_method() {
        // Create a grid with auto-fit tracks
        let mut grid = GridLayout {
            columns: vec![
                {
                    let mut t = GridTrack::new(&TrackSize::Px(100.0));
                    t.is_auto_fit = true;
                    t
                },
                {
                    let mut t = GridTrack::new(&TrackSize::Px(100.0));
                    t.is_auto_fit = true;
                    t
                },
                {
                    let mut t = GridTrack::new(&TrackSize::Px(100.0));
                    t.is_auto_fit = true;
                    t
                },
            ],
            rows: vec![GridTrack::new(&TrackSize::Auto)],
            column_gap: 10.0,
            row_gap: 10.0,
            auto_flow: GridAutoFlow::Row,
            cursor: (0, 0),
            explicit_columns: 3,
            explicit_rows: 1,
            column_auto_repeat: None,
            row_auto_repeat: None,
        };

        // Mark only the first and third columns as occupied
        let column_occupied = vec![true, false, true];
        grid.collapse_empty_auto_fit_columns(&column_occupied);

        // First column should be intact
        assert_eq!(grid.columns[0].base_size, 100.0);
        assert_eq!(grid.columns[0].size, 100.0);

        // Second column (empty) should be collapsed
        assert_eq!(grid.columns[1].base_size, 0.0);
        assert_eq!(grid.columns[1].size, 0.0);
        assert_eq!(grid.columns[1].growth_limit, 0.0);

        // Third column should be intact
        assert_eq!(grid.columns[2].base_size, 100.0);
        assert_eq!(grid.columns[2].size, 100.0);
    }

    #[test]
    fn test_auto_fit_gap_collapsing() {
        // Test that gaps around collapsed tracks also collapse
        // Per CSS spec: "the gutters on either side of it collapse"
        let mut tracks = vec![
            {
                let mut t = GridTrack::new(&TrackSize::Px(100.0));
                t.is_auto_fit = true;
                t
            },
            {
                // This one will be collapsed
                let mut t = GridTrack::new(&TrackSize::Px(100.0));
                t.is_auto_fit = true;
                t.base_size = 0.0;
                t.size = 0.0;
                t.growth_limit = 0.0;
                t
            },
            {
                let mut t = GridTrack::new(&TrackSize::Px(100.0));
                t.is_auto_fit = true;
                t
            },
        ];

        // Size with 20px gap
        size_grid_tracks(&mut tracks, 500.0, 20.0);

        // First track at position 0
        assert_eq!(tracks[0].position, 0.0);

        // Second track (collapsed) should be at position 100 (no gap added after first track
        // because next track is collapsed)
        assert_eq!(tracks[1].position, 100.0);
        assert_eq!(tracks[1].size, 0.0);

        // Third track should be at position 100 (no gap because previous was collapsed)
        // The gutters on EITHER SIDE of a collapsed track collapse
        assert_eq!(tracks[2].position, 100.0);
    }

    #[test]
    fn test_auto_fit_all_collapsed() {
        // Test when all auto-fit tracks are collapsed
        let mut tracks = vec![
            {
                let mut t = GridTrack::new(&TrackSize::Px(100.0));
                t.is_auto_fit = true;
                t.base_size = 0.0;
                t.size = 0.0;
                t.growth_limit = 0.0;
                t
            },
            {
                let mut t = GridTrack::new(&TrackSize::Px(100.0));
                t.is_auto_fit = true;
                t.base_size = 0.0;
                t.size = 0.0;
                t.growth_limit = 0.0;
                t
            },
        ];

        size_grid_tracks(&mut tracks, 500.0, 20.0);

        // All tracks should be at position 0 with size 0
        assert_eq!(tracks[0].position, 0.0);
        assert_eq!(tracks[0].size, 0.0);
        assert_eq!(tracks[1].position, 0.0);
        assert_eq!(tracks[1].size, 0.0);
    }
}
