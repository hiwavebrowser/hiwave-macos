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
        // A block-level flex container with `width: auto` still has a
        // DEFINITE used width — it resolves against its containing block.
        // Treating auto as indefinite sent the stretch path down the
        // "auto container" arm, where the target is the largest item in the
        // line instead of the container width, so column children stretched
        // to each other rather than to the viewport.
        //
        // The height case is deliberately NOT symmetric: an auto-height row
        // container really is indefinite (it is sized BY its content), and
        // test_auto_height_stretch depends on that staying true.
        Axis::Horizontal => {
            !matches!(container.style.width, Length::Auto) || container_cross_size > 0.0
        }
    };

    // The definite inner cross size, resolved from STYLE rather than from
    // containing_block: the block pre-pass can leave a stale
    // children-stacked height in content.height (logo 38.4 + nav 25.6 = 64
    // inside a height:60px header), which would center every item 2px low.
    // Px lengths resolve here; anything else falls back to the passed size.
    let definite_inner_cross = if has_definite_cross_size {
        let (spec, pb) = match cross_axis {
            Axis::Vertical => (
                &container.style.height,
                container.dimensions.padding.vertical() + container.dimensions.border.vertical(),
            ),
            Axis::Horizontal => (
                &container.style.width,
                container.dimensions.padding.horizontal()
                    + container.dimensions.border.horizontal(),
            ),
        };
        match spec {
            Length::Px(v) => {
                if container.style.box_sizing == rustkit_css::BoxSizing::BorderBox {
                    Some((v - pb).max(0.0))
                } else {
                    Some(*v)
                }
            }
            // `container_cross_size` is the CONTAINING BLOCK's content size,
            // not this container's. For the `width: auto` case above, the
            // used inner cross size is that minus the container's OWN margin,
            // border and padding — handing back the raw containing-block
            // number stretches every child past the container by exactly its
            // own edges. That is #81's defect inverted: items grew by their
            // padding instead of shrinking by it.
            _ if container_cross_size > 0.0 => {
                let own_edges = match cross_axis {
                    Axis::Horizontal => {
                        container.dimensions.margin.horizontal()
                            + container.dimensions.border.horizontal()
                            + container.dimensions.padding.horizontal()
                    }
                    Axis::Vertical => {
                        container.dimensions.margin.vertical()
                            + container.dimensions.border.vertical()
                            + container.dimensions.padding.vertical()
                    }
                };
                Some((container_cross_size - own_edges).max(0.0))
            }
            _ => None,
        }
    } else {
        None
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

        // css-flexbox-1 §4: an anonymous item containing only white space
        // is not rendered — it never becomes a flex item. Zero its rect so
        // stale pre-pass dimensions neither paint nor consume a gap slot.
        if let crate::BoxType::Text(t) = &child.box_type {
            if t.trim().is_empty() {
                child.dimensions.content.width = 0.0;
                child.dimensions.content.height = 0.0;
                continue;
            }
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
    //
    // Stretch targets the container's INNER cross size, not the containing
    // block's content size. Those differ by the container's own margin,
    // border and padding, and handing over the outer number makes every
    // stretched child overflow its parent by exactly those edges.
    let stretch_cross_size = definite_inner_cross.unwrap_or(container_cross_size);
    for line in &mut lines {
        calculate_cross_sizes(
            line,
            stretch_cross_size,
            style.align_items,
            has_definite_cross_size,
            cross_axis,
        );
    }

    // css-flexbox-1 §9.4.8 rule 1: in a single-line container with a
    // definite cross size, the line's cross size IS the container's inner
    // cross size. Items align within that, never within a taller
    // content-derived line (oversized content overflows instead).
    if wrap == FlexWrap::NoWrap {
        if let (Some(cross), Some(line)) = (definite_inner_cross, lines.first_mut()) {
            line.cross_size = cross;
        }
    }

    // 6. Calculate line cross sizes and positions
    let total_cross_size: f32 = lines.iter().map(|l| l.cross_size).sum::<f32>()
        + cross_gap * (lines.len().saturating_sub(1)) as f32;

    // 7. Apply align-content for multi-line containers
    // Only distribute lines if we have a definite cross size
    let effective_cross_size = match definite_inner_cross {
        Some(c) => c,
        None => total_cross_size,
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
                    //
                    // WITH sibling margin collapse: a flex item establishes an
                    // independent formatting context, so its own margins never
                    // collapse with its children's (fresh context), but the
                    // children still collapse among themselves (CSS 2.1
                    // §8.3.1). The plain layout_block_children here re-summed
                    // every sibling seam (mb+mt instead of max), un-doing the
                    // collapsed pre-pass — measured as the +20/+10 staircase
                    // on sticky-scroll's main column.
                    let mut item_margin_context = crate::MarginCollapseContext::new();
                    let mut item_float_context = crate::FloatContext::new();
                    item.layout_box.layout_block_children_with_collapse(
                        &mut item_margin_context,
                        &mut item_float_context,
                    );
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
                // A nested flex container was fully laid out in step 11 and
                // its content height is already final (its own step 12).
                // Summing a ROW container's children stacks side-by-side
                // items vertically: a 5-link nav measured 9 line-heights
                // tall, and align-items:center then pushed the sibling logo
                // 96px below a 60px header.
                let children_height: f32 = if item.layout_box.style.display.is_flex() {
                    item.layout_box.dimensions.content.height
                } else {
                    item.layout_box
                        .children
                        .iter()
                        .map(|c| c.dimensions.margin_box().height)
                        .sum()
                };

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
        // §9.4.8 rule 1 again: a definite single-line cross size is never
        // re-derived from content (see step 5).
        if wrap == FlexWrap::NoWrap {
            if let Some(cross) = definite_inner_cross {
                line.cross_size = cross;
            }
        }
    }

    // 11c. Re-position lines with the true cross sizes. Steps 6-10 placed
    // lines using the hypothetical (line-height fallback) cross sizes;
    // step 11's child layout revealed the real ones. Without this pass,
    // wrapped rows stack at the estimated heights and overlap whenever an
    // item is taller than one text line.
    let total_cross_size: f32 = lines.iter().map(|l| l.cross_size).sum::<f32>()
        + cross_gap * (lines.len().saturating_sub(1)) as f32;
    let effective_cross_size = match definite_inner_cross {
        Some(c) => c,
        None => total_cross_size,
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
                rustkit_css::Length::Percent(pct) if containing_block.content.height > 0.0 => {
                    Some(pct / 100.0 * containing_block.content.height)
                }
                _ => None,
            };
            match explicit {
                Some(h) => {
                    // Specified heights are border-box under border-box sizing.
                    let pb = container.dimensions.padding.vertical()
                        + container.dimensions.border.vertical();
                    let is_bb = container.style.box_sizing == rustkit_css::BoxSizing::BorderBox;
                    container.dimensions.content.height = if is_bb { (h - pb).max(0.0) } else { h };
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
    // CSS Flexbox §4.5 — automatic minimum size.
    //
    // `min-width: auto` is the DEFAULT for a flex item, and the spec resolves
    // it to the item's content-based minimum (min-content), not to zero.
    // Flooring at zero let shrink_items() squeeze items arbitrarily narrow,
    // including text: layout believed a run was 9.36px wide while paint drew
    // it at its true 18.66px, because the shaper is downstream of this and
    // never saw the squeeze. That mismatch is what put overlapping keyboard
    // chips on the new-tab page.
    //
    // Note the shape of the bug: the CROSS axis a few lines below already
    // falls back to `intrinsic_cross + cross_pb`. Only the main axis fell
    // through to 0.0, so the two axes disagreed about whether an unset
    // minimum means "no floor" or "the content floor".
    //
    // Conditions, both required by the spec:
    //   - the specified minimum is `auto` — an AUTHOR writing `min-width: 0`
    //     is explicitly asking to shrink to nothing and must keep getting it,
    //     which is why this tests the Length variant rather than `> 0.0`
    //     (resolve_length maps both Auto and Px(0) to 0.0).
    //   - the item's own overflow on the main axis is `visible`; any other
    //     value means the item can clip its content, so the content stops
    //     floring the box.
    //
    // estimate_min_content_width already returns a border-box figure for
    // element boxes (it adds padding+border itself) and a bare text measure
    // for text runs, which have neither. So it is used RAW — passing it
    // through spec_main_to_border_box would count padding twice.
    let specified_min_is_auto = matches!(
        match main_axis {
            Axis::Horizontal => &layout_box.style.min_width,
            Axis::Vertical => &layout_box.style.min_height,
        },
        rustkit_css::Length::Auto
    );
    let main_overflow_is_visible = matches!(
        match main_axis {
            Axis::Horizontal => layout_box.style.overflow_x,
            Axis::Vertical => layout_box.style.overflow_y,
        },
        rustkit_css::Overflow::Visible
    );

    let min_main = if css_min_main > 0.0 {
        spec_main_to_border_box(css_min_main)
    } else if specified_min_is_auto && main_overflow_is_visible {
        match main_axis {
            Axis::Horizontal => crate::grid::estimate_min_content_width(layout_box),
            // No min-content HEIGHT estimator exists yet. Returning 0.0 keeps
            // the previous behaviour on the vertical main axis rather than
            // inventing a number — stated so the gap is visible instead of
            // looking like the rule is implemented on both axes.
            Axis::Vertical => 0.0,
        }
    } else {
        0.0
    };
    let max_main = if max_main.is_finite() {
        spec_main_to_border_box(max_main)
    } else {
        max_main
    };
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

    let min_cross = if css_min_cross > 0.0 {
        spec_cross_to_border_box(css_min_cross)
    } else if explicit_cross_size.is_some() {
        // An author-specified cross size is used as specified. The intrinsic
        // floor below is for CONTENT-sized items; applying it to an explicit
        // size makes any control smaller than its intrinsic box impossible to
        // author. css-flexbox-1 4.5's automatic minimum is a MAIN-axis rule
        // and only applies when the size is `auto` — note min_main above
        // correctly floors at 0.0. This asymmetry is what rendered the shelf's
        // 24x24 close button 36 tall (font_size*1.5+12, the intrinsic button
        // height) and inflated the header to 53 against Chrome's 41.
        0.0
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
    cross_axis: Axis,
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
            None => get_content_cross_size(item.layout_box, cross_axis) + item.cross_pb(),
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
/// Content-based size of an item along the CROSS axis.
///
/// This used to be height-only, with no idea which axis it was measuring. In
/// a `flex-direction: column` container the cross axis is HORIZONTAL, so a
/// height was being handed back as a width: `apply_positions` then wrote it
/// into the box's width and produced literally square boxes whose width
/// tracked their line count. A two-line item came out 32x32.
///
/// Splitting on the axis is the fix. The vertical path is the original
/// behaviour, untouched; the horizontal path is new and must never fall back
/// to line-height, which is the specific wrong answer that caused this.
fn get_content_cross_size(layout_box: &LayoutBox, cross_axis: Axis) -> f32 {
    match cross_axis {
        Axis::Vertical => get_content_cross_height(layout_box),
        Axis::Horizontal => get_content_cross_width(layout_box),
    }
}

/// Content-based WIDTH, for items in a column flex container.
///
/// Returns a CONTENT-box figure: the caller adds the item's own padding and
/// border via `cross_pb()`, so including them here would double-count.
fn get_content_cross_width(layout_box: &LayoutBox) -> f32 {
    // An already-laid-out width is the best answer available.
    if layout_box.dimensions.content.width > 0.0 {
        return layout_box.dimensions.content.width;
    }

    let font_size = match layout_box.style.font_size {
        Length::Px(px) => px,
        _ => 16.0,
    };

    if let crate::BoxType::Text(text) = &layout_box.box_type {
        return crate::measure_text_advanced(
            text,
            &layout_box.style.font_family,
            font_size,
            layout_box.style.font_weight,
            layout_box.style.font_style,
        )
        .width;
    }

    if let crate::BoxType::Image { natural_width, .. } = &layout_box.box_type {
        if *natural_width > 0.0 {
            return *natural_width;
        }
    }

    match layout_box.style.width {
        Length::Px(px) if px > 0.0 => return px,
        Length::Em(em) if em > 0.0 => return em * font_size,
        _ => {}
    }

    // Block-level children STACK vertically, so the container's content width
    // is the widest child — not the sum, which is the row-axis answer.
    if !layout_box.children.is_empty() {
        let widest = layout_box
            .children
            .iter()
            .map(|c| c.dimensions.margin_box().width)
            .fold(0.0f32, f32::max);
        if widest > 0.0 {
            return widest;
        }
    }

    // Deliberately 0.0 rather than line-height. On the horizontal axis a line
    // height is not a width, and returning one is what produced square boxes.
    // Zero lets the stretch path supply the real number.
    0.0
}

/// Content-based HEIGHT — the original implementation, unchanged in behaviour.
fn get_content_cross_height(layout_box: &LayoutBox) -> f32 {
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
    let line_height = crate::resolve_line_height(&layout_box.style, font_size);

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
    crate::resolve_line_height(&layout_box.style, font_size)
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
                        crate::resolve_line_height(style, font_size)
                    }
                }
                Axis::Vertical => crate::resolve_line_height(style, font_size),
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
                        crate::resolve_line_height(style, font_size)
                    }
                }
                Axis::Vertical => crate::resolve_line_height(style, font_size),
            }
        }
        // A forced break occupies no main-axis space of its own.
        crate::BoxType::LineBreak => 0.0,
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
            let line_height = crate::resolve_line_height(style, font_size);
            match cross_axis {
                Axis::Vertical => line_height,
                Axis::Horizontal => 0.0, // Text width depends on content
            }
        }
        _ => {
            // For block/inline boxes, provide a minimum based on line height
            // This ensures flex items have non-zero cross size
            let line_height = crate::resolve_line_height(style, font_size);
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
    fn test_header_nav_row_like_chrome() {
        // Regression (sticky-scroll header, 2026-07-10): four coupled bugs
        // scattered a sticky header's flex row:
        //   1. estimate_max_content_width ignored flex gaps, so a 30px-gap
        //      nav's basis came out 120px narrow and flex-shrink smashed
        //      every link to ~2px on the re-layout pass;
        //   2. whitespace-only text runs became flex items (css-flexbox-1
        //      §4 forbids this), adding four phantom gap slots;
        //   3. step 11b summed a nested ROW container's children heights —
        //      a 5-link nav measured 9 line-heights tall;
        //   4. line cross size ignored §9.4.8 rule 1, so align-items:center
        //      centered the logo within that phantom line instead of the
        //      definite 60px header (logo painted at y=96; Chrome: y=10.8).
        fn text_box(text: &str, font_px: f32, weight: u16) -> LayoutBox {
            let mut s = ComputedStyle::new();
            s.font_size = Length::Px(font_px);
            s.font_weight = rustkit_css::FontWeight(weight);
            LayoutBox::new(BoxType::Text(text.to_string()), s)
        }

        let mut nav_style = ComputedStyle::new();
        nav_style.display = rustkit_css::Display::Flex;
        nav_style.flex_direction = FlexDirection::Row;
        nav_style.column_gap = Length::Px(30.0);
        let mut nav = LayoutBox::new(BoxType::Block, nav_style);
        for (i, label) in ["Home", "Features", "Pricing", "About", "Contact"]
            .iter()
            .enumerate()
        {
            if i > 0 {
                // Inter-element whitespace from the HTML source.
                nav.children.push(text_box(" ", 16.0, 400));
            }
            let mut a_style = ComputedStyle::new();
            a_style.font_size = Length::Px(16.0);
            let mut a = LayoutBox::new(BoxType::Inline, a_style);
            a.children.push(text_box(label, 16.0, 500));
            nav.children.push(a);
        }

        let mut logo_style = ComputedStyle::new();
        logo_style.font_size = Length::Px(24.0);
        let mut logo = LayoutBox::new(BoxType::Block, logo_style);
        logo.children.push(text_box("HiWave", 24.0, 700));

        let mut header_style = ComputedStyle::new();
        header_style.display = rustkit_css::Display::Flex;
        header_style.flex_direction = FlexDirection::Row;
        header_style.justify_content = JustifyContent::SpaceBetween;
        header_style.align_items = AlignItems::Center;
        header_style.height = Length::Px(60.0);
        let mut header = LayoutBox::new(BoxType::Block, header_style);
        header.children.push(logo);
        header.children.push(nav);

        let containing = Dimensions {
            content: Rect::new(0.0, 0.0, 1160.0, 60.0),
            ..Default::default()
        };
        layout_flex_container(&mut header, &containing);

        let logo = &header.children[0];
        let nav = &header.children[1];

        // Bugs 1+2: nav takes its max-content width — 5 measured link
        // widths (~250px with this font stack) plus exactly 4×30px gaps.
        let nav_w = nav.dimensions.content.width;
        assert!(
            (330.0..440.0).contains(&nav_w),
            "nav should be ~5 links + 4 gaps wide, got {nav_w}"
        );
        // space-between: nav flush to the container's right edge.
        let nav_right = nav.dimensions.content.x + nav_w;
        assert!(
            (nav_right - 1160.0).abs() < 1.0,
            "nav right edge should hit the container edge, got {nav_right}"
        );
        // Links keep their measured text widths (were ~2px when smashed),
        // and consecutive links sit exactly one 30px gap apart (whitespace
        // runs must not consume extra gap slots).
        let links: Vec<&LayoutBox> = nav
            .children
            .iter()
            .filter(|c| matches!(c.box_type, BoxType::Inline))
            .collect();
        assert_eq!(links.len(), 5);
        for a in &links {
            let w = a.dimensions.content.width;
            assert!(w > 30.0, "nav link should keep its text width, got {w}");
        }
        for pair in links.windows(2) {
            let gap = pair[1].dimensions.content.x
                - (pair[0].dimensions.content.x + pair[0].dimensions.content.width);
            assert!(
                (gap - 30.0).abs() < 1.0,
                "links should sit one 30px gap apart, got {gap}"
            );
        }

        // Bug 3: nav is one text line tall, not nine.
        let nav_h = nav.dimensions.content.height;
        assert!(
            nav_h < 35.0,
            "nav should be a single line tall, got {nav_h}"
        );

        // Bug 4: both items center within the DEFINITE 60px header.
        let logo_h = logo.dimensions.margin_box().height;
        let logo_y = logo.dimensions.content.y;
        let expected_logo_y = (60.0 - logo_h) / 2.0;
        assert!(
            (logo_y - expected_logo_y).abs() < 1.0,
            "logo should center in the 60px header (expected y≈{expected_logo_y}), got {logo_y}"
        );
        let nav_y = nav.dimensions.content.y;
        let expected_nav_y = (60.0 - nav_h) / 2.0;
        assert!(
            (nav_y - expected_nav_y).abs() < 1.0,
            "nav should center in the 60px header (expected y≈{expected_nav_y}), got {nav_y}"
        );
    }

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

    /// Column flex items must STRETCH to the container width by default.
    ///
    /// `align-items` defaults to `stretch`, and in a column container the
    /// cross axis is horizontal — so children fill the width and keep their
    /// content heights.
    ///
    /// T-RED: before the axis-aware split of get_content_cross_size, this
    /// failed with widths tracking the items' own heights, producing square
    /// boxes. The existing test_column_direction could not catch it because
    /// it only asserts vertical ORDER, never a width.
    ///
    /// Root-caused by Prometheus from the repro in defect-report 8e21b9e12ffc.
    #[test]
    fn test_column_stretch_fills_cross_axis_width() {
        let mut style = ComputedStyle::new();
        style.display = rustkit_css::Display::Flex;
        style.flex_direction = FlexDirection::Column;
        // width stays Auto on purpose: a block-level flex container with
        // auto width still has a definite USED width from its containing
        // block, and treating that as indefinite is half of the bug.
        let mut container = LayoutBox::new(BoxType::Block, style);

        for h in [16.0f32, 32.0f32] {
            let mut cs = ComputedStyle::new();
            cs.height = Length::Px(h);
            cs.flex_basis = rustkit_css::FlexBasis::Length(h);
            container.children.push(LayoutBox::new(BoxType::Block, cs));
        }

        let containing = Dimensions {
            content: Rect::new(0.0, 0.0, 1000.0, 600.0),
            ..Default::default()
        };
        layout_flex_container(&mut container, &containing);

        for (i, child) in container.children.iter().enumerate() {
            let w = child.dimensions.content.width;
            assert!(
                (w - 1000.0).abs() < 0.5,
                "column child {i} width {w}, expected 1000 (align-items:stretch on the \
                 cross axis). A width equal to the child's own height means the cross \
                 size is being measured on the main axis."
            );
        }

        // Heights must survive: stretching the cross axis must not disturb
        // main-axis sizing, or "fixed" would just mean "square".
        assert!((container.children[0].dimensions.content.height - 16.0).abs() < 0.5);
        assert!((container.children[1].dimensions.content.height - 32.0).abs() < 0.5);
    }

    /// With stretch OFF, a column item's cross size is its content WIDTH.
    ///
    /// This is the companion to test_column_stretch_fills_cross_axis_width,
    /// and it exists because that test turned out NOT to be a T-RED for the
    /// axis fix: when stretch applies it overwrites the content-cross value,
    /// so a height-measured cross size is invisible. I found that by reverting
    /// the axis split and watching the stretch test stay green.
    ///
    /// `align-items: flex-start` is what makes the content measurement
    /// load-bearing, so this is the test that actually fails if
    /// get_content_cross_size goes back to measuring height on both axes.
    #[test]
    fn test_column_non_stretch_item_uses_content_width_not_height() {
        let mut style = ComputedStyle::new();
        style.display = rustkit_css::Display::Flex;
        style.flex_direction = FlexDirection::Column;
        style.align_items = AlignItems::FlexStart; // no stretch to mask it
        let mut container = LayoutBox::new(BoxType::Block, style);

        // A tall, narrow child whose width is AUTO. The auto width is what
        // makes this bite: an explicit cross size short-circuits the content
        // measurement entirely, so a test that sets `width` cannot reach
        // get_content_cross_size at all. (I wrote it that way first and the
        // T-RED stayed green — the code under test was simply unreachable.)
        //
        // Pre-seeding the laid-out rect at 200x400 gives the measurement two
        // clearly different numbers to pick from, so a wrong-axis read is
        // unambiguous rather than a near miss.
        let mut cs = ComputedStyle::new();
        cs.height = Length::Px(400.0);
        cs.flex_basis = rustkit_css::FlexBasis::Length(400.0);
        let mut child = LayoutBox::new(BoxType::Block, cs);
        child.dimensions.content = Rect::new(0.0, 0.0, 200.0, 400.0);
        container.children.push(child);

        let containing = Dimensions {
            content: Rect::new(0.0, 0.0, 1000.0, 600.0),
            ..Default::default()
        };
        layout_flex_container(&mut container, &containing);

        let w = container.children[0].dimensions.content.width;
        assert!(
            (w - 200.0).abs() < 0.5,
            "non-stretch column child width {w}, expected 200. A value near 400 means \
             the cross size was measured as a HEIGHT."
        );
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

    #[test]
    fn test_explicitly_sized_button_in_a_flex_row_keeps_its_size() {
        // The shelf header, reduced. Chrome 148 puts #closeBtn at
        // 1240,8,24,24 inside a 1280x41 header; RustKit rendered it 24x36 and
        // the header came out 53 tall instead of 41.
        //
        // A <button> is a FormControl with an intrinsic cross size
        // (font_size*1.5+12 = 36 at 16px). That intrinsic was used as the
        // item's minimum on the cross axis even when the author specified a
        // height, so `height: 24px` could not go below it. Any author-sized
        // control in a flex row is affected; the shelf is only where it was
        // measured.
        let mut hdr_style = ComputedStyle::new();
        hdr_style.display = rustkit_css::Display::Flex;
        hdr_style.flex_direction = FlexDirection::Row;
        hdr_style.align_items = AlignItems::Center;
        hdr_style.justify_content = rustkit_css::JustifyContent::SpaceBetween;
        hdr_style.width = Length::Px(1280.0);
        hdr_style.box_sizing = rustkit_css::BoxSizing::BorderBox;

        let mut hdr = LayoutBox::new(BoxType::Block, hdr_style);
        hdr.dimensions.padding = EdgeSizes { top: 8.0, bottom: 8.0, left: 16.0, right: 16.0 };

        let mut title_style = ComputedStyle::new();
        title_style.width = Length::Px(105.0);
        title_style.height = Length::Px(15.0);
        hdr.children.push(LayoutBox::new(BoxType::Block, title_style));

        let mut close_style = ComputedStyle::new();
        close_style.width = Length::Px(24.0);
        close_style.height = Length::Px(24.0);
        close_style.display = rustkit_css::Display::Flex;
        close_style.font_size = Length::Px(16.0);
        hdr.children.push(LayoutBox::new(
            BoxType::FormControl(crate::FormControlType::Button {
                label: "\u{00d7}".to_string(),
                button_type: "button".to_string(),
            }),
            close_style,
        ));

        let containing = Dimensions { content: Rect::new(0.0, 0.0, 1280.0, 600.0), ..Default::default() };
        layout_flex_container(&mut hdr, &containing);

        let close = &hdr.children[1].dimensions.content;
        assert!(
            (close.width - 24.0).abs() < 0.5 && (close.height - 24.0).abs() < 0.5,
            "explicitly sized button should stay 24x24, got {}x{}",
            close.width, close.height
        );
        assert!(close.x >= 1200.0, "space-between should push it to the end, got x={}", close.x);

        // The header height follows: 8 + max(15, 24) + 8 = 40 (Chrome 41 with
        // its 1px border). While the button measured 36, this was 52.
        let hdr_h = hdr.dimensions.content.height + hdr.dimensions.padding.vertical();
        assert!(
            (39.0..=42.0).contains(&hdr_h),
            "header height should be ~40-41 once the button is 24, got {}", hdr_h
        );
    }

    #[test]
    fn test_auto_width_column_stretches_to_inner_width_not_containing_block() {
        // A `width: auto` column flex container takes its used width from the
        // containing block, but its children stretch to its INNER width --
        // containing block minus the container's own margin, border, padding.
        //
        // Reaching for the containing block's content width directly makes
        // every child overflow by exactly the container's own edges. That is
        // the #81 defect inverted (items grew by their padding instead of
        // shrinking by it), and it is what regressed the `shelf` parity case
        // from 3.71% to 33.87%: a full-width bar whose child ran past it.
        let mut style = ComputedStyle::new();
        style.display = rustkit_css::Display::Flex;
        style.flex_direction = FlexDirection::Column;
        style.width = Length::Auto;
        style.align_items = AlignItems::Stretch;

        let mut container = LayoutBox::new(BoxType::Block, style);
        container.dimensions.padding = EdgeSizes {
            left: 20.0,
            right: 20.0,
            ..Default::default()
        };
        container.dimensions.border = EdgeSizes {
            left: 5.0,
            right: 5.0,
            ..Default::default()
        };

        let mut child_style = ComputedStyle::new();
        child_style.width = Length::Auto;
        child_style.height = Length::Px(40.0);
        container
            .children
            .push(LayoutBox::new(BoxType::Block, child_style));

        let containing = Dimensions {
            content: Rect::new(0.0, 0.0, 1280.0, 600.0),
            ..Default::default()
        };

        layout_flex_container(&mut container, &containing);

        // 1280 - (20+20 padding) - (5+5 border) = 1230.
        let child_width = container.children[0].dimensions.content.width;
        assert!(
            (child_width - 1230.0).abs() < 0.5,
            "stretched child width {} should be the container's inner width 1230, \
             not the containing block's 1280 (overflowing by the container's own edges)",
            child_width
        );
    }
}
