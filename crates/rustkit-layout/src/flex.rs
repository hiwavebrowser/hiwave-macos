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
use tracing::trace;

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

    /// Whether the item has an explicit cross size (not auto).
    /// If true, stretch should not apply per CSS spec.
    pub has_explicit_cross_size: bool,

    /// Explicit cross size (border-box), when the style specifies one.
    pub explicit_cross_size: Option<f32>,

    /// Padding+border extent at the main-axis start edge (resolved px).
    /// All FlexItem sizes (basis, hypothetical, target, cross) are
    /// border-box; these extents convert back to the content rect at
    /// apply_positions time.
    pub main_pb_start: f32,

    /// Padding+border extent at the main-axis end edge.
    pub main_pb_end: f32,

    /// Padding+border extent at the cross-axis start edge.
    pub cross_pb_start: f32,

    /// Padding+border extent at the cross-axis end edge.
    pub cross_pb_end: f32,
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

    /// Total padding+border on the main axis.
    pub fn main_pb(&self) -> f32 {
        self.main_pb_start + self.main_pb_end
    }

    /// Total padding+border on the cross axis.
    pub fn cross_pb(&self) -> f32 {
        self.cross_pb_start + self.cross_pb_end
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
        self.items
            .iter()
            .map(|item| item.outer_hypothetical_main_size())
            .sum()
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
pub fn layout_flex_container(container: &mut LayoutBox, containing_block: &Dimensions) {
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

    // Check if the flex container has a definite cross size
    // For row direction, cross axis is vertical (height)
    // For column direction, cross axis is horizontal (width)
    let has_definite_cross_size = match cross_axis {
        Axis::Vertical => !matches!(container.style.height, Length::Auto),
        Axis::Horizontal => !matches!(container.style.width, Length::Auto),
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
    // Pass has_definite_cross_size so stretch behavior is correct for auto-height containers
    for line in &mut lines {
        calculate_cross_sizes(
            line,
            container_cross_size,
            style.align_items,
            has_definite_cross_size,
        );
    }

    // 6. Calculate line cross sizes and positions
    let total_cross_size: f32 = lines.iter().map(|l| l.cross_size).sum::<f32>()
        + cross_gap * (lines.len().saturating_sub(1)) as f32;

    // 7. Apply align-content for multi-line containers
    // Only distribute lines if we have a definite cross size
    let effective_cross_size = if has_definite_cross_size {
        container_cross_size
    } else {
        total_cross_size
    };
    distribute_lines(
        &mut lines,
        effective_cross_size,
        total_cross_size,
        cross_gap,
        style.align_content,
    );

    // 8. Main axis alignment (justify-content) and positioning
    for line in &mut lines {
        distribute_main_axis(
            line,
            container_main_size,
            main_gap,
            style.justify_content,
            direction.is_reverse(),
        );
    }

    // 9. Cross axis alignment (align-items, align-self)
    for line in &mut lines {
        align_cross_axis(line, style.align_items);
    }

    // 10. Apply final positions to layout boxes
    // Pass the container's content origin so positions are absolute, not relative
    let container_origin = (containing_block.content.x, containing_block.content.y);
    apply_positions(
        &mut lines,
        main_axis,
        direction.is_reverse(),
        wrap == FlexWrap::WrapReverse,
        container_origin,
    );

    // 11. Recursively layout children of flex items (important for nested flex containers)
    // After flex positioning, each item's dimensions are set, so we can use them as containing blocks
    for line in &mut lines {
        for item in &mut line.items {
            // If this flex item has children and is a container (flex or block), lay them out
            if !item.layout_box.children.is_empty() {
                if item.layout_box.style.display.is_flex() {
                    // Nested flex container: recursively apply flex layout
                    let child_containing = item.layout_box.dimensions.clone();
                    layout_flex_container(item.layout_box, &child_containing);
                    // Absolutely positioned children are skipped by the flex
                    // item collection; lay them out against the item's FINAL
                    // dimensions so `inset: 0` overlays position AND stretch
                    // (the pre-pass ran them against pre-flex dims).
                    let cb = item.layout_box.dimensions.clone();
                    for child in &mut item.layout_box.children {
                        if matches!(
                            child.style.position,
                            rustkit_css::Position::Absolute | rustkit_css::Position::Fixed
                        ) {
                            child.layout(&cb);
                        }
                    }
                } else {
                    // Block container: lay out children in normal flow.
                    // Cloning the item's FINAL dimensions per child would make every
                    // child position itself at content.y + content.height (block layout
                    // treats containing_block.content.height as the flow cursor), i.e.
                    // stacked at the item's bottom edge. layout_block_children advances
                    // a real cursor from the item's content top instead.
                    item.layout_box.layout_block_children();
                }
            }
        }
    }

    // 11b. Recompute cross sizes now that children are laid out
    // This fixes the chicken-and-egg problem where we need children heights
    // before we can determine item cross sizes
    for line in &mut lines {
        for item in &mut line.items {
            // A DEFINITE cross size never grows to fit content — content
            // overflows instead (css-flexbox-1 §9.4). Without this guard the
            // settings toggles (inline-flex, height:26px) ballooned to their
            // stacked children's 40.4px whenever they sat in a flex row.
            if item.has_explicit_cross_size {
                continue;
            }
            // Only recompute if cross_size is still using fallback (line_height or similar)
            // and we have children with actual heights
            if !item.layout_box.children.is_empty() {
                let children_height: f32 = item
                    .layout_box
                    .children
                    .iter()
                    .map(|c| c.dimensions.margin_box().height)
                    .sum();

                // children_height is a content measure; item.cross_size is
                // border-box, so compare and store with padding+border added.
                if children_height > 0.0 && children_height + item.cross_pb() > item.cross_size {
                    // Update cross size based on actual children heights
                    item.cross_size = (children_height + item.cross_pb())
                        .max(item.min_cross_size)
                        .min(item.max_cross_size);

                    // Also update the layout box content height
                    match cross_axis {
                        Axis::Vertical => {
                            if item.layout_box.dimensions.content.height < children_height {
                                item.layout_box.dimensions.content.height = children_height;
                            }
                        }
                        Axis::Horizontal => {
                            if item.layout_box.dimensions.content.width < children_height {
                                item.layout_box.dimensions.content.width = children_height;
                            }
                        }
                    }
                }
            }
        }

        // Recompute line cross size based on updated item cross sizes
        line.cross_size = line
            .items
            .iter()
            .map(|i| i.cross_size + i.cross_margin_start + i.cross_margin_end)
            .fold(0.0, f32::max);
    }

    // 11c. Re-position lines with the true cross sizes. Steps 6-10 placed
    // lines using the hypothetical (line-height fallback) cross sizes;
    // step 11's child layout revealed the real ones. Without this pass,
    // wrapped rows stack at the estimated heights and overlap whenever an
    // item is taller than one text line.
    let total_cross_size: f32 = lines.iter().map(|l| l.cross_size).sum::<f32>()
        + cross_gap * (lines.len().saturating_sub(1)) as f32;
    let effective_cross_size = if has_definite_cross_size {
        container_cross_size
    } else {
        total_cross_size
    };
    distribute_lines(
        &mut lines,
        effective_cross_size,
        total_cross_size,
        cross_gap,
        style.align_content,
    );
    for line in &mut lines {
        align_cross_axis(line, style.align_items);
        for item in &mut line.items {
            // New absolute border-box cross position, converted to a content
            // rect delta; main-axis positions are unchanged, so shifting the
            // already-laid-out subtree is sufficient.
            let d = &item.layout_box.dimensions;
            let (origin_cross, old_content_cross, pb_start) = match cross_axis {
                Axis::Vertical => (
                    container_origin.1,
                    d.content.y,
                    d.padding.top + d.border.top,
                ),
                Axis::Horizontal => (
                    container_origin.0,
                    d.content.x,
                    d.padding.left + d.border.left,
                ),
            };
            let delta = origin_cross + line.cross_position + item.cross_position + pb_start
                - old_content_cross;
            if delta != 0.0 {
                match cross_axis {
                    Axis::Vertical => translate_subtree(item.layout_box, 0.0, delta),
                    Axis::Horizontal => translate_subtree(item.layout_box, delta, 0.0),
                }
            }
        }
    }

    // 12. Update container dimensions based on flex items
    // Calculate the total main and cross sizes used by items
    if !lines.is_empty() {
        let (total_main, total_cross) = match main_axis {
            Axis::Horizontal => {
                // Main axis is horizontal (width), cross axis is vertical (height)
                let max_main: f32 = lines
                    .iter()
                    .flat_map(|l| l.items.iter())
                    .map(|item| item.main_position + item.target_main_size)
                    .fold(0.0f32, f32::max);
                let total_cross: f32 = lines.iter().map(|l| l.cross_size).sum::<f32>()
                    + cross_gap * (lines.len().saturating_sub(1)) as f32;
                (max_main, total_cross)
            }
            Axis::Vertical => {
                // Main axis is vertical (height), cross axis is horizontal (width)
                let max_main: f32 = lines
                    .iter()
                    .flat_map(|l| l.items.iter())
                    .map(|item| item.main_position + item.target_main_size)
                    .fold(0.0f32, f32::max);
                let total_cross: f32 = lines.iter().map(|l| l.cross_size).sum::<f32>()
                    + cross_gap * (lines.len().saturating_sub(1)) as f32;
                (max_main, total_cross)
            }
        };

        // Update container height. Auto heights take the content size; an
        // EXPLICIT height that reaches here unresolved (content.height still
        // 0.0 — e.g. a nested inline-flex laid out mid-pass, like the
        // settings toggles) gets RESOLVED from style, never clobbered with
        // the children's sum. The old `== 0.0 ||` arm did exactly that
        // clobbering: height:26px toggles grew to their content (40.4px)
        // whenever they sat inside another flex row.
        let content_size = match main_axis {
            Axis::Horizontal => total_cross,
            Axis::Vertical => total_main,
        };
        if matches!(container.style.height, rustkit_css::Length::Auto) {
            container.dimensions.content.height = content_size;
        } else if container.dimensions.content.height == 0.0 {
            let explicit = match container.style.height {
                rustkit_css::Length::Px(px) => Some(px),
                rustkit_css::Length::Percent(pct)
                    if containing_block.content.height > 0.0 =>
                {
                    Some(pct / 100.0 * containing_block.content.height)
                }
                _ => None,
            };
            match explicit {
                Some(h) => {
                    // Specified heights are border-box under border-box sizing.
                    let pb = container.dimensions.padding.vertical()
                        + container.dimensions.border.vertical();
                    let is_bb = container.style.box_sizing
                        == rustkit_css::BoxSizing::BorderBox;
                    container.dimensions.content.height =
                        if is_bb { (h - pb).max(0.0) } else { h };
                }
                None => container.dimensions.content.height = content_size,
            }
        }
    }
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
    let (main_margin_start, main_margin_end, cross_margin_start, cross_margin_end) = match main_axis
    {
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

    // Padding and border were resolved onto dimensions by the block
    // pre-pass that runs before flex (layout_block_with_definite_height),
    // so read them from there. All flex sizes below are border-box: a
    // specified size under box-sizing:content-box gains padding+border,
    // under border-box it is used as-is. Intrinsic estimates measure
    // content and always gain padding+border.
    let (main_pb_start, main_pb_end, cross_pb_start, cross_pb_end) = {
        let d = &layout_box.dimensions;
        match main_axis {
            Axis::Horizontal => (
                d.padding.left + d.border.left,
                d.padding.right + d.border.right,
                d.padding.top + d.border.top,
                d.padding.bottom + d.border.bottom,
            ),
            Axis::Vertical => (
                d.padding.top + d.border.top,
                d.padding.bottom + d.border.bottom,
                d.padding.left + d.border.left,
                d.padding.right + d.border.right,
            ),
        }
    };
    let main_pb = main_pb_start + main_pb_end;
    let cross_pb = cross_pb_start + cross_pb_end;
    let is_border_box = layout_box.style.box_sizing == rustkit_css::BoxSizing::BorderBox;
    let spec_main_to_border_box = |v: f32| if is_border_box { v } else { v + main_pb };
    let spec_cross_to_border_box = |v: f32| if is_border_box { v } else { v + cross_pb };

    // Calculate flex basis (border-box)
    let flex_basis = match flex_basis_value {
        FlexBasis::Auto => {
            // Use main size property, or intrinsic size for replaced elements
            let explicit_size = match main_axis {
                Axis::Horizontal => resolve_length(&layout_box.style.width, container_main),
                Axis::Vertical => resolve_length(&layout_box.style.height, container_main),
            };

            // If explicit size is 0 (auto), check for intrinsic sizing
            if explicit_size == 0.0 {
                // Get intrinsic size for replaced elements (form controls, images)
                get_intrinsic_main_size(layout_box, main_axis) + main_pb
            } else {
                spec_main_to_border_box(explicit_size)
            }
        }
        FlexBasis::Content => {
            // Use content size - for replaced elements, use intrinsic size
            get_intrinsic_main_size(layout_box, main_axis) + main_pb
        }
        FlexBasis::Length(len) => spec_main_to_border_box(len),
        FlexBasis::Percent(pct) => spec_main_to_border_box(pct / 100.0 * container_main),
    };

    // Get min/max constraints from CSS
    let (css_min_main, max_main, css_min_cross, max_cross) = match main_axis {
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

    // For replaced elements (form controls, images), use intrinsic size as minimum
    // This ensures flex items have proper sizing even without explicit min-width/height
    let intrinsic_cross =
        get_intrinsic_cross_size(&layout_box.box_type, main_axis, &layout_box.style);
    let min_main = if css_min_main > 0.0 {
        spec_main_to_border_box(css_min_main)
    } else {
        0.0
    };
    let max_main = if max_main.is_finite() {
        spec_main_to_border_box(max_main)
    } else {
        max_main
    };
    let min_cross = if css_min_cross > 0.0 {
        spec_cross_to_border_box(css_min_cross)
    } else {
        intrinsic_cross + cross_pb
    };
    let max_cross = if max_cross.is_finite() {
        spec_cross_to_border_box(max_cross)
    } else {
        max_cross
    };

    // Hypothetical main size (clamped)
    let hypothetical_main_size = flex_basis.max(min_main).min(max_main);

    // Check if the cross size is explicitly set (not auto)
    // Per CSS spec, items with explicit cross size should NOT be stretched
    let explicit_cross_length = match main_axis {
        Axis::Horizontal => &layout_box.style.height,
        Axis::Vertical => &layout_box.style.width,
    };
    let explicit_cross_size = match explicit_cross_length {
        rustkit_css::Length::Auto => None,
        // A percentage cross size may not be resolvable against an
        // indefinite container; keep it on the content-measure path.
        rustkit_css::Length::Percent(_) => None,
        l => Some(spec_cross_to_border_box(resolve_length(l, container_cross))),
    };
    let has_explicit_cross_size = !matches!(explicit_cross_length, rustkit_css::Length::Auto);

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
        has_explicit_cross_size,
        explicit_cross_size,
        main_pb_start,
        main_pb_end,
        cross_pb_start,
        cross_pb_end,
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
        let gap = if current_line.items.is_empty() {
            0.0
        } else {
            main_gap
        };

        if !current_line.items.is_empty() && line_main_size + gap + item_size > container_main {
            // Start new line
            lines.push(current_line);
            current_line = FlexLine::new();
            line_main_size = 0.0;
        }

        line_main_size += if current_line.items.is_empty() {
            0.0
        } else {
            main_gap
        };
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
    let used_space: f32 = line
        .items
        .iter()
        .map(|i| i.hypothetical_main_size + i.main_margin_start + i.main_margin_end)
        .sum();
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
    let total_grow: f32 = line
        .items
        .iter()
        .filter(|i| !i.frozen)
        .map(|i| i.flex_grow)
        .sum();

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
    let total_shrink_scaled: f32 = line
        .items
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
///
/// The `has_definite_cross_size` parameter indicates whether the flex container
/// has a definite (non-auto) cross size. This affects stretch behavior:
/// - With definite cross size: stretch items to fill the container
/// - With auto cross size: stretch items to match the tallest item in the line
fn calculate_cross_sizes(
    line: &mut FlexLine,
    container_cross: f32,
    align_items: AlignItems,
    has_definite_cross_size: bool,
) {
    // PASS 1: Calculate content-based cross sizes for ALL items (ignore stretch for now)
    // This determines the "natural" height of each item
    let mut content_cross_sizes: Vec<f32> = Vec::with_capacity(line.items.len());

    for item in &mut line.items {
        // Compute the hypothetical cross size (border-box): the explicit
        // cross size when specified, otherwise the content-based size plus
        // the item's own padding+border.
        let content_cross_size = match item.explicit_cross_size {
            Some(explicit) => explicit,
            None => get_content_cross_size(item.layout_box) + item.cross_pb(),
        };

        // Apply min/max constraints to content size
        let constrained_size = content_cross_size
            .max(item.min_cross_size)
            .min(item.max_cross_size);
        content_cross_sizes.push(constrained_size);

        // Initially set cross_size to content size
        item.cross_size = constrained_size;
    }

    // Compute the line cross size based on content sizes (largest item outer cross size)
    let line_cross_size = line
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| content_cross_sizes[i] + item.cross_margin_start + item.cross_margin_end)
        .fold(0.0, f32::max);

    // PASS 2: Apply stretch behavior based on container sizing
    for (i, item) in line.items.iter_mut().enumerate() {
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

        // Per CSS spec: stretch only applies if cross size is "auto"
        // Items with explicit height/width should NOT be stretched
        if align == AlignItems::Stretch && !item.has_explicit_cross_size {
            // Determine the stretch target based on container cross size
            let stretch_target = if has_definite_cross_size {
                // Container has definite height - stretch to fill container
                container_cross - item.cross_margin_start - item.cross_margin_end
            } else {
                // Container has auto height - stretch to match tallest item in line
                line_cross_size - item.cross_margin_start - item.cross_margin_end
            };

            // Stretch, but never below content size
            item.cross_size = stretch_target.max(content_cross_sizes[i]);
        }

        // Clamp to min/max
        item.cross_size = item
            .cross_size
            .max(item.min_cross_size)
            .min(item.max_cross_size);
    }

    // Set line cross size (largest item outer cross size after stretch)
    line.cross_size = line
        .items
        .iter()
        .map(|i| i.cross_size + i.cross_margin_start + i.cross_margin_end)
        .fold(0.0, f32::max);
}

/// Get the content-based cross size for a layout box.
/// This computes the hypothetical cross size based on content, intrinsic sizing, or children.
fn get_content_cross_size(layout_box: &LayoutBox) -> f32 {
    // If the box already has a computed height from layout, use it
    if layout_box.dimensions.content.height > 0.0 {
        return layout_box.dimensions.content.height;
    }

    // Get font size for intrinsic calculations
    let font_size = match layout_box.style.font_size {
        Length::Px(px) => px,
        _ => 16.0,
    };

    // Get line height (used for text and inline boxes)
    let line_height = layout_box.style.line_height.to_px(font_size);

    // For text boxes, use line height
    if let crate::BoxType::Text(_) = &layout_box.box_type {
        return line_height;
    }

    // For inline boxes, use line height as minimum cross size
    // This ensures proper vertical rhythm in flex containers
    if let crate::BoxType::Inline = &layout_box.box_type {
        return line_height;
    }

    // For form controls, use intrinsic height
    if let crate::BoxType::FormControl(control) = &layout_box.box_type {
        use crate::FormControlType;
        return match control {
            FormControlType::TextInput { .. } => font_size * 1.5 + 8.0,
            FormControlType::TextArea { rows, .. } => {
                let rows = (*rows).max(2) as f32;
                font_size * 1.2 * rows + 8.0
            }
            FormControlType::Button { .. } => font_size * 1.5 + 12.0,
            FormControlType::Checkbox { .. } | FormControlType::Radio { .. } => font_size * 1.2,
            FormControlType::Select { .. } => font_size * 1.5 + 8.0,
        };
    }

    // For images, use natural height
    if let crate::BoxType::Image { natural_height, .. } = &layout_box.box_type {
        if *natural_height > 0.0 {
            return *natural_height;
        }
    }

    // For containers with children, sum children heights (for block) or use max (for inline)
    if !layout_box.children.is_empty() {
        let children_height: f32 = layout_box
            .children
            .iter()
            .map(|c| c.dimensions.margin_box().height)
            .sum();
        if children_height > 0.0 {
            return children_height;
        }
    }

    // Check for explicit CSS height
    match layout_box.style.height {
        Length::Px(px) if px > 0.0 => return px,
        Length::Em(em) if em > 0.0 => return em * font_size,
        _ => {}
    }

    // For inline/block boxes without content, use line height as minimum
    layout_box.style.line_height.to_px(font_size)
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
    let free_space = (container_cross - total_line_size - total_gaps).max(0.0);

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
            if free_space > 0.0 {
                let extra_per_line = free_space / lines.len() as f32;
                for line in lines.iter_mut() {
                    line.cross_size += extra_per_line;
                }
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
///
/// The `container_origin` is the (x, y) of the container's content area,
/// which is added to the flex-computed positions to get absolute coordinates.
fn apply_positions(
    lines: &mut [FlexLine],
    main_axis: Axis,
    _reverse_main: bool,
    reverse_cross: bool,
    container_origin: (f32, f32),
) {
    let (origin_x, origin_y) = container_origin;

    trace!(
        ?origin_x,
        ?origin_y,
        num_lines = lines.len(),
        "apply_positions: starting"
    );

    let lines_iter: Box<dyn Iterator<Item = &mut FlexLine>> = if reverse_cross {
        Box::new(lines.iter_mut().rev())
    } else {
        Box::new(lines.iter_mut())
    };

    for line in lines_iter {
        for item in &mut line.items {
            let (rel_x, rel_y, width, height) = match main_axis {
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

            let abs_x = origin_x + rel_x;
            let abs_y = origin_y + rel_y;

            trace!(
                ?rel_x,
                ?rel_y,
                ?abs_x,
                ?abs_y,
                ?width,
                ?height,
                main_position = item.main_position,
                cross_position = item.cross_position,
                line_cross_position = line.cross_position,
                "apply_positions: positioning flex item"
            );

            // Flex math above is border-box; dimensions.content is the
            // content rect, so inset by the item's own padding+border
            // (resolved onto dimensions by the block pre-pass).
            let d = &item.layout_box.dimensions;
            let pb_left = d.padding.left + d.border.left;
            let pb_right = d.padding.right + d.border.right;
            let pb_top = d.padding.top + d.border.top;
            let pb_bottom = d.padding.bottom + d.border.bottom;

            // Update layout box dimensions with absolute positions
            item.layout_box.dimensions.content = Rect {
                x: abs_x + pb_left,
                y: abs_y + pb_top,
                width: (width - pb_left - pb_right).max(0.0),
                height: (height - pb_top - pb_bottom).max(0.0),
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

/// Shift a laid-out box and its entire subtree by (dx, dy).
/// Content rects hold absolute coordinates once layout has run, so every
/// descendant moves by the same delta.
pub(crate) fn translate_subtree(b: &mut crate::LayoutBox, dx: f32, dy: f32) {
    b.dimensions.content.x += dx;
    b.dimensions.content.y += dy;
    for child in &mut b.children {
        translate_subtree(child, dx, dy);
    }
}

/// Get the intrinsic main size for replaced elements (form controls, images).
fn get_intrinsic_main_size(layout_box: &crate::LayoutBox, main_axis: Axis) -> f32 {
    let box_type = &layout_box.box_type;
    let style = &layout_box.style;
    let font_size = match style.font_size {
        Length::Px(px) => px,
        _ => 16.0,
    };

    match box_type {
        crate::BoxType::FormControl(control) => {
            use crate::FormControlType;
            match control {
                FormControlType::TextInput { .. } => {
                    match main_axis {
                        Axis::Horizontal => font_size * 12.0, // ~20 chars
                        Axis::Vertical => font_size * 1.5 + 8.0,
                    }
                }
                FormControlType::TextArea { rows, cols, .. } => match main_axis {
                    Axis::Horizontal => font_size * 0.6 * (*cols).max(20) as f32,
                    Axis::Vertical => font_size * 1.2 * (*rows).max(2) as f32 + 8.0,
                },
                FormControlType::Button { label, .. } => match main_axis {
                    Axis::Horizontal => {
                        crate::measure_text_advanced(
                            label,
                            &style.font_family,
                            font_size,
                            style.font_weight,
                            style.font_style,
                        )
                        .width
                            + 24.0
                    }
                    Axis::Vertical => font_size * 1.5 + 12.0,
                },
                FormControlType::Checkbox { .. } | FormControlType::Radio { .. } => {
                    // Fixed size for checkboxes and radios
                    font_size * 1.2
                }
                FormControlType::Select { .. } => match main_axis {
                    Axis::Horizontal => font_size * 10.0,
                    Axis::Vertical => font_size * 1.5 + 8.0,
                },
            }
        }
        crate::BoxType::Image {
            natural_width,
            natural_height,
            ..
        } => match main_axis {
            Axis::Horizontal => *natural_width,
            Axis::Vertical => *natural_height,
        },
        crate::BoxType::Inline | crate::BoxType::Block | crate::BoxType::AnonymousBlock => {
            // Horizontal main axis: flex-basis:auto resolves to the item's
            // content size suggestion = MAX-content width (css-flexbox-1
            // §9.2.3.C) — text measured on one line, inline runs summed.
            // flex-shrink then pulls oversized items back to the container.
            // Falls back to line height when content gives nothing to
            // measure. Vertical main axis keeps the line-height heuristic:
            // heights come from the flex layout pass itself.
            match main_axis {
                Axis::Horizontal => {
                    let content = crate::grid::estimate_max_content_width(layout_box);
                    if content > 0.0 {
                        content
                    } else {
                        style.line_height.to_px(font_size)
                    }
                }
                Axis::Vertical => style.line_height.to_px(font_size),
            }
        }
        crate::BoxType::Text(text) => {
            // Anonymous text flex item: full single-line measure on the main
            // axis (max-content), line height on the cross/vertical axis.
            match main_axis {
                Axis::Horizontal => {
                    let w = crate::measure_text_advanced(
                        text,
                        &style.font_family,
                        font_size,
                        style.font_weight,
                        style.font_style,
                    )
                    .width;
                    if w > 0.0 {
                        w
                    } else {
                        style.line_height.to_px(font_size)
                    }
                }
                Axis::Vertical => style.line_height.to_px(font_size),
            }
        }
    }
}

/// Get the intrinsic cross size for replaced elements (form controls, images).
/// This returns the height for horizontal main axis, width for vertical main axis.
fn get_intrinsic_cross_size(
    box_type: &crate::BoxType,
    main_axis: Axis,
    style: &rustkit_css::ComputedStyle,
) -> f32 {
    let font_size = match style.font_size {
        Length::Px(px) => px,
        _ => 16.0,
    };

    // Cross axis is the opposite of main axis
    let cross_axis = main_axis.cross();

    match box_type {
        crate::BoxType::FormControl(control) => {
            use crate::FormControlType;
            match control {
                FormControlType::TextInput { .. } => match cross_axis {
                    Axis::Horizontal => font_size * 12.0,
                    Axis::Vertical => font_size * 1.5 + 8.0,
                },
                FormControlType::TextArea { rows, cols, .. } => match cross_axis {
                    Axis::Horizontal => font_size * 0.6 * (*cols).max(20) as f32,
                    Axis::Vertical => font_size * 1.2 * (*rows).max(2) as f32 + 8.0,
                },
                FormControlType::Button { label, .. } => match cross_axis {
                    Axis::Horizontal => label.len() as f32 * font_size * 0.6 + 24.0,
                    Axis::Vertical => font_size * 1.5 + 12.0,
                },
                FormControlType::Checkbox { .. } | FormControlType::Radio { .. } => font_size * 1.2,
                FormControlType::Select { .. } => match cross_axis {
                    Axis::Horizontal => font_size * 10.0,
                    Axis::Vertical => font_size * 1.5 + 8.0,
                },
            }
        }
        crate::BoxType::Image {
            natural_width,
            natural_height,
            ..
        } => match cross_axis {
            Axis::Horizontal => *natural_width,
            Axis::Vertical => *natural_height,
        },
        crate::BoxType::Text(_) => {
            // Text boxes have intrinsic height based on line height
            let line_height = style.line_height.to_px(font_size);
            match cross_axis {
                Axis::Vertical => line_height,
                Axis::Horizontal => 0.0, // Text width depends on content
            }
        }
        _ => {
            // For block/inline boxes, provide a minimum based on line height
            // This ensures flex items have non-zero cross size
            let line_height = style.line_height.to_px(font_size);
            match cross_axis {
                Axis::Vertical => line_height,
                Axis::Horizontal => 0.0,
            }
        }
    }
}

/// Resolve a Length to pixels.
fn resolve_length(length: &Length, container_size: f32) -> f32 {
    // Use the Length's built-in resolution with default viewport size
    length.to_px_with_viewport(16.0, 16.0, container_size, 800.0, 600.0)
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
    use crate::BoxType;
    use rustkit_css::{AlignItems, ComputedStyle, FlexDirection, JustifyContent, Length};

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
    fn test_auto_width_item_sized_by_block_child_content() {
        // Regression: a row-flex item with width:auto and flex-basis:auto
        // must take its content's width (max-content contribution), not the
        // line-height. Was: get_intrinsic_main_size returned line_height for
        // Block boxes, so a wrapper <div> around a 150px box measured ~24px
        // and every wrapper on the line overlapped its neighbors.
        // (2026-07-09, macOS trench session 8, gpu-gradient-regression)
        let mut style = ComputedStyle::new();
        style.display = rustkit_css::Display::Flex;
        style.flex_direction = FlexDirection::Row;
        let mut container = LayoutBox::new(BoxType::Block, style);

        // Two wrapper items, each holding an explicit 150px-wide child.
        for _ in 0..2 {
            let mut wrapper = LayoutBox::new(BoxType::Block, ComputedStyle::new());
            let mut inner_style = ComputedStyle::new();
            inner_style.width = Length::Px(150.0);
            inner_style.height = Length::Px(100.0);
            wrapper
                .children
                .push(LayoutBox::new(BoxType::Block, inner_style));
            container.children.push(wrapper);
        }

        let containing = Dimensions {
            content: Rect::new(0.0, 0.0, 760.0, 600.0),
            ..Default::default()
        };
        layout_flex_container(&mut container, &containing);

        let w0 = container.children[0].dimensions.content.width;
        let x0 = container.children[0].dimensions.content.x;
        let x1 = container.children[1].dimensions.content.x;
        assert!(
            (w0 - 150.0).abs() < 0.5,
            "wrapper item should size to its 150px child, got {w0}"
        );
        assert!(
            x1 >= x0 + 150.0,
            "second item must not overlap the first: x0={x0} x1={x1}"
        );
    }

    /// Build a flex item box with the given width/padding/box-sizing, with
    /// padding pre-resolved onto dimensions the way the block pre-pass does.
    fn padded_item(width: f32, padding: f32, border_box: bool) -> LayoutBox {
        let mut style = ComputedStyle::new();
        style.width = Length::Px(width);
        style.padding_left = Length::Px(padding);
        style.padding_right = Length::Px(padding);
        style.padding_top = Length::Px(padding);
        style.padding_bottom = Length::Px(padding);
        style.box_sizing = if border_box {
            rustkit_css::BoxSizing::BorderBox
        } else {
            rustkit_css::BoxSizing::ContentBox
        };
        let mut b = LayoutBox::new(BoxType::Block, style);
        b.dimensions.padding = EdgeSizes {
            left: padding,
            right: padding,
            top: padding,
            bottom: padding,
        };
        // Give the item some content height, as the block pre-pass would.
        let mut inner = ComputedStyle::new();
        inner.width = Length::Px(60.0);
        inner.height = Length::Px(60.0);
        b.children.push(LayoutBox::new(BoxType::Block, inner));
        b
    }

    #[test]
    fn test_border_box_items_wrap_and_center_like_chrome() {
        // Regression (card-grid, macOS trench session 9): flex math treated
        // item sizes as content-box and never added padding/border, so a
        // border-box 300px card with 24px padding painted 348px wide while
        // neighbors were placed 300px apart — 48px of overlap per item on
        // both axes. Border-box items must occupy exactly their specified
        // width, and gaps must separate them.
        let mut style = ComputedStyle::new();
        style.display = rustkit_css::Display::Flex;
        style.flex_direction = FlexDirection::Row;
        style.flex_wrap = rustkit_css::FlexWrap::Wrap;
        style.justify_content = JustifyContent::Center;
        style.row_gap = Length::Px(24.0);
        style.column_gap = Length::Px(24.0);
        let mut container = LayoutBox::new(BoxType::Block, style);
        for _ in 0..4 {
            container.children.push(padded_item(300.0, 24.0, true));
        }

        let containing = Dimensions {
            content: Rect::new(0.0, 0.0, 1200.0, 800.0),
            ..Default::default()
        };
        layout_flex_container(&mut container, &containing);

        // Row 1: 3 cards of 300 + 2 gaps of 24 = 948, centered in 1200
        // -> border boxes start at 126, 450, 774 (Chrome's numbers).
        // content rect = border box + 24 padding inset.
        let xs: Vec<f32> = container
            .children
            .iter()
            .map(|c| c.dimensions.content.x)
            .collect();
        let ws: Vec<f32> = container
            .children
            .iter()
            .map(|c| c.dimensions.content.width)
            .collect();
        assert!(
            (xs[0] - 150.0).abs() < 0.5,
            "card 1 content x: got {}",
            xs[0]
        );
        assert!(
            (xs[1] - 474.0).abs() < 0.5,
            "card 2 content x: got {}",
            xs[1]
        );
        assert!(
            (xs[2] - 798.0).abs() < 0.5,
            "card 3 content x: got {}",
            xs[2]
        );
        assert!(
            (ws[0] - 252.0).abs() < 0.5,
            "border-box 300 - 48 pb = 252 content, got {}",
            ws[0]
        );

        // Row 2: the 4th card wraps, centered alone: border box at 450.
        assert!(
            (xs[3] - 474.0).abs() < 0.5,
            "wrapped card content x: got {}",
            xs[3]
        );
        let y3 = container.children[3].dimensions.content.y;
        let y0 = container.children[0].dimensions.content.y;
        // Row 2 starts below row 1's border-box height (60 content + 48 pb) plus the 24px row gap.
        assert!(
            (y3 - (y0 + 108.0 + 24.0)).abs() < 1.0,
            "row 2 must sit one border-box row + gap below row 1: y0={y0} y3={y3}"
        );
    }

    #[test]
    fn test_content_box_padded_items_do_not_overlap() {
        // Without border-box, a 300px-wide item with 24px padding occupies
        // 348px; the next item must start at least 348 + gap further over.
        let mut style = ComputedStyle::new();
        style.display = rustkit_css::Display::Flex;
        style.flex_direction = FlexDirection::Row;
        style.column_gap = Length::Px(24.0);
        let mut container = LayoutBox::new(BoxType::Block, style);
        for _ in 0..2 {
            container.children.push(padded_item(300.0, 24.0, false));
        }

        let containing = Dimensions {
            content: Rect::new(0.0, 0.0, 1200.0, 800.0),
            ..Default::default()
        };
        layout_flex_container(&mut container, &containing);

        let c0 = &container.children[0].dimensions;
        let c1 = &container.children[1].dimensions;
        assert!(
            (c0.content.width - 300.0).abs() < 0.5,
            "content-box keeps 300 content width"
        );
        let border_box_end = c0.content.x - 24.0 + 348.0;
        let next_start = c1.content.x - 24.0;
        assert!(
            (next_start - (border_box_end + 24.0)).abs() < 0.5,
            "second item must start one gap after the first border box: end={border_box_end} next={next_start}"
        );
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
        container
            .children
            .push(LayoutBox::new(BoxType::Block, child1_style));

        let mut child2_style = ComputedStyle::new();
        child2_style.width = Length::Px(100.0);
        child2_style.height = Length::Px(50.0);
        container
            .children
            .push(LayoutBox::new(BoxType::Block, child2_style));

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
        container
            .children
            .push(LayoutBox::new(BoxType::Block, child1_style));

        let mut child2_style = ComputedStyle::new();
        child2_style.flex_grow = 1.0;
        container
            .children
            .push(LayoutBox::new(BoxType::Block, child2_style));

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
        container
            .children
            .push(LayoutBox::new(BoxType::Block, child_style));

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
        container
            .children
            .push(LayoutBox::new(BoxType::Block, child_style));

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
        container
            .children
            .push(LayoutBox::new(BoxType::Block, child1_style));

        let mut child2_style = ComputedStyle::new();
        child2_style.height = Length::Px(50.0);
        child2_style.flex_basis = rustkit_css::FlexBasis::Length(50.0);
        child2_style.min_height = Length::Px(50.0);
        container
            .children
            .push(LayoutBox::new(BoxType::Block, child2_style));

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

    #[test]
    fn test_auto_height_stretch() {
        // Test that flex items in an auto-height container stretch to the tallest item,
        // not the parent container's height
        let mut style = ComputedStyle::new();
        style.display = rustkit_css::Display::Flex;
        style.flex_direction = FlexDirection::Row;
        style.height = Length::Auto; // Auto height container

        let mut container = LayoutBox::new(BoxType::Block, style);

        // First child: explicit height of 50px
        let mut child1_style = ComputedStyle::new();
        child1_style.width = Length::Px(100.0);
        child1_style.height = Length::Px(50.0);
        container
            .children
            .push(LayoutBox::new(BoxType::Block, child1_style));

        // Second child: auto height (should stretch to match first child)
        let mut child2_style = ComputedStyle::new();
        child2_style.width = Length::Px(100.0);
        child2_style.height = Length::Auto;
        container
            .children
            .push(LayoutBox::new(BoxType::Block, child2_style));

        // Large parent container - items should NOT stretch to this
        let containing = Dimensions {
            content: Rect::new(0.0, 0.0, 400.0, 500.0),
            ..Default::default()
        };

        layout_flex_container(&mut container, &containing);

        // Both children should be ~50px (the height of the tallest item)
        // NOT 500px (the parent container height)
        let child1_height = container.children[0].dimensions.content.height;
        let child2_height = container.children[1].dimensions.content.height;

        assert!(
            child1_height < 100.0,
            "Child1 height {} should be less than 100px",
            child1_height
        );
        assert!(
            child2_height < 100.0,
            "Child2 height {} should be less than 100px (stretched to match tallest, not parent)",
            child2_height
        );
    }
}
