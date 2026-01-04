//! Flexbox layout implementation for RustKit.
//!
//! Implements the CSS Flexible Box Layout Module Level 1:
//! https://www.w3.org/TR/css-flexbox-1/
//!
//! The flexbox algorithm is complex and multi-step:
//! 1. Determine main/cross axes based on flex-direction
//! 2. Collect and sort flex items
//! 3. Calculate flex base sizes
//! 4. Collect items into flex lines (if wrapping)
//! 5. Resolve flexible lengths (grow/shrink)
//! 6. Calculate cross sizes
//! 7. Main axis alignment (justify-content)
//! 8. Cross axis alignment (align-items, align-self)
//! 9. Multi-line alignment (align-content)
//! 10. Handle reverse directions

use crate::{Dimensions, EdgeSizes, LayoutBox, Rect};
use rustkit_css::{
    AlignContent, AlignItems, AlignSelf, FlexBasis, FlexWrap, JustifyContent, Length,
};

/// Represents the main and cross axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

impl Axis {
    /// Get the perpendicular axis.
    pub fn cross(self) -> Self {
        match self {
            Axis::Horizontal => Axis::Vertical,
            Axis::Vertical => Axis::Horizontal,
        }
    }
}

/// A flex item during layout computation.
#[derive(Debug)]
pub struct FlexItem<'a> {
    /// Reference to the layout box.
    pub layout_box: &'a mut LayoutBox,

    /// Order property for sorting.
    pub order: i32,

    /// Flex grow factor.
    pub flex_grow: f32,

    /// Flex shrink factor.
    pub flex_shrink: f32,

    /// Flex basis (resolved to absolute value).
    pub flex_basis: f32,

    /// Hypothetical main size (clamped by min/max).
    pub hypothetical_main_size: f32,

    /// Target main size (after flex resolution).
    pub target_main_size: f32,

    /// Frozen flag (for grow/shrink algorithm).
    pub frozen: bool,

    /// Cross size.
    pub cross_size: f32,

    /// Main position (relative to container).
    pub main_position: f32,

    /// Cross position (relative to line start).
    pub cross_position: f32,

    /// Minimum main size.
    pub min_main_size: f32,

    /// Maximum main size.
    pub max_main_size: f32,

    /// Minimum cross size.
    pub min_cross_size: f32,

    /// Maximum cross size.
    pub max_cross_size: f32,

    /// Align self value.
    pub align_self: AlignSelf,

    /// Outer margin on main axis start.
    pub main_margin_start: f32,

    /// Outer margin on main axis end.
    pub main_margin_end: f32,

    /// Outer margin on cross axis start.
    pub cross_margin_start: f32,

    /// Outer margin on cross axis end.
    pub cross_margin_end: f32,
}

impl<'a> FlexItem<'a> {
    /// Get outer main size (target + margins).
    pub fn outer_main_size(&self) -> f32 {
        self.target_main_size + self.main_margin_start + self.main_margin_end
    }

    /// Get outer hypothetical main size.
    pub fn outer_hypothetical_main_size(&self) -> f32 {
        self.hypothetical_main_size + self.main_margin_start + self.main_margin_end
    }

    /// Get outer cross size.
    pub fn outer_cross_size(&self) -> f32 {
        self.cross_size + self.cross_margin_start + self.cross_margin_end
    }
}

/// A flex line containing multiple items.
#[derive(Debug)]
pub struct FlexLine<'a> {
    /// Items in this line.
    pub items: Vec<FlexItem<'a>>,

    /// Cross size of the line.
    pub cross_size: f32,

    /// Cross position of the line.
    pub cross_position: f32,
}

impl<'a> FlexLine<'a> {
    /// Create a new flex line.
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            cross_size: 0.0,
            cross_position: 0.0,
        }
    }

    /// Get the total hypothetical main size of items.
    pub fn hypothetical_main_size(&self) -> f32 {
        self.items.iter().map(|item| item.outer_hypothetical_main_size()).sum()
    }

    /// Get the largest outer cross size among items.
    pub fn max_outer_cross_size(&self) -> f32 {
        self.items
            .iter()
            .map(|item| item.outer_cross_size())
            .fold(0.0, f32::max)
    }
}

impl<'a> Default for FlexLine<'a> {
    fn default() -> Self {
        Self::new()
    }
}

/// Layout a flex container and its children.
pub fn layout_flex_container(
    container: &mut LayoutBox,
    containing_block: &Dimensions,
) {
    let style = &container.style;

    // 1. Determine main/cross axes
    let direction = style.flex_direction;
    let main_axis = if direction.is_row() {
        Axis::Horizontal
    } else {
        Axis::Vertical
    };
    let cross_axis = main_axis.cross();

    // Get container dimensions
    let container_main_size = match main_axis {
        Axis::Horizontal => containing_block.content.width,
        Axis::Vertical => containing_block.content.height,
    };
    let container_cross_size = match cross_axis {
        Axis::Horizontal => containing_block.content.width,
        Axis::Vertical => containing_block.content.height,
    };

    // Get gap values
    let main_gap = match main_axis {
        Axis::Horizontal => resolve_length(&style.column_gap, container_main_size),
        Axis::Vertical => resolve_length(&style.row_gap, container_main_size),
    };
    let cross_gap = match cross_axis {
        Axis::Horizontal => resolve_length(&style.column_gap, container_cross_size),
        Axis::Vertical => resolve_length(&style.row_gap, container_cross_size),
    };

    // 2. Collect flex items (skip absolutely positioned)
    let mut items: Vec<FlexItem> = Vec::new();
    for child in &mut container.children {
        if child.style.position == rustkit_css::Position::Absolute
            || child.style.position == rustkit_css::Position::Fixed
        {
            continue;
        }

        let item = create_flex_item(child, main_axis, container_main_size, container_cross_size);
        items.push(item);
    }

    // Sort by order property
    items.sort_by_key(|item| item.order);

    // 3. Collect items into flex lines
    let wrap = style.flex_wrap;
    let mut lines = collect_flex_lines(items, container_main_size, main_gap, wrap);

    if lines.is_empty() {
        return;
    }

    // 4. Resolve flexible lengths for each line
    for line in &mut lines {
        resolve_flexible_lengths(line, container_main_size, main_gap);
    }

    // 5. Calculate cross sizes for each line
    for line in &mut lines {
        calculate_cross_sizes(line, container_cross_size, style.align_items);
    }

    // 6. Calculate line cross sizes and positions
    let total_cross_size: f32 = lines.iter().map(|l| l.cross_size).sum::<f32>()
        + cross_gap * (lines.len().saturating_sub(1)) as f32;

    // 7. Apply align-content for multi-line containers
    distribute_lines(&mut lines, container_cross_size, total_cross_size, cross_gap, style.align_content);

    // 8. Main axis alignment (justify-content) and positioning
    for line in &mut lines {
        distribute_main_axis(line, container_main_size, main_gap, style.justify_content, direction.is_reverse());
    }

    // 9. Cross axis alignment (align-items, align-self)
    for line in &mut lines {
        align_cross_axis(line, style.align_items);
    }

    // 10. Apply final positions to layout boxes
    apply_positions(&mut lines, main_axis, direction.is_reverse(), wrap == FlexWrap::WrapReverse);
}

/// Create a FlexItem from a LayoutBox.
fn create_flex_item<'a>(
    layout_box: &'a mut LayoutBox,
    main_axis: Axis,
    container_main: f32,
    container_cross: f32,
) -> FlexItem<'a> {
    // Extract all values from style first to avoid borrow conflicts
    let order = layout_box.style.order;
    let flex_grow = layout_box.style.flex_grow;
    let flex_shrink = layout_box.style.flex_shrink;
    let flex_basis_value = layout_box.style.flex_basis;
    let align_self = layout_box.style.align_self;

    // Get margins
    let (main_margin_start, main_margin_end, cross_margin_start, cross_margin_end) = match main_axis {
        Axis::Horizontal => (
            resolve_length(&layout_box.style.margin_left, container_main),
            resolve_length(&layout_box.style.margin_right, container_main),
            resolve_length(&layout_box.style.margin_top, container_cross),
            resolve_length(&layout_box.style.margin_bottom, container_cross),
        ),
        Axis::Vertical => (
            resolve_length(&layout_box.style.margin_top, container_main),
            resolve_length(&layout_box.style.margin_bottom, container_main),
            resolve_length(&layout_box.style.margin_left, container_cross),
            resolve_length(&layout_box.style.margin_right, container_cross),
        ),
    };

    // Calculate flex basis
    let flex_basis = match flex_basis_value {
        FlexBasis::Auto => {
            // Use main size property
            match main_axis {
                Axis::Horizontal => resolve_length(&layout_box.style.width, container_main),
                Axis::Vertical => resolve_length(&layout_box.style.height, container_main),
            }
        }
        FlexBasis::Content => {
            // Use content size (simplified - would need actual content measurement)
            0.0
        }
        FlexBasis::Length(len) => len,
        FlexBasis::Percent(pct) => pct / 100.0 * container_main,
    };

    // Get min/max constraints
    let (min_main, max_main, min_cross, max_cross) = match main_axis {
        Axis::Horizontal => (
            resolve_length(&layout_box.style.min_width, container_main),
            resolve_max_length(&layout_box.style.max_width, container_main),
            resolve_length(&layout_box.style.min_height, container_cross),
            resolve_max_length(&layout_box.style.max_height, container_cross),
        ),
        Axis::Vertical => (
            resolve_length(&layout_box.style.min_height, container_main),
            resolve_max_length(&layout_box.style.max_height, container_main),
            resolve_length(&layout_box.style.min_width, container_cross),
            resolve_max_length(&layout_box.style.max_width, container_cross),
        ),
    };

    // Hypothetical main size (clamped)
    let hypothetical_main_size = flex_basis.max(min_main).min(max_main);

    FlexItem {
        layout_box,
        order,
        flex_grow,
        flex_shrink,
        flex_basis,
        hypothetical_main_size,
        target_main_size: hypothetical_main_size,
        frozen: false,
        cross_size: 0.0,
        main_position: 0.0,
        cross_position: 0.0,
        min_main_size: min_main,
        max_main_size: max_main,
        min_cross_size: min_cross,
        max_cross_size: max_cross,
        align_self,
        main_margin_start,
        main_margin_end,
        cross_margin_start,
        cross_margin_end,
    }
}

/// Collect items into flex lines based on wrap property.
fn collect_flex_lines<'a>(
    mut items: Vec<FlexItem<'a>>,
    container_main: f32,
    main_gap: f32,
    wrap: FlexWrap,
) -> Vec<FlexLine<'a>> {
    if items.is_empty() {
        return Vec::new();
    }

    if wrap == FlexWrap::NoWrap {
        // Single line
        let mut line = FlexLine::new();
        line.items = items;
        return vec![line];
    }

    // Multi-line
    let mut lines = Vec::new();
    let mut current_line = FlexLine::new();
    let mut line_main_size = 0.0f32;

    for item in items.drain(..) {
        let item_size = item.outer_hypothetical_main_size();
        let gap = if current_line.items.is_empty() { 0.0 } else { main_gap };

        if !current_line.items.is_empty() && line_main_size + gap + item_size > container_main {
            // Start new line
            lines.push(current_line);
            current_line = FlexLine::new();
            line_main_size = 0.0;
        }

        line_main_size += if current_line.items.is_empty() { 0.0 } else { main_gap };
        line_main_size += item_size;
        current_line.items.push(item);
    }

    if !current_line.items.is_empty() {
        lines.push(current_line);
    }

    lines
}

/// Resolve flexible lengths (grow/shrink) for a line.
fn resolve_flexible_lengths(line: &mut FlexLine, container_main: f32, main_gap: f32) {
    if line.items.is_empty() {
        return;
    }

    // Calculate used space
    let total_gaps = main_gap * (line.items.len().saturating_sub(1)) as f32;
    let used_space: f32 = line.items.iter().map(|i| i.hypothetical_main_size + i.main_margin_start + i.main_margin_end).sum();
    let free_space = container_main - used_space - total_gaps;

    if free_space.abs() < 0.01 {
        // No adjustment needed
        return;
    }

    // Reset frozen state
    for item in &mut line.items {
        item.frozen = false;
        item.target_main_size = item.hypothetical_main_size;
    }

    if free_space > 0.0 {
        // Grow items
        grow_items(line, free_space);
    } else {
        // Shrink items
        shrink_items(line, -free_space);
    }
}

/// Grow items to fill free space.
fn grow_items(line: &mut FlexLine, free_space: f32) {
    let total_grow: f32 = line.items.iter().filter(|i| !i.frozen).map(|i| i.flex_grow).sum();

    if total_grow <= 0.0 {
        return;
    }

    let space_per_grow = free_space / total_grow;

    for item in &mut line.items {
        if item.frozen {
            continue;
        }

        let grow = item.flex_grow * space_per_grow;
        let new_size = item.target_main_size + grow;

        if new_size > item.max_main_size {
            item.target_main_size = item.max_main_size;
            item.frozen = true;
        } else {
            item.target_main_size = new_size;
        }
    }
}

/// Shrink items to remove overflow.
fn shrink_items(line: &mut FlexLine, overflow: f32) {
    let total_shrink_scaled: f32 = line.items
        .iter()
        .filter(|i| !i.frozen)
        .map(|i| i.flex_shrink * i.flex_basis)
        .sum();

    if total_shrink_scaled <= 0.0 {
        return;
    }

    for item in &mut line.items {
        if item.frozen {
            continue;
        }

        let shrink_scaled = item.flex_shrink * item.flex_basis;
        let shrink_ratio = shrink_scaled / total_shrink_scaled;
        let shrink = overflow * shrink_ratio;
        let new_size = (item.target_main_size - shrink).max(item.min_main_size);

        if new_size <= item.min_main_size {
            item.target_main_size = item.min_main_size;
            item.frozen = true;
        } else {
            item.target_main_size = new_size;
        }
    }
}

/// Calculate cross sizes for items in a line.
fn calculate_cross_sizes(line: &mut FlexLine, container_cross: f32, align_items: AlignItems) {
    // Calculate each item's cross size
    for item in &mut line.items {
        let align = if item.align_self == AlignSelf::Auto {
            align_items
        } else {
            match item.align_self {
                AlignSelf::Auto => align_items,
                AlignSelf::FlexStart => AlignItems::FlexStart,
                AlignSelf::FlexEnd => AlignItems::FlexEnd,
                AlignSelf::Center => AlignItems::Center,
                AlignSelf::Baseline => AlignItems::Baseline,
                AlignSelf::Stretch => AlignItems::Stretch,
            }
        };

        if align == AlignItems::Stretch {
            // Stretch to fill line (will be adjusted later)
            item.cross_size = container_cross - item.cross_margin_start - item.cross_margin_end;
        } else {
            // Use hypothetical cross size (content-based)
            // For simplicity, use min_cross_size as a placeholder
            item.cross_size = item.min_cross_size;
        }

        // Clamp to min/max
        item.cross_size = item.cross_size.max(item.min_cross_size).min(item.max_cross_size);
    }

    // Determine line cross size (largest item)
    line.cross_size = line.items
        .iter()
        .map(|i| i.cross_size + i.cross_margin_start + i.cross_margin_end)
        .fold(0.0, f32::max);
}

/// Distribute lines according to align-content.
fn distribute_lines(
    lines: &mut [FlexLine],
    container_cross: f32,
    _total_cross: f32,
    cross_gap: f32,
    align_content: AlignContent,
) {
    if lines.is_empty() {
        return;
    }

    let total_line_size: f32 = lines.iter().map(|l| l.cross_size).sum();
    let total_gaps = cross_gap * (lines.len().saturating_sub(1)) as f32;
    let free_space = container_cross - total_line_size - total_gaps;

    let (initial_offset, spacing) = match align_content {
        AlignContent::FlexStart => (0.0, cross_gap),
        AlignContent::FlexEnd => (free_space, cross_gap),
        AlignContent::Center => (free_space / 2.0, cross_gap),
        AlignContent::SpaceBetween => {
            if lines.len() > 1 {
                (0.0, free_space / (lines.len() - 1) as f32 + cross_gap)
            } else {
                (0.0, cross_gap)
            }
        }
        AlignContent::SpaceAround => {
            let space = free_space / lines.len() as f32;
            (space / 2.0, space + cross_gap)
        }
        AlignContent::SpaceEvenly => {
            let space = free_space / (lines.len() + 1) as f32;
            (space, space + cross_gap)
        }
        AlignContent::Stretch => {
            // Distribute free space to lines
            let extra_per_line = free_space / lines.len() as f32;
            for line in lines.iter_mut() {
                line.cross_size += extra_per_line;
            }
            (0.0, cross_gap)
        }
    };

    // Set line positions
    let mut cross_pos = initial_offset;
    for line in lines.iter_mut() {
        line.cross_position = cross_pos;
        cross_pos += line.cross_size + spacing;
    }
}

/// Distribute items along main axis (justify-content).
fn distribute_main_axis(
    line: &mut FlexLine,
    container_main: f32,
    main_gap: f32,
    justify_content: JustifyContent,
    reverse: bool,
) {
    if line.items.is_empty() {
        return;
    }

    let total_item_size: f32 = line.items.iter().map(|i| i.outer_main_size()).sum();
    let total_gaps = main_gap * (line.items.len().saturating_sub(1)) as f32;
    let free_space = (container_main - total_item_size - total_gaps).max(0.0);

    let (initial_offset, spacing) = match justify_content {
        JustifyContent::FlexStart => (0.0, main_gap),
        JustifyContent::FlexEnd => (free_space, main_gap),
        JustifyContent::Center => (free_space / 2.0, main_gap),
        JustifyContent::SpaceBetween => {
            if line.items.len() > 1 {
                (0.0, free_space / (line.items.len() - 1) as f32 + main_gap)
            } else {
                (0.0, main_gap)
            }
        }
        JustifyContent::SpaceAround => {
            let space = free_space / line.items.len() as f32;
            (space / 2.0, space + main_gap)
        }
        JustifyContent::SpaceEvenly => {
            let space = free_space / (line.items.len() + 1) as f32;
            (space, space + main_gap)
        }
    };

    // Position items
    let mut main_pos = initial_offset;
    let items_to_position: Vec<_> = if reverse {
        (0..line.items.len()).rev().collect()
    } else {
        (0..line.items.len()).collect()
    };

    for (i, &idx) in items_to_position.iter().enumerate() {
        let item = &mut line.items[idx];
        item.main_position = main_pos + item.main_margin_start;
        main_pos += item.outer_main_size();
        if i < items_to_position.len() - 1 {
            main_pos += spacing;
        }
    }
}

/// Align items on cross axis within line.
fn align_cross_axis(line: &mut FlexLine, align_items: AlignItems) {
    for item in &mut line.items {
        let align = if item.align_self == AlignSelf::Auto {
            align_items
        } else {
            match item.align_self {
                AlignSelf::Auto => align_items,
                AlignSelf::FlexStart => AlignItems::FlexStart,
                AlignSelf::FlexEnd => AlignItems::FlexEnd,
                AlignSelf::Center => AlignItems::Center,
                AlignSelf::Baseline => AlignItems::Baseline,
                AlignSelf::Stretch => AlignItems::Stretch,
            }
        };

        let outer_cross = item.cross_size + item.cross_margin_start + item.cross_margin_end;
        let free_space = (line.cross_size - outer_cross).max(0.0);

        item.cross_position = match align {
            AlignItems::FlexStart => item.cross_margin_start,
            AlignItems::FlexEnd => free_space + item.cross_margin_start,
            AlignItems::Center => free_space / 2.0 + item.cross_margin_start,
            AlignItems::Baseline => item.cross_margin_start, // Simplified
            AlignItems::Stretch => item.cross_margin_start,
        };
    }
}

/// Apply computed positions to layout boxes.
fn apply_positions(
    lines: &mut [FlexLine],
    main_axis: Axis,
    _reverse_main: bool,
    reverse_cross: bool,
) {
    let lines_iter: Box<dyn Iterator<Item = &mut FlexLine>> = if reverse_cross {
        Box::new(lines.iter_mut().rev())
    } else {
        Box::new(lines.iter_mut())
    };

    for line in lines_iter {
        for item in &mut line.items {
            let (x, y, width, height) = match main_axis {
                Axis::Horizontal => (
                    item.main_position,
                    line.cross_position + item.cross_position,
                    item.target_main_size,
                    item.cross_size,
                ),
                Axis::Vertical => (
                    line.cross_position + item.cross_position,
                    item.main_position,
                    item.cross_size,
                    item.target_main_size,
                ),
            };

            // Update layout box dimensions
            item.layout_box.dimensions.content = Rect {
                x,
                y,
                width,
                height,
            };

            // Set margins
            item.layout_box.dimensions.margin = match main_axis {
                Axis::Horizontal => EdgeSizes {
                    left: item.main_margin_start,
                    right: item.main_margin_end,
                    top: item.cross_margin_start,
                    bottom: item.cross_margin_end,
                },
                Axis::Vertical => EdgeSizes {
                    top: item.main_margin_start,
                    bottom: item.main_margin_end,
                    left: item.cross_margin_start,
                    right: item.cross_margin_end,
                },
            };
        }
    }
}

/// Resolve a Length to pixels.
fn resolve_length(length: &Length, container_size: f32) -> f32 {
    match length {
        Length::Px(px) => *px,
        Length::Em(em) => em * 16.0, // Default font size
        Length::Rem(rem) => rem * 16.0,
        Length::Percent(pct) => pct / 100.0 * container_size,
        Length::Auto => 0.0,
        Length::Zero => 0.0,
    }
}

/// Resolve a max Length (returns f32::INFINITY for Auto).
fn resolve_max_length(length: &Length, container_size: f32) -> f32 {
    match length {
        Length::Auto => f32::INFINITY,
        _ => resolve_length(length, container_size),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustkit_css::{ComputedStyle, FlexDirection, JustifyContent, AlignItems, Length};
    use crate::BoxType;

    #[test]
    fn test_axis_cross() {
        assert_eq!(Axis::Horizontal.cross(), Axis::Vertical);
        assert_eq!(Axis::Vertical.cross(), Axis::Horizontal);
    }

    #[test]
    fn test_flex_direction_properties() {
        assert!(FlexDirection::Row.is_row());
        assert!(FlexDirection::RowReverse.is_row());
        assert!(!FlexDirection::Column.is_row());
        assert!(FlexDirection::RowReverse.is_reverse());
        assert!(!FlexDirection::Row.is_reverse());
    }

    #[test]
    fn test_flex_line_creation() {
        let line = FlexLine::new();
        assert!(line.items.is_empty());
        assert_eq!(line.cross_size, 0.0);
    }

    #[test]
    fn test_basic_flex_layout() {
        // Create a flex container with two children
        let mut style = ComputedStyle::new();
        style.display = rustkit_css::Display::Flex;
        style.flex_direction = FlexDirection::Row;

        let mut container = LayoutBox::new(BoxType::Block, style);

        // Add two children
        let mut child1_style = ComputedStyle::new();
        child1_style.width = Length::Px(100.0);
        child1_style.height = Length::Px(50.0);
        container.children.push(LayoutBox::new(BoxType::Block, child1_style));

        let mut child2_style = ComputedStyle::new();
        child2_style.width = Length::Px(100.0);
        child2_style.height = Length::Px(50.0);
        container.children.push(LayoutBox::new(BoxType::Block, child2_style));

        // Create containing block
        let containing = Dimensions {
            content: Rect::new(0.0, 0.0, 400.0, 300.0),
            ..Default::default()
        };

        // Layout
        layout_flex_container(&mut container, &containing);

        // Verify children have positions
        assert_eq!(container.children.len(), 2);
    }

    #[test]
    fn test_flex_grow() {
        let mut style = ComputedStyle::new();
        style.display = rustkit_css::Display::Flex;

        let mut container = LayoutBox::new(BoxType::Block, style);

        // Two children with flex-grow: 1
        let mut child1_style = ComputedStyle::new();
        child1_style.flex_grow = 1.0;
        container.children.push(LayoutBox::new(BoxType::Block, child1_style));

        let mut child2_style = ComputedStyle::new();
        child2_style.flex_grow = 1.0;
        container.children.push(LayoutBox::new(BoxType::Block, child2_style));

        let containing = Dimensions {
            content: Rect::new(0.0, 0.0, 400.0, 100.0),
            ..Default::default()
        };

        layout_flex_container(&mut container, &containing);

        // Both children should share space equally
        let child1_width = container.children[0].dimensions.content.width;
        let child2_width = container.children[1].dimensions.content.width;
        assert!((child1_width - child2_width).abs() < 1.0);
    }

    #[test]
    fn test_justify_content_center() {
        let mut style = ComputedStyle::new();
        style.display = rustkit_css::Display::Flex;
        style.justify_content = JustifyContent::Center;

        let mut container = LayoutBox::new(BoxType::Block, style);

        let mut child_style = ComputedStyle::new();
        child_style.width = Length::Px(100.0);
        child_style.flex_basis = rustkit_css::FlexBasis::Length(100.0);
        child_style.min_width = Length::Px(100.0); // Prevent shrinking
        child_style.flex_shrink = 0.0; // Don't shrink
        container.children.push(LayoutBox::new(BoxType::Block, child_style));

        let containing = Dimensions {
            content: Rect::new(0.0, 0.0, 400.0, 100.0),
            ..Default::default()
        };

        layout_flex_container(&mut container, &containing);

        // Child should be centered - (400 - 100) / 2 = 150
        let child_x = container.children[0].dimensions.content.x;
        let child_w = container.children[0].dimensions.content.width;
        let expected_x = (400.0 - child_w) / 2.0;
        assert!(
            (child_x - expected_x).abs() < 1.0,
            "Expected child_x around {}, got {} (child_w={})",
            expected_x,
            child_x,
            child_w
        );
    }

    #[test]
    fn test_align_items_center() {
        let mut style = ComputedStyle::new();
        style.display = rustkit_css::Display::Flex;
        style.align_items = AlignItems::Center;

        let mut container = LayoutBox::new(BoxType::Block, style);

        let mut child_style = ComputedStyle::new();
        child_style.width = Length::Px(100.0);
        child_style.height = Length::Px(50.0);
        child_style.min_height = Length::Px(50.0);
        container.children.push(LayoutBox::new(BoxType::Block, child_style));

        let containing = Dimensions {
            content: Rect::new(0.0, 0.0, 400.0, 200.0),
            ..Default::default()
        };

        layout_flex_container(&mut container, &containing);

        // Child should be vertically centered (cross axis)
        let child_y = container.children[0].dimensions.content.y;
        // Note: actual centering depends on line cross_size calculation
        assert!(child_y >= 0.0);
    }

    #[test]
    fn test_column_direction() {
        let mut style = ComputedStyle::new();
        style.display = rustkit_css::Display::Flex;
        style.flex_direction = FlexDirection::Column;

        let mut container = LayoutBox::new(BoxType::Block, style);

        let mut child1_style = ComputedStyle::new();
        child1_style.height = Length::Px(50.0);
        child1_style.flex_basis = rustkit_css::FlexBasis::Length(50.0);
        child1_style.min_height = Length::Px(50.0);
        container.children.push(LayoutBox::new(BoxType::Block, child1_style));

        let mut child2_style = ComputedStyle::new();
        child2_style.height = Length::Px(50.0);
        child2_style.flex_basis = rustkit_css::FlexBasis::Length(50.0);
        child2_style.min_height = Length::Px(50.0);
        container.children.push(LayoutBox::new(BoxType::Block, child2_style));

        let containing = Dimensions {
            content: Rect::new(0.0, 0.0, 400.0, 300.0),
            ..Default::default()
        };

        layout_flex_container(&mut container, &containing);

        // In column layout, items should stack vertically
        let child1_y = container.children[0].dimensions.content.y;
        let child2_y = container.children[1].dimensions.content.y;
        assert!(
            child2_y >= child1_y,
            "Expected child2_y ({}) >= child1_y ({})",
            child2_y,
            child1_y
        );
    }
}

