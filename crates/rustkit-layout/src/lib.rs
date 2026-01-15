//! # RustKit Layout
//!
//! Layout engine for the RustKit browser engine.
//! Implements block and inline layout algorithms.
//!
//! ## Design Goals
//!
//! 1. **Block layout**: Stack boxes vertically with margin collapse
//! 2. **Inline layout**: Flow text and inline elements horizontally with wrapping
//! 3. **Text shaping**: Use DirectWrite for accurate text measurement
//! 4. **Display list**: Generate paint commands with correct z-order
//! 5. **Positioned elements**: Support relative, absolute, fixed, sticky
//! 6. **Float layout**: Basic float behavior and clearance
//! 7. **Stacking contexts**: Z-index based paint ordering
//! 8. **Text rendering**: Font fallback, decorations, line height

pub mod flex;
pub mod forms;
pub mod grid;
pub mod images;
pub mod scroll;
pub mod text;

pub use grid::{layout_grid_container, GridItem, GridLayout, GridTrack};
pub use forms::{
    calculate_caret_position, calculate_selection_rects, render_button, render_checkbox,
    render_input, render_radio, CaretInfo, InputLayout, InputState, SelectionInfo,
};
pub use flex::{layout_flex_container, Axis, FlexItem, FlexLine};
pub use scroll::{
    calculate_scroll_into_view, handle_wheel_event, is_scroll_container, render_scrollbars,
    ScrollAlignment, Scrollbar, ScrollbarOrientation, ScrollMomentum, ScrollState, StickyOffsets,
    StickyState, WheelDeltaMode,
};
pub use images::{
    calculate_intrinsic_size, calculate_placeholder_size, render_background_image,
    render_broken_image, render_image, ImageLayoutInfo,
};
pub use text::{
    apply_text_transform, collapse_whitespace, FontCache, FontDisplay, FontFaceRule,
    FontFamilyChain, FontLoader, LineHeight, PositionedGlyph, ShapedRun, TextDecoration, TextError,
    TextMetrics, TextShaper,
};

use rustkit_css::{BoxSizing, Color, ComputedStyle, Length, TextAlign};
use std::cmp::Ordering;
use thiserror::Error;

/// Errors that can occur in layout.
#[derive(Error, Debug)]
pub enum LayoutError {
    #[error("Layout failed: {0}")]
    LayoutFailed(String),

    #[error("Text shaping error: {0}")]
    TextShapingError(String),
}

/// CSS position property values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Position {
    #[default]
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

/// CSS float property values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Float {
    #[default]
    None,
    Left,
    Right,
}

/// CSS clear property values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Clear {
    #[default]
    None,
    Left,
    Right,
    Both,
}

/// Offset values for positioned elements.
#[derive(Debug, Clone, Copy, Default)]
pub struct PositionOffsets {
    pub top: Option<f32>,
    pub right: Option<f32>,
    pub bottom: Option<f32>,
    pub left: Option<f32>,
}

/// Float exclusion area.
#[derive(Debug, Clone, Copy)]
pub struct FloatExclusion {
    pub rect: Rect,
    pub float_type: Float,
}

/// Float context for tracking float exclusions.
#[derive(Debug, Clone, Default)]
pub struct FloatContext {
    pub left_floats: Vec<FloatExclusion>,
    pub right_floats: Vec<FloatExclusion>,
}

impl FloatContext {
    /// Create a new empty float context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a left float.
    pub fn add_left(&mut self, rect: Rect) {
        self.left_floats.push(FloatExclusion {
            rect,
            float_type: Float::Left,
        });
    }

    /// Add a right float.
    pub fn add_right(&mut self, rect: Rect) {
        self.right_floats.push(FloatExclusion {
            rect,
            float_type: Float::Right,
        });
    }

    /// Get available width at a given y position.
    pub fn available_width(&self, y: f32, container_width: f32) -> (f32, f32) {
        let mut left_edge: f32 = 0.0;
        let mut right_edge: f32 = container_width;

        for float in &self.left_floats {
            if y >= float.rect.y && y < float.rect.bottom() {
                left_edge = left_edge.max(float.rect.right());
            }
        }

        for float in &self.right_floats {
            if y >= float.rect.y && y < float.rect.bottom() {
                right_edge = right_edge.min(float.rect.x);
            }
        }

        (left_edge, right_edge)
    }

    /// Clear floats up to a given y position.
    pub fn clear(&mut self, clear: Clear) -> f32 {
        let mut clear_y: f32 = 0.0;

        match clear {
            Clear::Left => {
                for float in &self.left_floats {
                    clear_y = clear_y.max(float.rect.bottom());
                }
            }
            Clear::Right => {
                for float in &self.right_floats {
                    clear_y = clear_y.max(float.rect.bottom());
                }
            }
            Clear::Both => {
                for float in &self.left_floats {
                    clear_y = clear_y.max(float.rect.bottom());
                }
                for float in &self.right_floats {
                    clear_y = clear_y.max(float.rect.bottom());
                }
            }
            Clear::None => {}
        }

        clear_y
    }
}

/// Margin collapse context.
#[derive(Debug, Clone, Default)]
pub struct MarginCollapseContext {
    /// Pending positive margin.
    pub positive_margin: f32,
    /// Pending negative margin.
    pub negative_margin: f32,
}

impl MarginCollapseContext {
    /// Create a new margin collapse context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a margin to the collapse context.
    pub fn add_margin(&mut self, margin: f32) {
        if margin >= 0.0 {
            self.positive_margin = self.positive_margin.max(margin);
        } else {
            self.negative_margin = self.negative_margin.min(margin);
        }
    }

    /// Resolve the collapsed margin.
    pub fn resolve(&self) -> f32 {
        self.positive_margin + self.negative_margin
    }

    /// Reset the context.
    pub fn reset(&mut self) {
        self.positive_margin = 0.0;
        self.negative_margin = 0.0;
    }
}

/// A 2D rectangle.
#[derive(Debug, Clone, Copy, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn zero() -> Self {
        Self::default()
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }
}

/// Edge sizes (margin, padding, border).
#[derive(Debug, Clone, Copy, Default)]
pub struct EdgeSizes {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl EdgeSizes {
    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }

    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

/// Box dimensions including content, padding, border, and margin.
#[derive(Debug, Clone, Default)]
pub struct Dimensions {
    /// Content area.
    pub content: Rect,
    /// Padding.
    pub padding: EdgeSizes,
    /// Border.
    pub border: EdgeSizes,
    /// Margin.
    pub margin: EdgeSizes,
}

impl Dimensions {
    /// Get the padding box (content + padding).
    pub fn padding_box(&self) -> Rect {
        Rect {
            x: self.content.x - self.padding.left,
            y: self.content.y - self.padding.top,
            width: self.content.width + self.padding.horizontal(),
            height: self.content.height + self.padding.vertical(),
        }
    }

    /// Get the border box (content + padding + border).
    pub fn border_box(&self) -> Rect {
        let pb = self.padding_box();
        Rect {
            x: pb.x - self.border.left,
            y: pb.y - self.border.top,
            width: pb.width + self.border.horizontal(),
            height: pb.height + self.border.vertical(),
        }
    }

    /// Get the margin box (content + padding + border + margin).
    pub fn margin_box(&self) -> Rect {
        let bb = self.border_box();
        Rect {
            x: bb.x - self.margin.left,
            y: bb.y - self.margin.top,
            width: bb.width + self.margin.horizontal(),
            height: bb.height + self.margin.vertical(),
        }
    }
}

/// Type of layout box.
#[derive(Debug, Clone)]
pub enum BoxType {
    /// Block-level box.
    Block,
    /// Inline-level box.
    Inline,
    /// Anonymous block (for grouping inline content).
    AnonymousBlock,
    /// Text run.
    Text(String),
    /// Replaced element (image).
    /// Contains: (url, natural_width, natural_height)
    Image {
        url: String,
        natural_width: f32,
        natural_height: f32,
    },
    /// Form control (input, button, textarea, select).
    FormControl(FormControlType),
}

/// Type of form control for layout/rendering.
#[derive(Debug, Clone, PartialEq)]
pub enum FormControlType {
    /// Text input field.
    TextInput {
        value: String,
        placeholder: String,
        input_type: String, // "text", "password", "email", etc.
    },
    /// Multi-line text area.
    TextArea {
        value: String,
        placeholder: String,
        rows: u32,
        cols: u32,
    },
    /// Button element.
    Button {
        label: String,
        button_type: String, // "submit", "button", "reset"
    },
    /// Checkbox input.
    Checkbox {
        checked: bool,
    },
    /// Radio button input.
    Radio {
        checked: bool,
        name: String,
    },
    /// Select dropdown (placeholder for future).
    Select {
        options: Vec<String>,
        selected_index: Option<usize>,
    },
}

/// Stacking context for z-index ordering.
#[derive(Debug, Clone, Default)]
pub struct StackingContext {
    /// Z-index value (0 for auto).
    pub z_index: i32,
    /// Whether this creates a new stacking context.
    pub creates_context: bool,
    /// Positioned children in this stacking context.
    pub positioned_children: Vec<usize>,
}

/// A layout box in the layout tree.
#[derive(Debug)]
pub struct LayoutBox {
    /// Box type.
    pub box_type: BoxType,
    /// Computed dimensions.
    pub dimensions: Dimensions,
    /// Computed style.
    pub style: ComputedStyle,
    /// Child boxes.
    pub children: Vec<LayoutBox>,
    /// CSS position property.
    pub position: Position,
    /// Position offsets (top, right, bottom, left).
    pub offsets: PositionOffsets,
    /// Float property.
    pub float: Float,
    /// Clear property.
    pub clear: Clear,
    /// Z-index for stacking.
    pub z_index: i32,
    /// Whether this box creates a stacking context.
    pub stacking_context: Option<StackingContext>,
    /// Reference to containing block (for positioned elements).
    #[allow(dead_code)]
    pub containing_block_index: Option<usize>,
    /// Viewport dimensions for resolving vh/vw units.
    pub viewport: (f32, f32),
    /// Sticky positioning state (for position: sticky elements).
    pub sticky_state: Option<StickyState>,
}

impl LayoutBox {
    /// Create a new layout box.
    pub fn new(box_type: BoxType, style: ComputedStyle) -> Self {
        Self {
            box_type,
            dimensions: Dimensions::default(),
            style,
            children: Vec::new(),
            position: Position::Static,
            offsets: PositionOffsets::default(),
            float: Float::None,
            clear: Clear::None,
            z_index: 0,
            stacking_context: None,
            containing_block_index: None,
            viewport: (0.0, 0.0),
            sticky_state: None,
        }
    }

    /// Create a new layout box with positioning.
    pub fn with_position(box_type: BoxType, style: ComputedStyle, position: Position) -> Self {
        let mut layout_box = Self::new(box_type, style);
        layout_box.position = position;

        // Create stacking context if positioned with z-index
        if position != Position::Static {
            layout_box.stacking_context = Some(StackingContext::default());
        }

        layout_box
    }

    /// Create a new layout box with float.
    pub fn with_float(box_type: BoxType, style: ComputedStyle, float: Float) -> Self {
        let mut layout_box = Self::new(box_type, style);
        layout_box.float = float;
        layout_box
    }

    /// Set z-index and create stacking context if needed.
    pub fn set_z_index(&mut self, z_index: i32) {
        self.z_index = z_index;
        if self.position != Position::Static {
            let mut ctx = self.stacking_context.take().unwrap_or_default();
            ctx.z_index = z_index;
            ctx.creates_context = true;
            self.stacking_context = Some(ctx);
        }
    }

    /// Set position offsets.
    pub fn set_offsets(
        &mut self,
        top: Option<f32>,
        right: Option<f32>,
        bottom: Option<f32>,
        left: Option<f32>,
    ) {
        self.offsets = PositionOffsets {
            top,
            right,
            bottom,
            left,
        };
    }

    /// Update sticky positions based on scroll state.
    ///
    /// This should be called before building the display list when scroll has changed.
    /// It recursively updates all sticky elements in the tree based on the current
    /// scroll position relative to their containing blocks.
    pub fn update_sticky_positions(&mut self, scroll_x: f32, scroll_y: f32, container_rect: Rect) {
        // Update this element's sticky state if it's sticky
        if let Some(ref mut sticky_state) = self.sticky_state {
            sticky_state.update(scroll_y, container_rect);

            // Apply the sticky adjustment to dimensions if stuck
            if sticky_state.is_stuck {
                if let Some(stuck_rect) = sticky_state.stuck_rect {
                    // Adjust content position to the stuck position
                    // We need to account for margin/border/padding
                    let border_box = self.dimensions.border_box();
                    let dy = stuck_rect.y - border_box.y;
                    self.dimensions.content.y += dy;

                    // Handle horizontal sticky if applicable
                    if sticky_state.offsets.left.is_some() || sticky_state.offsets.right.is_some() {
                        let dx = stuck_rect.x - border_box.x;
                        self.dimensions.content.x += dx;
                    }
                }
            }
        }

        // Determine the container rect for children
        // For scroll containers, use the content rect; otherwise pass through
        let child_container = if is_scroll_container(
            self.style.overflow_x,
            self.style.overflow_y,
        ) {
            self.dimensions.content
        } else {
            container_rect
        };

        // Recursively update children
        for child in &mut self.children {
            child.update_sticky_positions(scroll_x, scroll_y, child_container);
        }
    }

    /// Reset sticky positions to their original normal flow positions.
    ///
    /// Call this before relayout or when scroll position is reset.
    pub fn reset_sticky_positions(&mut self) {
        if let Some(ref sticky_state) = self.sticky_state {
            // Restore original position
            let original = sticky_state.original_rect;
            let border_box = self.dimensions.border_box();

            // Calculate offset from current to original
            let dx = original.x - border_box.x;
            let dy = original.y - border_box.y;

            self.dimensions.content.x += dx;
            self.dimensions.content.y += dy;
        }

        // Reset sticky state
        if let Some(ref mut sticky_state) = self.sticky_state {
            sticky_state.is_stuck = false;
            sticky_state.stuck_rect = None;
        }

        // Recursively reset children
        for child in &mut self.children {
            child.reset_sticky_positions();
        }
    }

    /// Perform layout within the given containing block.
    pub fn layout(&mut self, containing_block: &Dimensions) {
        match &self.box_type {
            BoxType::Block | BoxType::AnonymousBlock => {
                // Check for flex or grid container
                if self.style.display.is_flex() {
                    self.layout_block(containing_block);
                    // Flex layout is applied to children
                    flex::layout_flex_container(
                        self,
                        &self.dimensions.clone(),
                    );
                } else if self.style.display.is_grid() {
                    self.layout_block(containing_block);
                    // Grid layout is applied to children
                    grid::layout_grid_container(
                        self,
                        self.dimensions.content.width,
                        self.dimensions.content.height,
                    );
                } else {
                    self.layout_block(containing_block);
                }
            }
            BoxType::Inline => {
                // Inline boxes: position at containing block's current content area
                self.layout_inline(containing_block);
            }
            BoxType::Text(text) => {
                // Text boxes: calculate dimensions based on text content
                self.layout_text(text.clone(), containing_block);
            }
            BoxType::Image { natural_width, natural_height, .. } => {
                // Replaced element: use intrinsic dimensions or explicit sizing
                self.layout_image(*natural_width, *natural_height, containing_block);
            }
            BoxType::FormControl(ref control) => {
                // Form controls are replaced elements with intrinsic sizing
                self.layout_form_control(control.clone(), containing_block);
            }
        }

        // Apply positioning offsets after normal layout
        self.apply_position_offsets(containing_block);
    }

    /// Layout an inline box.
    fn layout_inline(&mut self, containing_block: &Dimensions) {
        // Calculate margins, padding, and borders for the inline box
        let d = &mut self.dimensions;
        let container_width = containing_block.content.width;
        
        d.margin.left = self.style.margin_left.to_px(16.0, 16.0, container_width);
        d.margin.right = self.style.margin_right.to_px(16.0, 16.0, container_width);
        // Vertical margins don't apply to inline elements
        d.margin.top = 0.0;
        d.margin.bottom = 0.0;
        
        d.padding.left = self.style.padding_left.to_px(16.0, 16.0, container_width);
        d.padding.right = self.style.padding_right.to_px(16.0, 16.0, container_width);
        d.padding.top = self.style.padding_top.to_px(16.0, 16.0, container_width);
        d.padding.bottom = self.style.padding_bottom.to_px(16.0, 16.0, container_width);
        
        d.border.left = self.style.border_left_width.to_px(16.0, 16.0, container_width);
        d.border.right = self.style.border_right_width.to_px(16.0, 16.0, container_width);
        d.border.top = self.style.border_top_width.to_px(16.0, 16.0, container_width);
        d.border.bottom = self.style.border_bottom_width.to_px(16.0, 16.0, container_width);
        
        // Position at containing block's content area
        d.content.x = containing_block.content.x + d.margin.left + d.border.left + d.padding.left;
        d.content.y = containing_block.content.y + containing_block.content.height;
        
        // Check for explicit CSS width first
        let explicit_width = match self.style.width {
            Length::Px(px) if px > 0.0 => Some(px),
            Length::Percent(pct) if pct > 0.0 => Some(pct / 100.0 * container_width),
            Length::Em(em) if em > 0.0 => {
                let font_size = match self.style.font_size {
                    Length::Px(px) => px,
                    _ => 16.0,
                };
                Some(em * font_size)
            }
            _ => None,
        };
        
        // Check for explicit CSS height
        let explicit_height = match self.style.height {
            Length::Px(px) if px > 0.0 => Some(px),
            Length::Percent(pct) if pct > 0.0 && containing_block.content.height > 0.0 => {
                Some(pct / 100.0 * containing_block.content.height)
            }
            Length::Em(em) if em > 0.0 => {
                let font_size = match self.style.font_size {
                    Length::Px(px) => px,
                    _ => 16.0,
                };
                Some(em * font_size)
            }
            _ => None,
        };
        
        // Layout inline children sequentially
        // Use the containing block's width for child layout, not our own (which might be 0)
        let available_width = containing_block.content.width;
        let mut cursor_x = 0.0;
        let mut max_height = 0.0f32;
        
        for child in &mut self.children {
            let mut cb = self.dimensions.clone();
            cb.content.x = self.dimensions.content.x + cursor_x;
            cb.content.width = available_width; // Pass parent's available width
            cb.content.height = 0.0;
            
            child.layout(&cb);
            
            cursor_x += child.dimensions.margin_box().width;
            max_height = max_height.max(child.dimensions.margin_box().height);
        }
        
        // Set content dimensions:
        // 1. Use explicit CSS width if specified
        // 2. Otherwise use computed width from children
        // 3. Ensure minimum width for padding/border contribution
        let computed_width = if let Some(w) = explicit_width {
            w
        } else if cursor_x > 0.0 {
            cursor_x
        } else {
            // Inline box with no children and no explicit width:
            // Use horizontal padding + border as minimum (inline-block behavior)
            let horizontal_box = self.dimensions.padding.horizontal() + self.dimensions.border.horizontal();
            horizontal_box
        };
        self.dimensions.content.width = computed_width;
        
        // Height: use explicit height, or line-height as minimum for inline boxes
        let min_height = self.dimensions.padding.vertical() + self.dimensions.border.vertical();
        let line_height = self.get_line_height();
        
        // Height calculation for inline boxes:
        // Inline boxes should always have at least line-height to maintain proper vertical rhythm.
        // This is critical for flex containers where inline items need proper sizing.
        let computed_height = if let Some(h) = explicit_height {
            h
        } else if !self.children.is_empty() {
            // Has children: use max of children height and line height
            max_height.max(line_height).max(min_height)
        } else {
            // No children: use line height as minimum (ensures proper flex item sizing)
            line_height.max(min_height)
        };
        self.dimensions.content.height = computed_height;
    }

    /// Layout a text box.
    fn layout_text(&mut self, text: String, containing_block: &Dimensions) {
        // Get font size
        let font_size = match self.style.font_size {
            Length::Px(px) => px,
            _ => 16.0,
        };

        // Get letter-spacing and word-spacing in pixels
        // CSS "normal" keyword (Auto/Zero) maps to 0.0 via the wildcard
        let letter_spacing = match self.style.letter_spacing {
            Length::Px(px) => px,
            Length::Em(em) => em * font_size,
            Length::Rem(rem) => rem * 16.0, // Root font size assumed 16px
            _ => 0.0,
        };
        let word_spacing = match self.style.word_spacing {
            Length::Px(px) => px,
            Length::Em(em) => em * font_size,
            Length::Rem(rem) => rem * 16.0,
            _ => 0.0,
        };

        // Use proper text measurement for width with spacing
        let metrics = measure_text_with_spacing(
            &text,
            &self.style.font_family,
            font_size,
            self.style.font_weight,
            self.style.font_style,
            letter_spacing,
            word_spacing,
        );
        let text_width = metrics.width;

        // Calculate text-align offset
        let container_width = containing_block.content.width;
        let text_align_offset = if container_width > text_width {
            match self.style.text_align {
                TextAlign::Left => 0.0,
                TextAlign::Right => container_width - text_width,
                TextAlign::Center => (container_width - text_width) / 2.0,
                TextAlign::Justify => 0.0, // Single text run doesn't justify
            }
        } else {
            0.0
        };

        // Position at containing block's content area with text-align offset
        self.dimensions.content.x = containing_block.content.x + text_align_offset;
        self.dimensions.content.y = containing_block.content.y + containing_block.content.height;
        // Use text width, clamping to containing block only if it has a meaningful width
        // This prevents text from collapsing to 0 width in intrinsic sizing scenarios
        self.dimensions.content.width = if container_width > 0.0 {
            text_width.min(container_width)
        } else {
            text_width // Don't clamp if containing block has no width yet
        };
        self.dimensions.content.height = self.get_line_height();
    }

    /// Layout a replaced element (image).
    fn layout_image(&mut self, natural_width: f32, natural_height: f32, containing_block: &Dimensions) {
        // Calculate explicit dimensions from style
        let explicit_width = match self.style.width {
            Length::Px(px) => Some(px),
            Length::Percent(pct) => Some(pct / 100.0 * containing_block.content.width),
            _ => None,
        };
        
        let explicit_height = match self.style.height {
            Length::Px(px) => Some(px),
            Length::Percent(pct) => Some(pct / 100.0 * containing_block.content.height),
            _ => None,
        };
        
        // Determine final dimensions using intrinsic size calculation
        let (width, height) = crate::images::calculate_intrinsic_size(
            if natural_width > 0.0 { Some(natural_width) } else { None },
            if natural_height > 0.0 { Some(natural_height) } else { None },
            explicit_width,
            explicit_height,
            containing_block.content.width,
        );
        
        // Position within containing block
        self.dimensions.content.x = containing_block.content.x;
        self.dimensions.content.y = containing_block.content.y + containing_block.content.height;
        self.dimensions.content.width = width;
        self.dimensions.content.height = height;
    }

    /// Layout a form control (input, button, textarea, etc.)
    fn layout_form_control(&mut self, control: FormControlType, containing_block: &Dimensions) {
        let font_size = match self.style.font_size {
            Length::Px(px) => px,
            _ => 16.0,
        };
        
        // Calculate intrinsic dimensions based on control type
        let (intrinsic_width, intrinsic_height) = match &control {
            FormControlType::TextInput { .. } => {
                // Default text input: ~20 characters wide, single line height
                (font_size * 12.0, font_size * 1.5 + 8.0)
            }
            FormControlType::TextArea { rows, cols, .. } => {
                // Textarea: based on rows/cols
                let rows = (*rows).max(2) as f32;
                let cols = (*cols).max(20) as f32;
                (font_size * 0.6 * cols, font_size * 1.2 * rows + 8.0)
            }
            FormControlType::Button { label, .. } => {
                // Button: width based on label, with padding
                let label_width = label.len() as f32 * font_size * 0.6;
                (label_width + 24.0, font_size * 1.5 + 12.0)
            }
            FormControlType::Checkbox { .. } | FormControlType::Radio { .. } => {
                // Fixed size for checkboxes and radios
                (font_size * 1.2, font_size * 1.2)
            }
            FormControlType::Select { .. } => {
                // Dropdown: similar to text input but with arrow space
                (font_size * 10.0, font_size * 1.5 + 8.0)
            }
        };
        
        // Override with explicit CSS dimensions if specified, but always fall back to intrinsic
        // if the explicit value resolves to zero (e.g., percent of zero-height container)
        let width = match self.style.width {
            Length::Px(px) if px > 0.0 => px,
            Length::Percent(pct) => {
                let resolved = pct / 100.0 * containing_block.content.width;
                if resolved > 0.0 { resolved } else { intrinsic_width }
            }
            Length::Em(em) if em > 0.0 => em * font_size,
            _ => intrinsic_width,
        };
        
        let height = match self.style.height {
            Length::Px(px) if px > 0.0 => px,
            Length::Percent(pct) => {
                let resolved = pct / 100.0 * containing_block.content.height;
                // CRITICAL: Fall back to intrinsic height if percent resolves to 0
                // This fixes form controls in flex containers before flex layout runs
                if resolved > 0.0 { resolved } else { intrinsic_height }
            }
            Length::Em(em) if em > 0.0 => em * font_size,
            _ => intrinsic_height,
        };
        
        // Position within containing block
        self.dimensions.content.x = containing_block.content.x;
        self.dimensions.content.y = containing_block.content.y + containing_block.content.height;
        self.dimensions.content.width = width;
        self.dimensions.content.height = height;
    }

    /// Get line height for text layout.
    fn get_line_height(&self) -> f32 {
        let font_size = match self.style.font_size {
            Length::Px(px) => px,
            _ => 16.0,
        };
        // Use line_height from style (which is a multiplier), or default to 1.2
        let line_height_multiplier = if self.style.line_height > 0.0 {
            self.style.line_height
        } else {
            1.2
        };
        font_size * line_height_multiplier
    }

    /// Perform layout with margin collapse context.
    pub fn layout_with_collapse(
        &mut self,
        containing_block: &Dimensions,
        margin_context: &mut MarginCollapseContext,
        float_context: &mut FloatContext,
    ) {
        // Handle clear property
        if self.clear != Clear::None {
            let clear_y = float_context.clear(self.clear);
            if clear_y > 0.0 {
                margin_context.reset();
            }
        }

        match &self.box_type {
            BoxType::Block | BoxType::AnonymousBlock => {
                self.layout_block_with_collapse(containing_block, margin_context, float_context);
            }
            BoxType::Inline => {
                self.layout_inline(containing_block);
            }
            BoxType::Text(text) => {
                self.layout_text(text.clone(), containing_block);
            }
            BoxType::Image { natural_width, natural_height, .. } => {
                self.layout_image(*natural_width, *natural_height, containing_block);
            }
            BoxType::FormControl(ref control) => {
                self.layout_form_control(control.clone(), containing_block);
            }
        }

        // Handle float
        if self.float != Float::None {
            self.layout_float(containing_block, float_context);
        }

        // Apply positioning offsets after normal layout
        self.apply_position_offsets(containing_block);
    }

    /// Layout a block-level box.
    fn layout_block(&mut self, containing_block: &Dimensions) {
        tracing::trace!(
            containing_width = containing_block.content.width,
            "layout_block called"
        );
        
        // Calculate width first (depends on containing block)
        self.calculate_block_width(containing_block);

        tracing::trace!(
            calculated_width = self.dimensions.content.width,
            "After calculate_block_width"
        );

        // Position the box
        self.calculate_block_position(containing_block);

        // Layout children
        self.layout_block_children();

        // Height depends on children
        self.calculate_block_height();
    }

    /// Layout a block-level box with margin collapse.
    fn layout_block_with_collapse(
        &mut self,
        containing_block: &Dimensions,
        margin_context: &mut MarginCollapseContext,
        float_context: &mut FloatContext,
    ) {
        // Calculate width first (depends on containing block)
        self.calculate_block_width(containing_block);

        // Calculate margin/padding/border
        self.calculate_block_vertical_box_model(containing_block);

        // Handle margin collapse with previous sibling
        margin_context.add_margin(self.dimensions.margin.top);
        let collapsed_margin = margin_context.resolve();

        // Position the box with collapsed margin
        self.dimensions.content.x = containing_block.content.x
            + self.dimensions.margin.left
            + self.dimensions.border.left
            + self.dimensions.padding.left;

        self.dimensions.content.y = containing_block.content.y
            + containing_block.content.height
            + collapsed_margin
            + self.dimensions.border.top
            + self.dimensions.padding.top;

        // If this box has border or padding, margins don't collapse through it
        let blocks_collapse = self.dimensions.border.top > 0.0
            || self.dimensions.padding.top > 0.0
            || self.dimensions.border.bottom > 0.0
            || self.dimensions.padding.bottom > 0.0;

        // Check for flex or grid container - these have special child layout
        if self.style.display.is_flex() {
            // For flex containers, layout children normally first to get their intrinsic sizes
            if blocks_collapse {
                let mut child_margin_context = MarginCollapseContext::new();
                self.layout_block_children_with_collapse(&mut child_margin_context, float_context);
            } else {
                self.layout_block_children_with_collapse(margin_context, float_context);
            }
            // Then apply flex layout algorithm
            flex::layout_flex_container(self, &self.dimensions.clone());
        } else if self.style.display.is_grid() {
            // For grid containers, layout children normally first
            if blocks_collapse {
                let mut child_margin_context = MarginCollapseContext::new();
                self.layout_block_children_with_collapse(&mut child_margin_context, float_context);
            } else {
                self.layout_block_children_with_collapse(margin_context, float_context);
            }
            // Then apply grid layout algorithm
            grid::layout_grid_container(
                self,
                self.dimensions.content.width,
                self.dimensions.content.height,
            );
        } else {
            // Normal block layout
            if blocks_collapse {
                let mut child_margin_context = MarginCollapseContext::new();
                self.layout_block_children_with_collapse(&mut child_margin_context, float_context);
            } else {
                // Margins can collapse through this box
                self.layout_block_children_with_collapse(margin_context, float_context);
            }
        }

        // Height depends on children
        self.calculate_block_height();

        // Reset margin context for next sibling, add bottom margin
        margin_context.reset();
        margin_context.add_margin(self.dimensions.margin.bottom);
    }

    /// Calculate vertical box model values (margin, border, padding).
    fn calculate_block_vertical_box_model(&mut self, containing_block: &Dimensions) {
        let style = &self.style;

        self.dimensions.margin.top =
            self.length_to_px(&style.margin_top, containing_block.content.width);
        self.dimensions.margin.bottom =
            self.length_to_px(&style.margin_bottom, containing_block.content.width);
        self.dimensions.border.top =
            self.length_to_px(&style.border_top_width, containing_block.content.width);
        self.dimensions.border.bottom =
            self.length_to_px(&style.border_bottom_width, containing_block.content.width);
        self.dimensions.padding.top =
            self.length_to_px(&style.padding_top, containing_block.content.width);
        self.dimensions.padding.bottom =
            self.length_to_px(&style.padding_bottom, containing_block.content.width);
    }

    /// Layout a floated box.
    fn layout_float(&mut self, containing_block: &Dimensions, float_context: &mut FloatContext) {
        // Calculate dimensions
        self.calculate_block_width(containing_block);
        self.calculate_block_vertical_box_model(containing_block);

        // Find position based on float type
        let (left_edge, right_edge) = float_context.available_width(
            containing_block.content.y + containing_block.content.height,
            containing_block.content.width,
        );

        let box_width = self.dimensions.margin_box().width;

        match self.float {
            Float::Left => {
                self.dimensions.content.x = containing_block.content.x
                    + left_edge
                    + self.dimensions.margin.left
                    + self.dimensions.border.left
                    + self.dimensions.padding.left;

                float_context.add_left(self.dimensions.margin_box());
            }
            Float::Right => {
                self.dimensions.content.x = containing_block.content.x + right_edge - box_width
                    + self.dimensions.margin.left
                    + self.dimensions.border.left
                    + self.dimensions.padding.left;

                float_context.add_right(self.dimensions.margin_box());
            }
            Float::None => {}
        }

        self.dimensions.content.y = containing_block.content.y
            + containing_block.content.height
            + self.dimensions.margin.top
            + self.dimensions.border.top
            + self.dimensions.padding.top;

        // Layout children
        self.layout_block_children();
        self.calculate_block_height();
    }

    /// Apply position offsets for positioned elements.
    fn apply_position_offsets(&mut self, containing_block: &Dimensions) {
        match self.position {
            Position::Static => {
                // No offsets applied
            }
            Position::Relative => {
                // Offset from normal flow position
                if let Some(top) = self.offsets.top {
                    self.dimensions.content.y += top;
                } else if let Some(bottom) = self.offsets.bottom {
                    self.dimensions.content.y -= bottom;
                }

                if let Some(left) = self.offsets.left {
                    self.dimensions.content.x += left;
                } else if let Some(right) = self.offsets.right {
                    self.dimensions.content.x -= right;
                }
            }
            Position::Absolute => {
                // Position relative to containing block
                if let Some(left) = self.offsets.left {
                    self.dimensions.content.x = containing_block.content.x
                        + left
                        + self.dimensions.margin.left
                        + self.dimensions.border.left
                        + self.dimensions.padding.left;
                } else if let Some(right) = self.offsets.right {
                    self.dimensions.content.x = containing_block.content.right()
                        - right
                        - self.dimensions.margin.right
                        - self.dimensions.border.right
                        - self.dimensions.padding.right
                        - self.dimensions.content.width;
                }

                if let Some(top) = self.offsets.top {
                    self.dimensions.content.y = containing_block.content.y
                        + top
                        + self.dimensions.margin.top
                        + self.dimensions.border.top
                        + self.dimensions.padding.top;
                } else if let Some(bottom) = self.offsets.bottom {
                    self.dimensions.content.y = containing_block.content.bottom()
                        - bottom
                        - self.dimensions.margin.bottom
                        - self.dimensions.border.bottom
                        - self.dimensions.padding.bottom
                        - self.dimensions.content.height;
                }
            }
            Position::Fixed => {
                // Position relative to viewport (root containing block)
                // In a full implementation, this would use the viewport dimensions
                self.apply_position_offsets_absolute(containing_block);
            }
            Position::Sticky => {
                // Sticky positioning: element stays in normal flow but can "stick"
                // when scrolled past its threshold.
                //
                // The offsets (top, left, etc.) define the sticky threshold, not
                // an initial offset like relative positioning.
                //
                // Store the original position and sticky offsets in StickyState.
                // The actual sticky adjustment happens during rendering based on scroll.
                let original_rect = self.dimensions.border_box();
                let sticky_offsets = StickyOffsets {
                    top: self.offsets.top,
                    right: self.offsets.right,
                    bottom: self.offsets.bottom,
                    left: self.offsets.left,
                };
                self.sticky_state = Some(StickyState::new(original_rect, sticky_offsets));
                // Position stays at normal flow - no offset applied during layout
            }
        }
    }

    /// Apply absolute positioning offsets.
    fn apply_position_offsets_absolute(&mut self, containing_block: &Dimensions) {
        let has_left = self.offsets.left.is_some();
        let has_right = self.offsets.right.is_some();
        let has_top = self.offsets.top.is_some();
        let has_bottom = self.offsets.bottom.is_some();

        // Handle horizontal positioning
        if has_left && has_right {
            // When both left and right are set with width: auto, stretch to fill
            let left = self.offsets.left.unwrap();
            let right = self.offsets.right.unwrap();

            // Calculate stretched width if width is auto
            if matches!(self.style.width, Length::Auto) {
                let available_width = containing_block.content.width
                    - left
                    - right
                    - self.dimensions.margin.left
                    - self.dimensions.margin.right
                    - self.dimensions.border.left
                    - self.dimensions.border.right
                    - self.dimensions.padding.left
                    - self.dimensions.padding.right;
                self.dimensions.content.width = available_width.max(0.0);
            }

            // Position from left
            self.dimensions.content.x = containing_block.content.x
                + left
                + self.dimensions.margin.left
                + self.dimensions.border.left
                + self.dimensions.padding.left;
        } else if let Some(left) = self.offsets.left {
            self.dimensions.content.x = containing_block.content.x
                + left
                + self.dimensions.margin.left
                + self.dimensions.border.left
                + self.dimensions.padding.left;
        } else if let Some(right) = self.offsets.right {
            self.dimensions.content.x = containing_block.content.right()
                - right
                - self.dimensions.margin.right
                - self.dimensions.border.right
                - self.dimensions.padding.right
                - self.dimensions.content.width;
        }

        // Handle vertical positioning
        if has_top && has_bottom {
            // When both top and bottom are set with height: auto, stretch to fill
            let top = self.offsets.top.unwrap();
            let bottom = self.offsets.bottom.unwrap();

            // Calculate stretched height if height is auto
            if matches!(self.style.height, Length::Auto) {
                let available_height = containing_block.content.height
                    - top
                    - bottom
                    - self.dimensions.margin.top
                    - self.dimensions.margin.bottom
                    - self.dimensions.border.top
                    - self.dimensions.border.bottom
                    - self.dimensions.padding.top
                    - self.dimensions.padding.bottom;
                self.dimensions.content.height = available_height.max(0.0);
            }

            // Position from top
            self.dimensions.content.y = containing_block.content.y
                + top
                + self.dimensions.margin.top
                + self.dimensions.border.top
                + self.dimensions.padding.top;
        } else if let Some(top) = self.offsets.top {
            self.dimensions.content.y = containing_block.content.y
                + top
                + self.dimensions.margin.top
                + self.dimensions.border.top
                + self.dimensions.padding.top;
        } else if let Some(bottom) = self.offsets.bottom {
            self.dimensions.content.y = containing_block.content.bottom()
                - bottom
                - self.dimensions.margin.bottom
                - self.dimensions.border.bottom
                - self.dimensions.padding.bottom
                - self.dimensions.content.height;
        }
    }

    /// Calculate block width.
    fn calculate_block_width(&mut self, containing_block: &Dimensions) {
        let style = &self.style;

        // Get values from style
        let margin_left = self.length_to_px(&style.margin_left, containing_block.content.width);
        let margin_right = self.length_to_px(&style.margin_right, containing_block.content.width);
        let border_left =
            self.length_to_px(&style.border_left_width, containing_block.content.width);
        let border_right =
            self.length_to_px(&style.border_right_width, containing_block.content.width);
        let padding_left = self.length_to_px(&style.padding_left, containing_block.content.width);
        let padding_right = self.length_to_px(&style.padding_right, containing_block.content.width);

        let total_margin_border_padding =
            margin_left + margin_right + border_left + border_right + padding_left + padding_right;

        // Calculate content width
        let content_width = match style.width {
            Length::Auto => {
                // Fill available space
                (containing_block.content.width - total_margin_border_padding).max(0.0)
            }
            _ => {
                let specified_width = self.length_to_px(&style.width, containing_block.content.width);
                // With box-sizing: border-box, the specified width includes padding and border
                if style.box_sizing == BoxSizing::BorderBox {
                    (specified_width - padding_left - padding_right - border_left - border_right).max(0.0)
                } else {
                    specified_width
                }
            }
        };

        // Apply min-width constraint (also respects box-sizing)
        let min_width_raw = self.length_to_px(&style.min_width, containing_block.content.width);
        let min_width = if style.box_sizing == BoxSizing::BorderBox && min_width_raw > 0.0 {
            (min_width_raw - padding_left - padding_right - border_left - border_right).max(0.0)
        } else {
            min_width_raw
        };
        let content_width = content_width.max(min_width);

        // Apply max-width constraint (also respects box-sizing)
        let max_width = match style.max_width {
            Length::Auto | Length::Zero => f32::INFINITY,
            _ => {
                let max_width_raw = self.length_to_px(&style.max_width, containing_block.content.width);
                if style.box_sizing == BoxSizing::BorderBox {
                    (max_width_raw - padding_left - padding_right - border_left - border_right).max(0.0)
                } else {
                    max_width_raw
                }
            }
        };
        let content_width = content_width.min(max_width);

        self.dimensions.content.width = content_width;
        self.dimensions.margin.left = margin_left;
        self.dimensions.margin.right = margin_right;
        self.dimensions.border.left = border_left;
        self.dimensions.border.right = border_right;
        self.dimensions.padding.left = padding_left;
        self.dimensions.padding.right = padding_right;
    }

    /// Calculate block position.
    fn calculate_block_position(&mut self, containing_block: &Dimensions) {
        let style = &self.style;

        self.dimensions.margin.top =
            self.length_to_px(&style.margin_top, containing_block.content.width);
        self.dimensions.margin.bottom =
            self.length_to_px(&style.margin_bottom, containing_block.content.width);
        self.dimensions.border.top =
            self.length_to_px(&style.border_top_width, containing_block.content.width);
        self.dimensions.border.bottom =
            self.length_to_px(&style.border_bottom_width, containing_block.content.width);
        self.dimensions.padding.top =
            self.length_to_px(&style.padding_top, containing_block.content.width);
        self.dimensions.padding.bottom =
            self.length_to_px(&style.padding_bottom, containing_block.content.width);

        // Position below the containing block's content
        self.dimensions.content.x = containing_block.content.x
            + self.dimensions.margin.left
            + self.dimensions.border.left
            + self.dimensions.padding.left;

        self.dimensions.content.y = containing_block.content.y
            + containing_block.content.height
            + self.dimensions.margin.top
            + self.dimensions.border.top
            + self.dimensions.padding.top;
    }

    /// Layout block children.
    fn layout_block_children(&mut self) {
        let mut cursor_y = 0.0;
        let mut cursor_x = 0.0;
        let mut line_height = 0.0_f32;
        let container_width = self.dimensions.content.width;
        let text_align = self.style.text_align;

        // Track lines for text-align adjustment after layout: (start_index, end_index, line_width)
        let mut lines: Vec<(usize, usize, f32)> = Vec::new();
        let mut line_start_index: Option<usize> = None;
        let mut line_width = 0.0_f32;

        for (i, child) in self.children.iter_mut().enumerate() {
            // Skip absolutely/fixed positioned children for flow layout
            if child.position == Position::Absolute || child.position == Position::Fixed {
                let mut cb = self.dimensions.clone();
                cb.content.height = cursor_y;
                child.layout(&cb);
                continue;
            }

            // Check if child is inline-block
            let is_inline_block = child.style.display.is_inline_block();

            if is_inline_block {
                // Layout inline-block child to get its dimensions first
                let mut cb = self.dimensions.clone();
                cb.content.x = self.dimensions.content.x + cursor_x;
                cb.content.y = self.dimensions.content.y + cursor_y;
                child.layout(&cb);

                let child_width = child.dimensions.margin_box().width;
                let child_height = child.dimensions.margin_box().height;

                // Check if child fits on current line
                if cursor_x > 0.0 && cursor_x + child_width > container_width {
                    // Record completed line for text-align
                    if let Some(start) = line_start_index {
                        lines.push((start, i, line_width));
                    }

                    // Wrap to next line
                    cursor_y += line_height;
                    cursor_x = 0.0;
                    line_height = 0.0;
                    line_start_index = Some(i);
                    line_width = 0.0;

                    // Re-layout at new position
                    cb.content.x = self.dimensions.content.x;
                    cb.content.y = self.dimensions.content.y + cursor_y;
                    child.layout(&cb);
                }

                // Track line start
                if line_start_index.is_none() {
                    line_start_index = Some(i);
                }

                // Position the child
                child.dimensions.content.x = self.dimensions.content.x + cursor_x + child.dimensions.margin.left;
                child.dimensions.content.y = self.dimensions.content.y + cursor_y + child.dimensions.margin.top;

                // Advance cursor
                cursor_x += child_width;
                line_width += child_width;
                line_height = line_height.max(child_height);

                if child.float != Float::None {
                    // Floated elements don't affect cursor
                    cursor_x -= child_width;
                    line_width -= child_width;
                }
            } else {
                // Regular block layout
                // First, finish any inline-block line
                if cursor_x > 0.0 {
                    if let Some(start) = line_start_index {
                        lines.push((start, i, line_width));
                    }
                    cursor_y += line_height;
                    cursor_x = 0.0;
                    line_height = 0.0;
                    line_start_index = None;
                    line_width = 0.0;
                }

                let mut cb = self.dimensions.clone();
                cb.content.height = cursor_y;
                child.layout(&cb);

                if child.float == Float::None {
                    cursor_y += child.dimensions.margin_box().height;
                }
            }
        }

        // Record any remaining inline-block line
        if cursor_x > 0.0 {
            if let Some(start) = line_start_index {
                lines.push((start, self.children.len(), line_width));
            }
            cursor_y += line_height;
        }

        // Apply text-align to all recorded lines
        for (start, end, width) in lines {
            Self::apply_text_align_offset(&mut self.children[start..end], width, container_width, text_align);
        }

        self.dimensions.content.height = cursor_y;
    }

    /// Apply text-align offset to inline children on a line.
    fn apply_text_align_offset(
        children: &mut [LayoutBox],
        line_width: f32,
        container_width: f32,
        text_align: TextAlign,
    ) {
        let offset = match text_align {
            TextAlign::Left => 0.0,
            TextAlign::Right => (container_width - line_width).max(0.0),
            TextAlign::Center => ((container_width - line_width) / 2.0).max(0.0),
            TextAlign::Justify => 0.0, // Justify would need gap distribution (complex)
        };

        if offset > 0.0 {
            for child in children {
                if child.style.display.is_inline_block() {
                    child.dimensions.content.x += offset;
                }
            }
        }
    }

    /// Layout block children with margin collapse.
    fn layout_block_children_with_collapse(
        &mut self,
        margin_context: &mut MarginCollapseContext,
        float_context: &mut FloatContext,
    ) {
        let mut cursor_y = 0.0;
        let mut cursor_x = 0.0;
        let mut line_height = 0.0_f32;
        let container_width = self.dimensions.content.width;
        let text_align = self.style.text_align;

        // Track lines for text-align adjustment after layout: (start_index, end_index, line_width)
        let mut lines: Vec<(usize, usize, f32)> = Vec::new();
        let mut line_start_index: Option<usize> = None;
        let mut line_width = 0.0_f32;

        for (i, child) in self.children.iter_mut().enumerate() {
            // Skip absolutely/fixed positioned children for flow layout
            if child.position == Position::Absolute || child.position == Position::Fixed {
                let mut cb = self.dimensions.clone();
                cb.content.height = cursor_y;
                child.layout_with_collapse(&cb, margin_context, float_context);
                continue;
            }

            // Check if child is inline-block
            let is_inline_block = child.style.display.is_inline_block();

            if is_inline_block {
                // Inline-block elements don't participate in margin collapse
                // Layout to get dimensions first
                let mut cb = self.dimensions.clone();
                cb.content.x = self.dimensions.content.x + cursor_x;
                cb.content.y = self.dimensions.content.y + cursor_y;
                child.layout_with_collapse(&cb, margin_context, float_context);

                let child_width = child.dimensions.margin_box().width;
                let child_height = child.dimensions.margin_box().height;

                // Check if child fits on current line
                if cursor_x > 0.0 && cursor_x + child_width > container_width {
                    // Record completed line for text-align
                    if let Some(start) = line_start_index {
                        lines.push((start, i, line_width));
                    }

                    // Wrap to next line
                    cursor_y += line_height;
                    cursor_x = 0.0;
                    line_height = 0.0;
                    line_start_index = Some(i);
                    line_width = 0.0;

                    // Re-layout at new position
                    cb.content.x = self.dimensions.content.x;
                    cb.content.y = self.dimensions.content.y + cursor_y;
                    child.layout_with_collapse(&cb, margin_context, float_context);
                }

                // Track line start
                if line_start_index.is_none() {
                    line_start_index = Some(i);
                }

                // Position the child
                child.dimensions.content.x = self.dimensions.content.x + cursor_x + child.dimensions.margin.left;
                child.dimensions.content.y = self.dimensions.content.y + cursor_y + child.dimensions.margin.top;

                // Advance cursor
                cursor_x += child_width;
                line_width += child_width;
                line_height = line_height.max(child_height);

                if child.float != Float::None {
                    cursor_x -= child_width;
                    line_width -= child_width;
                }
            } else {
                // Regular block layout with margin collapse
                // First, finish any inline-block line
                if cursor_x > 0.0 {
                    if let Some(start) = line_start_index {
                        lines.push((start, i, line_width));
                    }
                    cursor_y += line_height;
                    cursor_x = 0.0;
                    line_height = 0.0;
                    line_start_index = None;
                    line_width = 0.0;
                }

                let mut cb = self.dimensions.clone();
                cb.content.height = cursor_y;
                child.layout_with_collapse(&cb, margin_context, float_context);

                if child.float == Float::None {
                    cursor_y = child.dimensions.border_box().bottom() - self.dimensions.content.y;
                }
            }
        }

        // Record any remaining inline-block line
        if cursor_x > 0.0 {
            if let Some(start) = line_start_index {
                lines.push((start, self.children.len(), line_width));
            }
            cursor_y += line_height;
        }

        // Apply text-align to all recorded lines
        for (start, end, width) in lines {
            Self::apply_text_align_offset(&mut self.children[start..end], width, container_width, text_align);
        }

        self.dimensions.content.height = cursor_y;
    }

    /// Calculate block height.
    fn calculate_block_height(&mut self) {
        // Get padding and border for box-sizing calculations
        let padding_top = self.dimensions.padding.top;
        let padding_bottom = self.dimensions.padding.bottom;
        let border_top = self.dimensions.border.top;
        let border_bottom = self.dimensions.border.bottom;
        let padding_border_height = padding_top + padding_bottom + border_top + border_bottom;
        let is_border_box = self.style.box_sizing == BoxSizing::BorderBox;

        // If height is explicitly set, use it
        match self.style.height {
            Length::Px(h) => {
                // With box-sizing: border-box, specified height includes padding and border
                self.dimensions.content.height = if is_border_box {
                    (h - padding_border_height).max(0.0)
                } else {
                    h
                };
            }
            Length::Percent(pct) => {
                // Percent height requires a known containing block height
                // For now, use viewport height as fallback
                if self.viewport.1 > 0.0 {
                    let specified = pct / 100.0 * self.viewport.1;
                    self.dimensions.content.height = if is_border_box {
                        (specified - padding_border_height).max(0.0)
                    } else {
                        specified
                    };
                }
            }
            Length::Vh(vh) => {
                if self.viewport.1 > 0.0 {
                    let specified = vh / 100.0 * self.viewport.1;
                    self.dimensions.content.height = if is_border_box {
                        (specified - padding_border_height).max(0.0)
                    } else {
                        specified
                    };
                }
            }
            Length::Em(em) => {
                let font_size = match self.style.font_size {
                    Length::Px(px) => px,
                    _ => 16.0,
                };
                let specified = em * font_size;
                self.dimensions.content.height = if is_border_box {
                    (specified - padding_border_height).max(0.0)
                } else {
                    specified
                };
            }
            Length::Rem(rem) => {
                let specified = rem * 16.0; // Root font size
                self.dimensions.content.height = if is_border_box {
                    (specified - padding_border_height).max(0.0)
                } else {
                    specified
                };
            }
            _ => {
                // Auto or Zero - content.height was set by layout_block_children
                // But if aspect-ratio is set and we have a width, calculate height from it
                if let Some(ratio) = self.style.aspect_ratio {
                    if self.dimensions.content.width > 0.0 && ratio > 0.0 {
                        self.dimensions.content.height = self.dimensions.content.width / ratio;
                    }
                }
            }
        }

        // Apply min-height constraint (also respects box-sizing)
        let min_height_raw = match self.style.min_height {
            Length::Px(px) => px,
            Length::Vh(vh) => vh / 100.0 * self.viewport.1,
            Length::Percent(pct) => pct / 100.0 * self.viewport.1,
            _ => 0.0,
        };
        let min_height = if is_border_box && min_height_raw > 0.0 {
            (min_height_raw - padding_border_height).max(0.0)
        } else {
            min_height_raw
        };
        if self.dimensions.content.height < min_height {
            self.dimensions.content.height = min_height;
        }

        // Apply max-height constraint (also respects box-sizing)
        let max_height_raw = match self.style.max_height {
            Length::Px(px) => px,
            Length::Vh(vh) => vh / 100.0 * self.viewport.1,
            Length::Percent(pct) => pct / 100.0 * self.viewport.1,
            _ => f32::INFINITY,
        };
        let max_height = if is_border_box && max_height_raw < f32::INFINITY {
            (max_height_raw - padding_border_height).max(0.0)
        } else {
            max_height_raw
        };
        if self.dimensions.content.height > max_height {
            self.dimensions.content.height = max_height;
        }
    }

    /// Convert a Length to pixels.
    fn length_to_px(&self, length: &Length, container_size: f32) -> f32 {
        let font_size = match &self.style.font_size {
            Length::Px(px) => *px,
            _ => 16.0,
        };
        length.to_px_with_viewport(font_size, 16.0, container_size, self.viewport.0, self.viewport.1)
    }
    
    /// Set viewport dimensions for this box and all children.
    pub fn set_viewport(&mut self, width: f32, height: f32) {
        self.viewport = (width, height);
        for child in &mut self.children {
            child.set_viewport(width, height);
        }
    }

    /// Get children sorted by z-index for painting.
    pub fn get_paint_order(&self) -> Vec<&LayoutBox> {
        let mut normal_flow: Vec<&LayoutBox> = Vec::new();
        let mut positioned: Vec<(&LayoutBox, i32)> = Vec::new();

        for child in &self.children {
            if child.position == Position::Static {
                normal_flow.push(child);
            } else {
                positioned.push((child, child.z_index));
            }
        }

        // Sort positioned elements by z-index
        positioned.sort_by(|a, b| a.1.cmp(&b.1));

        // Combine: negative z-index, normal flow, positive z-index
        let mut result: Vec<&LayoutBox> = Vec::new();

        // Add negative z-index positioned elements first
        for (child, z) in positioned.iter() {
            if *z < 0 {
                result.push(child);
            }
        }

        // Add normal flow elements
        result.extend(normal_flow);

        // Add zero and positive z-index positioned elements
        for (child, z) in positioned.iter() {
            if *z >= 0 {
                result.push(child);
            }
        }

        result
    }

    /// Perform hit testing at the given point.
    /// Returns the hit test result with information about the element at the point.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<HitTestResult> {
        self.hit_test_internal(x, y, 0)
    }

    /// Internal hit test that tracks depth.
    fn hit_test_internal(&self, x: f32, y: f32, depth: u32) -> Option<HitTestResult> {
        // Get the border box for this element
        let border_box = self.dimensions.border_box();

        // Check if the point is within our border box
        if !border_box.contains(x, y) {
            return None;
        }

        // Check children in reverse paint order (topmost first)
        let paint_order = self.get_paint_order();
        for child in paint_order.iter().rev() {
            if let Some(mut result) = child.hit_test_internal(x, y, depth + 1) {
                // Found a hit in a child - add ourselves to the path
                result.ancestors.push(HitTestAncestor {
                    box_type: self.box_type.clone(),
                    border_box: self.dimensions.border_box(),
                    content_box: self.dimensions.content,
                    z_index: self.z_index,
                    position: self.position,
                });
                return Some(result);
            }
        }

        // No child was hit, so we are the target
        Some(HitTestResult {
            box_type: self.box_type.clone(),
            border_box,
            content_box: self.dimensions.content,
            padding_box: self.dimensions.padding_box(),
            local_x: x - border_box.x,
            local_y: y - border_box.y,
            depth,
            ancestors: Vec::new(),
            z_index: self.z_index,
            position: self.position,
            is_scrollable: false, // TODO: detect overflow
        })
    }

    /// Check if a point is within the border box.
    pub fn contains_point(&self, x: f32, y: f32) -> bool {
        self.dimensions.border_box().contains(x, y)
    }

    /// Get all elements at a point (including overlapping elements).
    pub fn hit_test_all(&self, x: f32, y: f32) -> Vec<HitTestResult> {
        let mut results = Vec::new();
        self.hit_test_all_internal(x, y, 0, &mut results);
        results
    }

    /// Internal hit test that collects all results.
    fn hit_test_all_internal(&self, x: f32, y: f32, depth: u32, results: &mut Vec<HitTestResult>) {
        let border_box = self.dimensions.border_box();

        if !border_box.contains(x, y) {
            return;
        }

        // Add this element
        results.push(HitTestResult {
            box_type: self.box_type.clone(),
            border_box,
            content_box: self.dimensions.content,
            padding_box: self.dimensions.padding_box(),
            local_x: x - border_box.x,
            local_y: y - border_box.y,
            depth,
            ancestors: Vec::new(),
            z_index: self.z_index,
            position: self.position,
            is_scrollable: false,
        });

        // Check all children
        for child in &self.children {
            child.hit_test_all_internal(x, y, depth + 1, results);
        }
    }
}

/// Result of a hit test operation.
#[derive(Debug, Clone)]
pub struct HitTestResult {
    /// The type of the hit box.
    pub box_type: BoxType,
    /// The border box of the hit element.
    pub border_box: Rect,
    /// The content box of the hit element.
    pub content_box: Rect,
    /// The padding box of the hit element.
    pub padding_box: Rect,
    /// X coordinate relative to the border box.
    pub local_x: f32,
    /// Y coordinate relative to the border box.
    pub local_y: f32,
    /// Depth in the layout tree (0 = root).
    pub depth: u32,
    /// Ancestor chain from parent to root.
    pub ancestors: Vec<HitTestAncestor>,
    /// Z-index of the hit element.
    pub z_index: i32,
    /// Position property of the hit element.
    pub position: Position,
    /// Whether the element is scrollable.
    pub is_scrollable: bool,
}

impl HitTestResult {
    /// Check if the hit was in the content area.
    pub fn is_in_content(&self) -> bool {
        self.content_box.contains(
            self.border_box.x + self.local_x,
            self.border_box.y + self.local_y,
        )
    }

    /// Check if the hit was in the padding area.
    pub fn is_in_padding(&self) -> bool {
        let abs_x = self.border_box.x + self.local_x;
        let abs_y = self.border_box.y + self.local_y;
        self.padding_box.contains(abs_x, abs_y) && !self.content_box.contains(abs_x, abs_y)
    }

    /// Check if the hit was in the border area.
    pub fn is_in_border(&self) -> bool {
        let abs_x = self.border_box.x + self.local_x;
        let abs_y = self.border_box.y + self.local_y;
        self.border_box.contains(abs_x, abs_y) && !self.padding_box.contains(abs_x, abs_y)
    }
}

/// Information about an ancestor in the hit test path.
#[derive(Debug, Clone)]
pub struct HitTestAncestor {
    /// Box type.
    pub box_type: BoxType,
    /// Border box.
    pub border_box: Rect,
    /// Content box.
    pub content_box: Rect,
    /// Z-index.
    pub z_index: i32,
    /// Position property.
    pub position: Position,
}

/// Border radius values for each corner.
#[derive(Debug, Clone, Copy, Default)]
pub struct BorderRadius {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl BorderRadius {
    /// Create uniform border radius.
    pub fn uniform(radius: f32) -> Self {
        Self {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        }
    }
    
    /// Check if all radii are zero (no rounding).
    pub fn is_zero(&self) -> bool {
        self.top_left == 0.0 && self.top_right == 0.0 
            && self.bottom_right == 0.0 && self.bottom_left == 0.0
    }
}

/// A paint command for rendering.
#[derive(Debug, Clone)]
pub enum DisplayCommand {
    /// Fill a rectangle with a solid color.
    SolidColor(Color, Rect),
    /// Fill a rounded rectangle with a solid color.
    RoundedRect {
        color: Color,
        rect: Rect,
        radius: BorderRadius,
    },
    /// Draw a border.
    Border {
        color: Color,
        rect: Rect,
        top: f32,
        right: f32,
        bottom: f32,
        left: f32,
    },
    /// Draw text.
    Text {
        text: String,
        x: f32,
        y: f32,
        color: Color,
        font_size: f32,
        font_family: String,
        font_weight: u16,
        font_style: u8,
    },
    /// Draw text decoration line (underline, strikethrough, overline).
    TextDecoration {
        x: f32,
        y: f32,
        width: f32,
        thickness: f32,
        color: Color,
        style: TextDecorationStyleValue,
    },
    /// Draw an image.
    Image {
        /// URL or cache key of the image
        url: String,
        /// Source rectangle in the image (for sprites or cropping)
        src_rect: Option<Rect>,
        /// Destination rectangle on screen
        dest_rect: Rect,
        /// Object-fit mode
        object_fit: ObjectFit,
        /// Opacity (0.0 - 1.0)
        opacity: f32,
    },
    /// Draw a background image.
    BackgroundImage {
        /// URL or cache key of the image
        url: String,
        /// Destination rectangle
        rect: Rect,
        /// Background size
        size: BackgroundSize,
        /// Background position (0-1 range)
        position: (f32, f32),
        /// Background repeat
        repeat: BackgroundRepeat,
    },
    /// Draw a box shadow.
    BoxShadow {
        /// Shadow offset X
        offset_x: f32,
        /// Shadow offset Y
        offset_y: f32,
        /// Blur radius
        blur_radius: f32,
        /// Spread radius
        spread_radius: f32,
        /// Shadow color
        color: Color,
        /// Box rectangle (shadow is drawn outside this box, or inside if inset)
        rect: Rect,
        /// Whether this is an inset shadow
        inset: bool,
    },
    /// Apply a backdrop filter (blur, grayscale, etc.) to the pixels behind this rectangle.
    BackdropFilter {
        /// The rectangle to apply the filter to.
        rect: Rect,
        /// Border radius for clipping.
        border_radius: BorderRadius,
        /// The filter to apply.
        filter: rustkit_css::BackdropFilter,
    },
    /// Draw a linear gradient.
    LinearGradient {
        rect: Rect,
        direction: rustkit_css::GradientDirection,
        stops: Vec<rustkit_css::ColorStop>,
        repeating: bool,
        border_radius: BorderRadius,
    },
    /// Draw a radial gradient.
    RadialGradient {
        rect: Rect,
        shape: rustkit_css::RadialShape,
        size: rustkit_css::RadialSize,
        center: (f32, f32),
        stops: Vec<rustkit_css::ColorStop>,
        repeating: bool,
        border_radius: BorderRadius,
    },
    /// Draw a conic gradient.
    ConicGradient {
        rect: Rect,
        from_angle: f32,
        center: (f32, f32),
        stops: Vec<rustkit_css::ColorStop>,
        repeating: bool,
        border_radius: BorderRadius,
    },
    /// Draw a text input field.
    TextInput {
        rect: Rect,
        value: String,
        placeholder: String,
        font_size: f32,
        text_color: Color,
        placeholder_color: Color,
        background_color: Color,
        border_color: Color,
        border_width: f32,
        focused: bool,
        caret_position: Option<usize>,
    },
    /// Draw a button.
    Button {
        rect: Rect,
        label: String,
        font_size: f32,
        text_color: Color,
        background_color: Color,
        border_color: Color,
        border_width: f32,
        border_radius: f32,
        pressed: bool,
        focused: bool,
    },
    /// Draw a focus ring around an element.
    FocusRing {
        rect: Rect,
        color: Color,
        width: f32,
        offset: f32,
    },
    /// Draw a text caret (cursor).
    Caret {
        x: f32,
        y: f32,
        height: f32,
        color: Color,
    },
    /// Push a clip rect (for overflow handling).
    PushClip(Rect),
    /// Pop clip rect.
    PopClip,
    /// Start stacking context.
    PushStackingContext { z_index: i32, rect: Rect },
    /// End stacking context.
    PopStackingContext,
    /// Push a 2D transform matrix.
    /// The matrix is [a, b, c, d, e, f] representing:
    /// | a c e |
    /// | b d f |
    /// | 0 0 1 |
    /// Origin is the point around which the transform is applied.
    PushTransform {
        matrix: [f32; 6],
        origin: (f32, f32),
    },
    /// Pop a transform matrix.
    PopTransform,

    /// Draw text with a gradient fill (for background-clip: text effect).
    GradientText {
        text: String,
        x: f32,
        y: f32,
        font_size: f32,
        font_family: String,
        font_weight: u16,
        font_style: u8,
        gradient: rustkit_css::Gradient,
        rect: Rect,
    },

    // SVG-specific commands
    /// Fill a rectangle with solid color.
    FillRect { rect: Rect, color: Color },
    /// Stroke a rectangle.
    StrokeRect { rect: Rect, color: Color, width: f32 },
    /// Fill a circle.
    FillCircle { cx: f32, cy: f32, radius: f32, color: Color },
    /// Stroke a circle.
    StrokeCircle { cx: f32, cy: f32, radius: f32, color: Color, width: f32 },
    /// Fill an ellipse.
    FillEllipse { rect: Rect, color: Color },
    /// Draw a line.
    Line { x1: f32, y1: f32, x2: f32, y2: f32, color: Color, width: f32 },
    /// Draw a polyline (connected line segments).
    Polyline { points: Vec<(f32, f32)>, color: Color, width: f32 },
    /// Fill a polygon.
    FillPolygon { points: Vec<(f32, f32)>, color: Color },
    /// Stroke a polygon.
    StrokePolygon { points: Vec<(f32, f32)>, color: Color, width: f32 },
}

/// Text decoration style for display commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextDecorationStyleValue {
    Solid,
    Double,
    Dotted,
    Dashed,
    Wavy,
}

/// CSS object-fit values for images.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ObjectFit {
    /// Fill the box, possibly distorting the image
    Fill,
    /// Scale to fit inside the box, preserving aspect ratio
    #[default]
    Contain,
    /// Scale to cover the box, preserving aspect ratio
    Cover,
    /// Don't scale the image
    None,
    /// Like contain but never scale up
    ScaleDown,
}

impl ObjectFit {
    /// Parse from CSS value
    pub fn from_css(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "fill" => ObjectFit::Fill,
            "contain" => ObjectFit::Contain,
            "cover" => ObjectFit::Cover,
            "none" => ObjectFit::None,
            "scale-down" => ObjectFit::ScaleDown,
            _ => ObjectFit::default(),
        }
    }

    /// Calculate the image rectangle within a container
    pub fn compute_rect(
        &self,
        container: Rect,
        image_width: f32,
        image_height: f32,
        position: (f32, f32),
    ) -> ImageDrawRect {
        if image_width == 0.0 || image_height == 0.0 {
            return ImageDrawRect {
                dest: container,
                src: None,
            };
        }

        let image_aspect = image_width / image_height;
        let container_aspect = container.width / container.height;

        let (draw_width, draw_height) = match self {
            ObjectFit::Fill => (container.width, container.height),

            ObjectFit::Contain => {
                if image_aspect > container_aspect {
                    (container.width, container.width / image_aspect)
                } else {
                    (container.height * image_aspect, container.height)
                }
            }

            ObjectFit::Cover => {
                if image_aspect > container_aspect {
                    (container.height * image_aspect, container.height)
                } else {
                    (container.width, container.width / image_aspect)
                }
            }

            ObjectFit::None => (image_width, image_height),

            ObjectFit::ScaleDown => {
                if image_width <= container.width && image_height <= container.height {
                    (image_width, image_height)
                } else if image_aspect > container_aspect {
                    (container.width, container.width / image_aspect)
                } else {
                    (container.height * image_aspect, container.height)
                }
            }
        };

        let x = container.x + (container.width - draw_width) * position.0;
        let y = container.y + (container.height - draw_height) * position.1;

        ImageDrawRect {
            dest: Rect {
                x,
                y,
                width: draw_width,
                height: draw_height,
            },
            src: None,
        }
    }
}

/// Result of computing image draw rectangle
#[derive(Debug, Clone)]
pub struct ImageDrawRect {
    /// Destination rectangle
    pub dest: Rect,
    /// Source rectangle (for cropping, e.g., in cover mode)
    pub src: Option<Rect>,
}

/// CSS background-size values.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum BackgroundSize {
    /// Stretch to fill
    Cover,
    /// Scale to fit
    Contain,
    /// Explicit size
    Explicit { width: Option<f32>, height: Option<f32> },
    /// Auto sizing
    #[default]
    Auto,
}

impl BackgroundSize {
    /// Parse from CSS value
    pub fn from_css(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "cover" => BackgroundSize::Cover,
            "contain" => BackgroundSize::Contain,
            "auto" => BackgroundSize::Auto,
            _ => {
                // Try to parse explicit size
                let parts: Vec<&str> = value.split_whitespace().collect();
                let width = parts.first().and_then(|s| parse_length(s));
                let height = parts.get(1).and_then(|s| parse_length(s));
                BackgroundSize::Explicit { width, height }
            }
        }
    }

    /// Calculate the background image size
    pub fn compute_size(
        &self,
        container: Rect,
        image_width: f32,
        image_height: f32,
    ) -> (f32, f32) {
        if image_width == 0.0 || image_height == 0.0 {
            return (0.0, 0.0);
        }

        let image_aspect = image_width / image_height;
        let container_aspect = container.width / container.height;

        match self {
            BackgroundSize::Cover => {
                if image_aspect > container_aspect {
                    (container.height * image_aspect, container.height)
                } else {
                    (container.width, container.width / image_aspect)
                }
            }

            BackgroundSize::Contain => {
                if image_aspect > container_aspect {
                    (container.width, container.width / image_aspect)
                } else {
                    (container.height * image_aspect, container.height)
                }
            }

            BackgroundSize::Auto => (image_width, image_height),

            BackgroundSize::Explicit { width, height } => {
                match (width, height) {
                    (Some(w), Some(h)) => (*w, *h),
                    (Some(w), None) => (*w, *w / image_aspect),
                    (None, Some(h)) => (*h * image_aspect, *h),
                    (None, None) => (image_width, image_height),
                }
            }
        }
    }
}

/// CSS background-repeat values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackgroundRepeat {
    /// Repeat in both directions
    #[default]
    Repeat,
    /// Repeat horizontally only
    RepeatX,
    /// Repeat vertically only
    RepeatY,
    /// No repeat
    NoRepeat,
    /// Space evenly
    Space,
    /// Round to fill
    Round,
}

impl BackgroundRepeat {
    /// Parse from CSS value
    pub fn from_css(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "repeat" => BackgroundRepeat::Repeat,
            "repeat-x" => BackgroundRepeat::RepeatX,
            "repeat-y" => BackgroundRepeat::RepeatY,
            "no-repeat" => BackgroundRepeat::NoRepeat,
            "space" => BackgroundRepeat::Space,
            "round" => BackgroundRepeat::Round,
            _ => BackgroundRepeat::default(),
        }
    }

    /// Check if repeating on x-axis
    pub fn repeats_x(&self) -> bool {
        matches!(self, BackgroundRepeat::Repeat | BackgroundRepeat::RepeatX | BackgroundRepeat::Space | BackgroundRepeat::Round)
    }

    /// Check if repeating on y-axis
    pub fn repeats_y(&self) -> bool {
        matches!(self, BackgroundRepeat::Repeat | BackgroundRepeat::RepeatY | BackgroundRepeat::Space | BackgroundRepeat::Round)
    }
}

/// Parse a CSS length value to pixels
fn parse_length(value: &str) -> Option<f32> {
    let value = value.trim();
    if value.ends_with("px") {
        value.trim_end_matches("px").parse().ok()
    } else if value.ends_with('%') {
        // Percentages would need container size - return None for now
        None
    } else {
        value.parse().ok()
    }
}

/// A paint item with z-index for sorting.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct PaintItem {
    z_index: i32,
    layer: u32, // For stable sort within same z-index
    commands: Vec<DisplayCommand>,
}

#[allow(dead_code)]
impl PaintItem {
    fn new(z_index: i32, layer: u32) -> Self {
        Self {
            z_index,
            layer,
            commands: Vec::new(),
        }
    }
}

/// A display list of paint commands.
#[derive(Debug, Default, Clone)]
pub struct DisplayList {
    pub commands: Vec<DisplayCommand>,
}

impl DisplayList {
    /// Create an empty display list.
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    /// Build display list from a layout box with proper stacking order.
    pub fn build(root: &LayoutBox) -> Self {
        let mut list = DisplayList::new();
        list.render_stacking_context(root, 0, &mut 0);
        list
    }

    /// Build display list from a layout box with scroll state applied.
    ///
    /// This updates sticky positions based on the scroll state before building
    /// the display list. Use this when the scroll position has changed and
    /// sticky elements need to be repositioned.
    pub fn build_with_scroll(
        root: &mut LayoutBox,
        scroll_x: f32,
        scroll_y: f32,
        viewport: Rect,
    ) -> Self {
        // Update sticky positions based on scroll
        root.update_sticky_positions(scroll_x, scroll_y, viewport);

        // Build the display list
        let mut list = DisplayList::new();
        list.render_stacking_context(root, 0, &mut 0);
        list
    }

    /// Render a stacking context with proper z-ordering.
    fn render_stacking_context(&mut self, layout_box: &LayoutBox, parent_z: i32, layer: &mut u32) {
        let z_index = if layout_box.position != Position::Static {
            layout_box.z_index
        } else {
            parent_z
        };

        // Check if this creates a new stacking context
        let creates_context = layout_box
            .stacking_context
            .as_ref()
            .map(|ctx| ctx.creates_context)
            .unwrap_or(false);

        if creates_context {
            self.commands.push(DisplayCommand::PushStackingContext {
                z_index,
                rect: layout_box.dimensions.border_box(),
            });
        }

        // Check if this box has a transform
        let has_transform = !layout_box.style.transform.is_identity();
        if has_transform {
            let border_box = layout_box.dimensions.border_box();
            // Compute transform matrix
            let matrix = layout_box.style.transform.to_matrix(border_box.width, border_box.height);
            // Compute origin in absolute coordinates
            let origin_x = border_box.x + layout_box.style.transform_origin.x.to_px(16.0, 16.0, border_box.width);
            let origin_y = border_box.y + layout_box.style.transform_origin.y.to_px(16.0, 16.0, border_box.height);
            self.commands.push(DisplayCommand::PushTransform {
                matrix,
                origin: (origin_x, origin_y),
            });
        }

        // Render this box
        self.render_box_content(layout_box);

        // Collect children grouped by paint order
        let mut negative_z: Vec<(&LayoutBox, u32)> = Vec::new();
        let mut normal_flow: Vec<(&LayoutBox, u32)> = Vec::new();
        let mut positive_z: Vec<(&LayoutBox, u32)> = Vec::new();

        for child in &layout_box.children {
            *layer += 1;
            let child_layer = *layer;

            if child.position != Position::Static {
                if child.z_index < 0 {
                    negative_z.push((child, child_layer));
                } else {
                    positive_z.push((child, child_layer));
                }
            } else if child.float != Float::None {
                // Floats paint between normal flow and positioned
                positive_z.push((child, child_layer));
            } else {
                normal_flow.push((child, child_layer));
            }
        }

        // Sort by z-index, then by layer for stability
        negative_z.sort_by(|a, b| {
            let z_cmp = a.0.z_index.cmp(&b.0.z_index);
            if z_cmp == Ordering::Equal {
                a.1.cmp(&b.1)
            } else {
                z_cmp
            }
        });
        positive_z.sort_by(|a, b| {
            let z_cmp = a.0.z_index.cmp(&b.0.z_index);
            if z_cmp == Ordering::Equal {
                a.1.cmp(&b.1)
            } else {
                z_cmp
            }
        });

        // Render in correct order:
        // 1. Negative z-index positioned descendants
        for (child, _) in negative_z {
            self.render_stacking_context(child, z_index, layer);
        }

        // 2. Normal flow block children
        for (child, _) in &normal_flow {
            self.render_stacking_context(child, z_index, layer);
        }

        // 3. Floats and positive/zero z-index positioned descendants
        for (child, _) in positive_z {
            self.render_stacking_context(child, z_index, layer);
        }

        // Pop transform if we pushed one
        if has_transform {
            self.commands.push(DisplayCommand::PopTransform);
        }

        if creates_context {
            self.commands.push(DisplayCommand::PopStackingContext);
        }
    }

    /// Render a layout box's own content (shadows, background, borders, text, images).
    fn render_box_content(&mut self, layout_box: &LayoutBox) {
        // Box shadows (outer) are drawn first, behind the element
        self.render_box_shadows(layout_box);
        // Then background
        self.render_background(layout_box);
        // Then inset shadows (on top of background, inside the box)
        self.render_inset_shadows(layout_box);
        // Then borders
        self.render_borders(layout_box);
        // Then text
        self.render_text(layout_box);
        // Then images (replaced content)
        self.render_replaced_content(layout_box);
    }

    /// Render a layout box and its children (legacy method).
    #[allow(dead_code)]
    fn render_box(&mut self, layout_box: &LayoutBox) {
        // Box shadows (outer) are drawn first, behind the element
        self.render_box_shadows(layout_box);
        // Then background
        self.render_background(layout_box);
        // Then inset shadows (on top of background, inside the box)
        self.render_inset_shadows(layout_box);
        // Then borders
        self.render_borders(layout_box);
        // Then text
        self.render_text(layout_box);
        // Then images (replaced content)
        self.render_replaced_content(layout_box);

        for child in &layout_box.children {
            self.render_box(child);
        }
    }

    /// Render box shadows (must be called before background).
    fn render_box_shadows(&mut self, layout_box: &LayoutBox) {
        let box_rect = layout_box.dimensions.border_box();
        
        // Render outer shadows first (in order, first shadow is top-most)
        for shadow in &layout_box.style.box_shadows {
            if shadow.is_visible() && !shadow.inset {
                self.commands.push(DisplayCommand::BoxShadow {
                    offset_x: shadow.offset_x,
                    offset_y: shadow.offset_y,
                    blur_radius: shadow.blur_radius,
                    spread_radius: shadow.spread_radius,
                    color: shadow.color,
                    rect: box_rect,
                    inset: false,
                });
            }
        }
    }
    
    /// Render inset box shadows (called after background).
    fn render_inset_shadows(&mut self, layout_box: &LayoutBox) {
        let box_rect = layout_box.dimensions.border_box();
        
        for shadow in &layout_box.style.box_shadows {
            if shadow.is_visible() && shadow.inset {
                self.commands.push(DisplayCommand::BoxShadow {
                    offset_x: shadow.offset_x,
                    offset_y: shadow.offset_y,
                    blur_radius: shadow.blur_radius,
                    spread_radius: shadow.spread_radius,
                    color: shadow.color,
                    rect: box_rect,
                    inset: true,
                });
            }
        }
    }
    
    /// Render background.
    /// Supports multiple background layers painted bottom-to-top.
    /// Respects background-clip property (border-box, padding-box, content-box).
    fn render_background(&mut self, layout_box: &LayoutBox) {
        let d = &layout_box.dimensions;
        let border_rect = d.border_box();
        let s = &layout_box.style;

        // Get font size for relative length calculations
        let font_size = match s.font_size {
            Length::Px(px) => px,
            _ => 16.0,
        };
        let root_font_size = 16.0; // TODO: Pass actual root font size

        // Calculate border radius once (used for both solid color and gradient clipping)
        let radius = BorderRadius {
            top_left: s.border_top_left_radius.to_px(font_size, root_font_size, border_rect.width),
            top_right: s.border_top_right_radius.to_px(font_size, root_font_size, border_rect.width),
            bottom_right: s.border_bottom_right_radius.to_px(font_size, root_font_size, border_rect.width),
            bottom_left: s.border_bottom_left_radius.to_px(font_size, root_font_size, border_rect.width),
        };

        // Calculate the clipped rect based on background-clip property
        let clip_rect = match s.background_clip {
            rustkit_css::BackgroundClip::BorderBox => border_rect,
            rustkit_css::BackgroundClip::PaddingBox => {
                // Clip to padding box (inside borders)
                Rect::new(
                    border_rect.x + d.border.left,
                    border_rect.y + d.border.top,
                    border_rect.width - d.border.left - d.border.right,
                    border_rect.height - d.border.top - d.border.bottom,
                )
            }
            rustkit_css::BackgroundClip::ContentBox => {
                // Clip to content box (inside padding)
                Rect::new(
                    border_rect.x + d.border.left + d.padding.left,
                    border_rect.y + d.border.top + d.padding.top,
                    border_rect.width - d.border.left - d.border.right - d.padding.left - d.padding.right,
                    border_rect.height - d.border.top - d.border.bottom - d.padding.top - d.padding.bottom,
                )
            }
            rustkit_css::BackgroundClip::Text => {
                // Text clipping is handled separately in gradient text rendering
                border_rect
            }
        };

        // Only proceed if clip_rect has positive dimensions
        if clip_rect.width <= 0.0 || clip_rect.height <= 0.0 {
            return;
        }

        // Apply clipping if not border-box (the default)
        let needs_clip = !matches!(s.background_clip, rustkit_css::BackgroundClip::BorderBox);
        if needs_clip {
            self.commands.push(DisplayCommand::PushClip(clip_rect));
        }

        // Use clip_rect for painting (not border_rect) to ensure proper bounds
        let paint_rect = clip_rect;

        // Step 0: Apply backdrop-filter if set (must be done BEFORE any background painting)
        // Backdrop filter applies to pixels that are already rendered behind this element
        if !s.backdrop_filter.is_none() {
            self.commands.push(DisplayCommand::BackdropFilter {
                rect: paint_rect,
                border_radius: radius,
                filter: s.backdrop_filter,
            });
        }

        // Step 1: Paint solid background color FIRST (bottom layer)
        // This must be painted even if there's a gradient, as the gradient may be semi-transparent
        let color = s.background_color;
        if color.a > 0.0 {
            if radius.is_zero() || needs_clip {
                // When clipping, use solid rect within the clipped area
                self.commands.push(DisplayCommand::SolidColor(color, paint_rect));
            } else {
                self.commands.push(DisplayCommand::RoundedRect { color, rect: paint_rect, radius });
            }
        }

        // Step 2: Paint background layers (bottom-to-top, index 0 is bottommost)
        // This is the new multi-layer background support
        if !s.background_layers.is_empty() {
            for layer in &s.background_layers {
                self.render_background_layer(layer, paint_rect, font_size, root_font_size, radius);
            }
        } else if let Some(gradient) = &s.background_gradient {
            // Fallback to legacy single gradient for backwards compatibility
            self.render_gradient(gradient, paint_rect, radius);
        }

        // Pop the clip if we pushed one
        if needs_clip {
            self.commands.push(DisplayCommand::PopClip);
        }
    }

    /// Render a single background layer.
    fn render_background_layer(
        &mut self,
        layer: &rustkit_css::BackgroundLayer,
        container: Rect,
        _font_size: f32,
        _root_font_size: f32,
        border_radius: BorderRadius,
    ) {
        match &layer.image {
            rustkit_css::BackgroundImage::None => {
                // No image, nothing to render
            }
            rustkit_css::BackgroundImage::Gradient(gradient) => {
                // Calculate the positioned rect for this gradient based on size/position
                let positioned_rect = self.calculate_background_rect(
                    container,
                    &layer.size,
                    &layer.position,
                    container.width, // For gradients, use container size as "intrinsic" size
                    container.height,
                );
                self.render_gradient(gradient, positioned_rect, border_radius);
            }
            rustkit_css::BackgroundImage::Url(url) => {
                // For URL backgrounds, emit a BackgroundImage command
                // The actual image dimensions would come from the image cache
                // For now, use container size as fallback
                let size = self.convert_background_size(&layer.size);
                let position = self.convert_background_position(&layer.position);
                let repeat = self.convert_background_repeat(layer.repeat);

                self.commands.push(DisplayCommand::BackgroundImage {
                    url: url.clone(),
                    rect: container,
                    size,
                    position,
                    repeat,
                });
            }
        }
    }

    /// Calculate the rect for a background image/gradient based on size and position.
    fn calculate_background_rect(
        &self,
        container: Rect,
        size: &rustkit_css::BackgroundSize,
        position: &rustkit_css::BackgroundPosition,
        intrinsic_width: f32,
        intrinsic_height: f32,
    ) -> Rect {
        // Calculate the background size
        let (bg_width, bg_height) = match size {
            rustkit_css::BackgroundSize::Auto => (intrinsic_width, intrinsic_height),
            rustkit_css::BackgroundSize::Cover => {
                let scale_x = container.width / intrinsic_width;
                let scale_y = container.height / intrinsic_height;
                let scale = scale_x.max(scale_y);
                (intrinsic_width * scale, intrinsic_height * scale)
            }
            rustkit_css::BackgroundSize::Contain => {
                let scale_x = container.width / intrinsic_width;
                let scale_y = container.height / intrinsic_height;
                let scale = scale_x.min(scale_y);
                (intrinsic_width * scale, intrinsic_height * scale)
            }
            rustkit_css::BackgroundSize::Explicit { width, height } => {
                let w = width.map(|v| if v < 0.0 { container.width * (-v / 100.0) } else { v })
                    .unwrap_or(intrinsic_width);
                let h = height.map(|v| if v < 0.0 { container.height * (-v / 100.0) } else { v })
                    .unwrap_or(intrinsic_height);
                (w, h)
            }
        };

        // Calculate position
        let x = container.x + position.x.to_px(container.width, bg_width);
        let y = container.y + position.y.to_px(container.height, bg_height);

        Rect::new(x, y, bg_width, bg_height)
    }

    /// Render a gradient to a rect with optional border-radius clipping.
    fn render_gradient(&mut self, gradient: &rustkit_css::Gradient, rect: Rect, border_radius: BorderRadius) {
        match gradient {
            rustkit_css::Gradient::Linear(linear) => {
                self.commands.push(DisplayCommand::LinearGradient {
                    rect,
                    direction: linear.direction,
                    stops: linear.stops.clone(),
                    repeating: linear.repeating,
                    border_radius,
                });
            }
            rustkit_css::Gradient::Radial(radial) => {
                self.commands.push(DisplayCommand::RadialGradient {
                    rect,
                    shape: radial.shape,
                    size: radial.size,
                    center: radial.center,
                    stops: radial.stops.clone(),
                    repeating: radial.repeating,
                    border_radius,
                });
            }
            rustkit_css::Gradient::Conic(conic) => {
                self.commands.push(DisplayCommand::ConicGradient {
                    rect,
                    from_angle: conic.from_angle,
                    center: conic.center,
                    stops: conic.stops.clone(),
                    repeating: conic.repeating,
                    border_radius,
                });
            }
        }
    }

    /// Convert rustkit_css::BackgroundSize to layout BackgroundSize.
    fn convert_background_size(&self, size: &rustkit_css::BackgroundSize) -> BackgroundSize {
        match size {
            rustkit_css::BackgroundSize::Auto => BackgroundSize::Auto,
            rustkit_css::BackgroundSize::Cover => BackgroundSize::Cover,
            rustkit_css::BackgroundSize::Contain => BackgroundSize::Contain,
            rustkit_css::BackgroundSize::Explicit { width, height } => {
                BackgroundSize::Explicit { width: *width, height: *height }
            }
        }
    }

    /// Convert rustkit_css::BackgroundPosition to (f32, f32) tuple.
    fn convert_background_position(&self, pos: &rustkit_css::BackgroundPosition) -> (f32, f32) {
        let x = match &pos.x {
            rustkit_css::BackgroundPositionValue::Percent(p) => *p,
            rustkit_css::BackgroundPositionValue::Px(_) => 0.0, // Will be handled in rendering
        };
        let y = match &pos.y {
            rustkit_css::BackgroundPositionValue::Percent(p) => *p,
            rustkit_css::BackgroundPositionValue::Px(_) => 0.0,
        };
        (x, y)
    }

    /// Convert rustkit_css::BackgroundRepeat to layout BackgroundRepeat.
    fn convert_background_repeat(&self, repeat: rustkit_css::BackgroundRepeat) -> BackgroundRepeat {
        match repeat {
            rustkit_css::BackgroundRepeat::Repeat => BackgroundRepeat::Repeat,
            rustkit_css::BackgroundRepeat::RepeatX => BackgroundRepeat::RepeatX,
            rustkit_css::BackgroundRepeat::RepeatY => BackgroundRepeat::RepeatY,
            rustkit_css::BackgroundRepeat::NoRepeat => BackgroundRepeat::NoRepeat,
            rustkit_css::BackgroundRepeat::Space => BackgroundRepeat::Space,
            rustkit_css::BackgroundRepeat::Round => BackgroundRepeat::Round,
        }
    }

    /// Render borders.
    fn render_borders(&mut self, layout_box: &LayoutBox) {
        let d = &layout_box.dimensions;
        let s = &layout_box.style;

        // Render each border side separately for correct colors
        // Top border
        if d.border.top > 0.0 {
            let rect = Rect::new(
                d.border_box().x,
                d.border_box().y,
                d.border_box().width,
                d.border.top,
            );
            self.commands
                .push(DisplayCommand::SolidColor(s.border_top_color, rect));
        }

        // Right border
        if d.border.right > 0.0 {
            let rect = Rect::new(
                d.border_box().right() - d.border.right,
                d.border_box().y,
                d.border.right,
                d.border_box().height,
            );
            self.commands
                .push(DisplayCommand::SolidColor(s.border_right_color, rect));
        }

        // Bottom border
        if d.border.bottom > 0.0 {
            let rect = Rect::new(
                d.border_box().x,
                d.border_box().bottom() - d.border.bottom,
                d.border_box().width,
                d.border.bottom,
            );
            self.commands
                .push(DisplayCommand::SolidColor(s.border_bottom_color, rect));
        }

        // Left border
        if d.border.left > 0.0 {
            let rect = Rect::new(
                d.border_box().x,
                d.border_box().y,
                d.border.left,
                d.border_box().height,
            );
            self.commands
                .push(DisplayCommand::SolidColor(s.border_left_color, rect));
        }
    }

    /// Render text with decorations.
    fn render_text(&mut self, layout_box: &LayoutBox) {
        if let BoxType::Text(ref text) = layout_box.box_type {
            let style = &layout_box.style;
            let font_size = match &style.font_size {
                Length::Px(px) => *px,
                _ => 16.0,
            };

            let x = layout_box.dimensions.content.x;
            let content_y = layout_box.dimensions.content.y;
            let text_width = layout_box.dimensions.content.width;

            // Calculate half-leading for proper baseline alignment
            // CSS line-height creates extra space above and below the text content
            // The half-leading is split evenly above and below the text
            let line_height_multiplier = if style.line_height > 0.0 {
                style.line_height
            } else {
                1.2 // Default line-height
            };
            let line_height = font_size * line_height_multiplier;

            // Get font metrics for accurate baseline calculation
            let metrics = measure_text_advanced(
                text,
                &style.font_family,
                font_size,
                style.font_weight,
                style.font_style,
            );

            // Content height is ascent + descent (the actual rendered text height)
            let content_height = metrics.ascent + metrics.descent;

            // Half-leading is the space above (and below) the text content
            let half_leading = ((line_height - content_height) / 2.0).max(0.0);

            // Adjust y to account for half-leading - this places the text baseline correctly
            let y = content_y + half_leading;

            // Check if this is gradient text (background-clip: text with gradient and transparent fill)
            let is_gradient_text = style.background_clip == rustkit_css::BackgroundClip::Text
                && style.webkit_text_fill_color == Some(rustkit_css::Color::TRANSPARENT)
                && style.background_gradient.is_some();

            if is_gradient_text {
                // Emit gradient text command
                if let Some(gradient) = &style.background_gradient {
                    self.commands.push(DisplayCommand::GradientText {
                        text: text.clone(),
                        x,
                        y,
                        font_size,
                        font_family: style.font_family.clone(),
                        font_weight: style.font_weight.0,
                        font_style: match style.font_style {
                            rustkit_css::FontStyle::Normal => 0,
                            rustkit_css::FontStyle::Italic => 1,
                            rustkit_css::FontStyle::Oblique => 2,
                        },
                        gradient: gradient.clone(),
                        rect: Rect::new(x, y, text_width, layout_box.dimensions.content.height),
                    });
                    return; // Skip regular text rendering
                }
            }

            // Draw regular text
            self.commands.push(DisplayCommand::Text {
                text: text.clone(),
                x,
                y,
                color: style.color,
                font_size,
                font_family: style.font_family.clone(),
                font_weight: style.font_weight.0,
                font_style: match style.font_style {
                    rustkit_css::FontStyle::Normal => 0,
                    rustkit_css::FontStyle::Italic => 1,
                    rustkit_css::FontStyle::Oblique => 2,
                },
            });

            // Draw text decorations
            let decoration_line = style.text_decoration_line;
            if decoration_line.underline || decoration_line.overline || decoration_line.line_through
            {
                let decoration_color = style.text_decoration_color.unwrap_or(style.color);
                let decoration_style = match style.text_decoration_style {
                    rustkit_css::TextDecorationStyle::Solid => TextDecorationStyleValue::Solid,
                    rustkit_css::TextDecorationStyle::Double => TextDecorationStyleValue::Double,
                    rustkit_css::TextDecorationStyle::Dotted => TextDecorationStyleValue::Dotted,
                    rustkit_css::TextDecorationStyle::Dashed => TextDecorationStyleValue::Dashed,
                    rustkit_css::TextDecorationStyle::Wavy => TextDecorationStyleValue::Wavy,
                };

                // Get actual font metrics for accurate decoration positioning
                let metrics = measure_text_advanced(
                    text,
                    &style.font_family,
                    font_size,
                    style.font_weight,
                    style.font_style,
                );
                
                // Calculate thickness from style or font metrics
                let thickness = match style.text_decoration_thickness {
                    Length::Px(px) => px,
                    Length::Em(em) => em * font_size,
                    _ => {
                        // Use font metrics if available, otherwise fallback
                        if metrics.underline_thickness > 0.0 {
                            metrics.underline_thickness
                        } else {
                            font_size / 14.0
                        }
                    }
                };

                // Use actual metrics for positioning
                let ascent = if metrics.ascent > 0.0 { metrics.ascent } else { font_size * 0.8 };

                // Underline: position below baseline using font metrics
                if decoration_line.underline {
                    let underline_y = if metrics.underline_offset != 0.0 {
                        // Font provides underline position (negative = below baseline)
                        y + ascent - metrics.underline_offset
                    } else {
                        // Fallback: position slightly below baseline
                        y + ascent + font_size * 0.1
                    };
                    
                    self.commands.push(DisplayCommand::TextDecoration {
                        x,
                        y: underline_y,
                        width: text_width,
                        thickness,
                        color: decoration_color,
                        style: decoration_style,
                    });
                }

                // Overline: position at top of text
                if decoration_line.overline {
                    let overline_y = if metrics.overline_offset != 0.0 {
                        y + ascent - metrics.overline_offset
                    } else {
                        y // At top of text box
                    };
                    
                    self.commands.push(DisplayCommand::TextDecoration {
                        x,
                        y: overline_y,
                        width: text_width,
                        thickness,
                        color: decoration_color,
                        style: decoration_style,
                    });
                }

                // Line-through (strikethrough): position at middle of x-height
                if decoration_line.line_through {
                    let strikethrough_y = if metrics.strikethrough_offset != 0.0 {
                        y + ascent - metrics.strikethrough_offset
                    } else {
                        // Fallback: approximately middle of x-height
                        y + ascent * 0.35
                    };
                    
                    self.commands.push(DisplayCommand::TextDecoration {
                        x,
                        y: strikethrough_y,
                        width: text_width,
                        thickness,
                        color: decoration_color,
                        style: decoration_style,
                    });
                }
            }
        }
    }
    
    /// Render replaced content (images).
    fn render_replaced_content(&mut self, layout_box: &LayoutBox) {
        match &layout_box.box_type {
            BoxType::Image { url, natural_width, natural_height } => {
                let dims = &layout_box.dimensions;
                let container = Rect {
                    x: dims.content.x,
                    y: dims.content.y,
                    width: dims.content.width,
                    height: dims.content.height,
                };
                
                // Parse object-fit from style
                let object_fit = match layout_box.style.object_fit.as_str() {
                    "fill" => ObjectFit::Fill,
                    "contain" => ObjectFit::Contain,
                    "cover" => ObjectFit::Cover,
                    "none" => ObjectFit::None,
                    "scale-down" => ObjectFit::ScaleDown,
                    _ => ObjectFit::Contain,
                };
                
                let (pos_x, pos_y) = layout_box.style.object_position;
                
                // Generate image display command
                let cmd = crate::images::render_image(
                    url,
                    container,
                    *natural_width,
                    *natural_height,
                    object_fit,
                    (pos_x, pos_y),
                    layout_box.style.opacity,
                );
                
                self.commands.push(cmd);
            }
            BoxType::FormControl(control) => {
                self.render_form_control(layout_box, control);
            }
            _ => {}
        }
    }
    
    /// Render a form control.
    fn render_form_control(&mut self, layout_box: &LayoutBox, control: &FormControlType) {
        let dims = &layout_box.dimensions;
        let rect = Rect {
            x: dims.content.x,
            y: dims.content.y,
            width: dims.content.width,
            height: dims.content.height,
        };
        
        let font_size = match layout_box.style.font_size {
            Length::Px(px) => px,
            _ => 16.0,
        };
        
        let text_color = layout_box.style.color;
        let bg_color = layout_box.style.background_color;
        let border_color = layout_box.style.border_top_color;
        
        match control {
            FormControlType::TextInput { value, placeholder, .. } => {
                self.commands.push(DisplayCommand::TextInput {
                    rect,
                    value: value.clone(),
                    placeholder: placeholder.clone(),
                    font_size,
                    text_color,
                    placeholder_color: Color::new(160, 160, 160, 1.0),
                    background_color: if bg_color.a > 0.0 { bg_color } else { Color::WHITE },
                    border_color: if border_color.a > 0.0 { border_color } else { Color::new(200, 200, 200, 1.0) },
                    border_width: 1.0,
                    focused: false, // TODO: track focus state
                    caret_position: None,
                });
            }
            FormControlType::TextArea { value, placeholder, .. } => {
                self.commands.push(DisplayCommand::TextInput {
                    rect,
                    value: value.clone(),
                    placeholder: placeholder.clone(),
                    font_size,
                    text_color,
                    placeholder_color: Color::new(160, 160, 160, 1.0),
                    background_color: if bg_color.a > 0.0 { bg_color } else { Color::WHITE },
                    border_color: if border_color.a > 0.0 { border_color } else { Color::new(200, 200, 200, 1.0) },
                    border_width: 1.0,
                    focused: false,
                    caret_position: None,
                });
            }
            FormControlType::Button { label, .. } => {
                self.commands.push(DisplayCommand::Button {
                    rect,
                    label: label.clone(),
                    font_size,
                    text_color: if text_color.a > 0.0 { text_color } else { Color::BLACK },
                    background_color: if bg_color.a > 0.0 { bg_color } else { Color::new(239, 239, 239, 1.0) },
                    border_color: if border_color.a > 0.0 { border_color } else { Color::new(180, 180, 180, 1.0) },
                    border_width: 1.0,
                    border_radius: 4.0,
                    pressed: false,
                    focused: false,
                });
            }
            FormControlType::Checkbox { checked } => {
                // Draw checkbox as a small rect with optional checkmark
                let check_color = if *checked { text_color } else { Color::TRANSPARENT };
                self.commands.push(DisplayCommand::SolidColor(
                    if bg_color.a > 0.0 { bg_color } else { Color::WHITE },
                    rect,
                ));
                self.commands.push(DisplayCommand::Border {
                    color: Color::new(150, 150, 150, 1.0),
                    rect,
                    top: 1.0,
                    right: 1.0,
                    bottom: 1.0,
                    left: 1.0,
                });
                if *checked {
                    // Draw a simple checkmark using lines (simplified)
                    let inner = Rect {
                        x: rect.x + 3.0,
                        y: rect.y + 3.0,
                        width: rect.width - 6.0,
                        height: rect.height - 6.0,
                    };
                    self.commands.push(DisplayCommand::SolidColor(check_color, inner));
                }
            }
            FormControlType::Radio { checked, .. } => {
                // Draw radio as a circle (using ellipse)
                self.commands.push(DisplayCommand::FillEllipse {
                    rect,
                    color: if bg_color.a > 0.0 { bg_color } else { Color::WHITE },
                });
                // Outer ring
                self.commands.push(DisplayCommand::StrokeCircle {
                    cx: rect.x + rect.width / 2.0,
                    cy: rect.y + rect.height / 2.0,
                    radius: rect.width / 2.0 - 1.0,
                    color: Color::new(150, 150, 150, 1.0),
                    width: 1.0,
                });
                if *checked {
                    // Inner dot
                    self.commands.push(DisplayCommand::FillCircle {
                        cx: rect.x + rect.width / 2.0,
                        cy: rect.y + rect.height / 2.0,
                        radius: rect.width / 4.0,
                        color: text_color,
                    });
                }
            }
            FormControlType::Select { options, selected_index } => {
                // Draw as a text input with dropdown arrow
                let display_text = selected_index
                    .and_then(|i| options.get(i))
                    .cloned()
                    .unwrap_or_default();
                
                self.commands.push(DisplayCommand::TextInput {
                    rect,
                    value: display_text,
                    placeholder: String::new(),
                    font_size,
                    text_color,
                    placeholder_color: Color::new(160, 160, 160, 1.0),
                    background_color: if bg_color.a > 0.0 { bg_color } else { Color::WHITE },
                    border_color: if border_color.a > 0.0 { border_color } else { Color::new(200, 200, 200, 1.0) },
                    border_width: 1.0,
                    focused: false,
                    caret_position: None,
                });
            }
        }
    }
}

/// Measure text using the text shaper.
///
/// This provides accurate text measurement using DirectWrite on Windows.
pub fn measure_text_advanced(
    text: &str,
    font_family: &str,
    font_size: f32,
    font_weight: rustkit_css::FontWeight,
    font_style: rustkit_css::FontStyle,
) -> TextMetrics {
    measure_text_with_spacing(text, font_family, font_size, font_weight, font_style, 0.0, 0.0)
}

/// Measure text using the text shaper with letter-spacing and word-spacing.
///
/// This provides accurate text measurement using DirectWrite on Windows,
/// with support for CSS letter-spacing and word-spacing properties.
pub fn measure_text_with_spacing(
    text: &str,
    font_family: &str,
    font_size: f32,
    font_weight: rustkit_css::FontWeight,
    font_style: rustkit_css::FontStyle,
    letter_spacing: f32,
    word_spacing: f32,
) -> TextMetrics {
    let shaper = TextShaper::new();
    let chain = FontFamilyChain::from_css_value(font_family);

    match shaper.shape(
        text,
        &chain,
        font_weight,
        font_style,
        rustkit_css::FontStretch::Normal,
        font_size,
    ) {
        Ok(mut run) => {
            // Apply letter-spacing and word-spacing
            run.apply_spacing(letter_spacing, word_spacing);
            run.metrics
        }
        Err(_) => {
            // Fallback to simple measurement with spacing
            let mut metrics = measure_text_simple(text, font_size);
            // Apply letter-spacing (one per character)
            let char_count = text.chars().count();
            metrics.width += letter_spacing * char_count as f32;
            // Apply word-spacing (one per whitespace)
            let space_count = text.chars().filter(|c| c.is_whitespace()).count();
            metrics.width += word_spacing * space_count as f32;
            metrics
        }
    }
}

/// Simple text measurement (fallback when shaping is unavailable).
pub fn measure_text_simple(text: &str, font_size: f32) -> TextMetrics {
    // Approximate metrics based on font size
    // Typical Latin font has ~0.5em average character width
    let avg_char_width = font_size * 0.5;
    let width = text.chars().count() as f32 * avg_char_width;

    TextMetrics {
        width,
        ..TextMetrics::with_font_size(font_size)
    }
}

/// Measure text (simplified - uses average character width approximation).
///
/// For more accurate measurement, use `measure_text_advanced`.
#[deprecated(
    since = "0.1.0",
    note = "Use measure_text_advanced for accurate measurement"
)]
pub fn measure_text(text: &str, _font_family: &str, font_size: f32) -> text::TextMetrics {
    measure_text_simple(text, font_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rect() {
        let r = Rect::new(10.0, 20.0, 100.0, 50.0);
        assert_eq!(r.right(), 110.0);
        assert_eq!(r.bottom(), 70.0);
        assert!(r.contains(50.0, 30.0));
        assert!(!r.contains(0.0, 0.0));
    }

    #[test]
    fn test_dimensions_boxes() {
        let d = Dimensions {
            content: Rect::new(20.0, 20.0, 100.0, 50.0),
            padding: EdgeSizes {
                top: 5.0,
                right: 5.0,
                bottom: 5.0,
                left: 5.0,
            },
            border: EdgeSizes {
                top: 1.0,
                right: 1.0,
                bottom: 1.0,
                left: 1.0,
            },
            margin: EdgeSizes {
                top: 10.0,
                right: 10.0,
                bottom: 10.0,
                left: 10.0,
            },
        };

        let pb = d.padding_box();
        assert_eq!(pb.width, 110.0);
        assert_eq!(pb.height, 60.0);

        let bb = d.border_box();
        assert_eq!(bb.width, 112.0);
        assert_eq!(bb.height, 62.0);

        let mb = d.margin_box();
        assert_eq!(mb.width, 132.0);
        assert_eq!(mb.height, 82.0);
    }

    #[test]
    fn test_layout_box_creation() {
        let style = ComputedStyle::new();
        let layout_box = LayoutBox::new(BoxType::Block, style);
        assert!(matches!(layout_box.box_type, BoxType::Block));
        assert_eq!(layout_box.position, Position::Static);
        assert_eq!(layout_box.float, Float::None);
    }

    #[test]
    fn test_layout_box_with_position() {
        let style = ComputedStyle::new();
        let layout_box = LayoutBox::with_position(BoxType::Block, style, Position::Relative);
        assert_eq!(layout_box.position, Position::Relative);
        assert!(layout_box.stacking_context.is_some());
    }

    #[test]
    fn test_layout_box_with_float() {
        let style = ComputedStyle::new();
        let layout_box = LayoutBox::with_float(BoxType::Block, style, Float::Left);
        assert_eq!(layout_box.float, Float::Left);
    }

    #[test]
    fn test_margin_collapse_positive() {
        let mut ctx = MarginCollapseContext::new();
        ctx.add_margin(10.0);
        ctx.add_margin(20.0);
        assert_eq!(ctx.resolve(), 20.0); // Max of positive margins
    }

    #[test]
    fn test_margin_collapse_negative() {
        let mut ctx = MarginCollapseContext::new();
        ctx.add_margin(-10.0);
        ctx.add_margin(-20.0);
        assert_eq!(ctx.resolve(), -20.0); // Min of negative margins
    }

    #[test]
    fn test_margin_collapse_mixed() {
        let mut ctx = MarginCollapseContext::new();
        ctx.add_margin(20.0);
        ctx.add_margin(-10.0);
        assert_eq!(ctx.resolve(), 10.0); // Sum of max positive and min negative
    }

    #[test]
    fn test_float_context() {
        let mut ctx = FloatContext::new();

        // Add a left float
        ctx.add_left(Rect::new(0.0, 0.0, 100.0, 50.0));

        // Check available width at y=25 (within float)
        let (left, right) = ctx.available_width(25.0, 500.0);
        assert_eq!(left, 100.0); // Left edge is after the float
        assert_eq!(right, 500.0); // Right edge is container width

        // Check available width at y=60 (below float)
        let (left, right) = ctx.available_width(60.0, 500.0);
        assert_eq!(left, 0.0); // No float at this y
        assert_eq!(right, 500.0);
    }

    #[test]
    fn test_float_clear() {
        let mut ctx = FloatContext::new();

        ctx.add_left(Rect::new(0.0, 0.0, 100.0, 50.0));
        ctx.add_right(Rect::new(400.0, 0.0, 100.0, 80.0));

        assert_eq!(ctx.clear(Clear::Left), 50.0);
        assert_eq!(ctx.clear(Clear::Right), 80.0);
        assert_eq!(ctx.clear(Clear::Both), 80.0);
        assert_eq!(ctx.clear(Clear::None), 0.0);
    }

    #[test]
    fn test_position_offsets() {
        let style = ComputedStyle::new();
        let mut layout_box = LayoutBox::with_position(BoxType::Block, style, Position::Relative);
        layout_box.set_offsets(Some(10.0), None, None, Some(20.0));

        assert_eq!(layout_box.offsets.top, Some(10.0));
        assert_eq!(layout_box.offsets.left, Some(20.0));
        assert_eq!(layout_box.offsets.right, None);
        assert_eq!(layout_box.offsets.bottom, None);
    }

    #[test]
    fn test_z_index_stacking() {
        let style = ComputedStyle::new();
        let mut layout_box = LayoutBox::with_position(BoxType::Block, style, Position::Absolute);
        layout_box.set_z_index(5);

        assert_eq!(layout_box.z_index, 5);
        let ctx = layout_box.stacking_context.as_ref().unwrap();
        assert!(ctx.creates_context);
        assert_eq!(ctx.z_index, 5);
    }

    #[test]
    fn test_display_list_build() {
        let mut style = ComputedStyle::new();
        style.background_color = Color::from_rgb(255, 255, 255);

        let mut layout_box = LayoutBox::new(BoxType::Block, style);
        // Set dimensions - render_background skips zero-sized boxes
        layout_box.dimensions.content = Rect::new(0.0, 0.0, 100.0, 100.0);
        let display_list = DisplayList::build(&layout_box);

        assert!(!display_list.commands.is_empty());
    }

    #[test]
    fn test_display_list_with_positioned() {
        let style = ComputedStyle::new();
        let mut parent = LayoutBox::new(BoxType::Block, style.clone());

        let mut child = LayoutBox::with_position(BoxType::Block, style, Position::Absolute);
        child.set_z_index(-1);
        parent.children.push(child);

        let display_list = DisplayList::build(&parent);

        // Should have commands for both parent and child
        assert!(!display_list.commands.is_empty());
    }

    #[test]
    fn test_paint_order() {
        let style = ComputedStyle::new();
        let mut parent = LayoutBox::new(BoxType::Block, style.clone());

        // Add normal flow child
        let normal = LayoutBox::new(BoxType::Block, style.clone());
        parent.children.push(normal);

        // Add positioned child with positive z-index
        let mut positive_z =
            LayoutBox::with_position(BoxType::Block, style.clone(), Position::Absolute);
        positive_z.set_z_index(1);
        parent.children.push(positive_z);

        // Add positioned child with negative z-index
        let mut negative_z = LayoutBox::with_position(BoxType::Block, style, Position::Absolute);
        negative_z.set_z_index(-1);
        parent.children.push(negative_z);

        let paint_order = parent.get_paint_order();

        // Order should be: negative z-index, normal flow, positive z-index
        assert_eq!(paint_order.len(), 3);
        assert_eq!(paint_order[0].z_index, -1);
        assert_eq!(paint_order[1].position, Position::Static);
        assert_eq!(paint_order[2].z_index, 1);
    }

    #[test]
    fn test_sticky_positioning_state_initialization() {
        let style = ComputedStyle::new();
        let mut layout_box = LayoutBox::with_position(BoxType::Block, style, Position::Sticky);

        // Set position offsets (sticky threshold)
        layout_box.set_offsets(Some(10.0), None, None, None);

        // Set dimensions as if layout happened
        layout_box.dimensions.content.x = 0.0;
        layout_box.dimensions.content.y = 100.0;
        layout_box.dimensions.content.width = 200.0;
        layout_box.dimensions.content.height = 50.0;

        // Create containing block
        let containing_block = Dimensions {
            content: Rect::new(0.0, 0.0, 800.0, 600.0),
            ..Default::default()
        };

        // Apply position offsets - this should initialize sticky_state
        layout_box.apply_position_offsets(&containing_block);

        // Verify sticky_state was created
        assert!(layout_box.sticky_state.is_some());

        let sticky = layout_box.sticky_state.as_ref().unwrap();
        assert!(!sticky.is_stuck);
        assert!(sticky.offsets.top.is_some());
        assert_eq!(sticky.offsets.top.unwrap(), 10.0);
    }

    #[test]
    fn test_sticky_update_not_stuck() {
        let style = ComputedStyle::new();
        let mut layout_box = LayoutBox::with_position(BoxType::Block, style, Position::Sticky);

        layout_box.set_offsets(Some(10.0), None, None, None);

        // Element at y=100
        layout_box.dimensions.content.y = 100.0;
        layout_box.dimensions.content.height = 50.0;
        layout_box.dimensions.content.width = 200.0;

        let containing_block = Dimensions::default();
        layout_box.apply_position_offsets(&containing_block);

        let container = Rect::new(0.0, 0.0, 800.0, 600.0);

        // Scroll position is 0 - element should not be stuck
        // Threshold is original_y - top_offset = 100 - 10 = 90
        layout_box.update_sticky_positions(0.0, 0.0, container);

        let sticky = layout_box.sticky_state.as_ref().unwrap();
        assert!(!sticky.is_stuck);
    }

    #[test]
    fn test_sticky_update_stuck() {
        let style = ComputedStyle::new();
        let mut layout_box = LayoutBox::with_position(BoxType::Block, style, Position::Sticky);

        layout_box.set_offsets(Some(10.0), None, None, None);

        // Element at y=100
        layout_box.dimensions.content.y = 100.0;
        layout_box.dimensions.content.height = 50.0;
        layout_box.dimensions.content.width = 200.0;

        let containing_block = Dimensions::default();
        layout_box.apply_position_offsets(&containing_block);

        let container = Rect::new(0.0, 0.0, 800.0, 600.0);

        // Scroll position is 150 - element should be stuck
        // Threshold is original_y - top_offset = 100 - 10 = 90
        // scroll_y (150) > threshold (90), so should stick
        layout_box.update_sticky_positions(0.0, 150.0, container);

        let sticky = layout_box.sticky_state.as_ref().unwrap();
        assert!(sticky.is_stuck);
    }

    #[test]
    fn test_sticky_reset() {
        let style = ComputedStyle::new();
        let mut layout_box = LayoutBox::with_position(BoxType::Block, style, Position::Sticky);

        layout_box.set_offsets(Some(10.0), None, None, None);

        layout_box.dimensions.content.y = 100.0;
        layout_box.dimensions.content.height = 50.0;
        layout_box.dimensions.content.width = 200.0;

        let containing_block = Dimensions::default();
        layout_box.apply_position_offsets(&containing_block);

        let container = Rect::new(0.0, 0.0, 800.0, 600.0);

        // First, make it stuck
        layout_box.update_sticky_positions(0.0, 150.0, container);
        assert!(layout_box.sticky_state.as_ref().unwrap().is_stuck);

        // Reset
        layout_box.reset_sticky_positions();
        assert!(!layout_box.sticky_state.as_ref().unwrap().is_stuck);
    }

    #[test]
    fn test_display_list_build_with_scroll() {
        let style = ComputedStyle::new();
        let mut layout_box = LayoutBox::new(BoxType::Block, style.clone());
        layout_box.dimensions.content = Rect::new(0.0, 0.0, 800.0, 600.0);

        // Add a sticky child
        let mut sticky_child =
            LayoutBox::with_position(BoxType::Block, style, Position::Sticky);
        sticky_child.set_offsets(Some(10.0), None, None, None);
        sticky_child.dimensions.content = Rect::new(0.0, 100.0, 200.0, 50.0);

        let containing_block = Dimensions::default();
        sticky_child.apply_position_offsets(&containing_block);

        layout_box.children.push(sticky_child);

        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);

        // Build with scroll = 0 (not stuck)
        let _display_list = DisplayList::build_with_scroll(&mut layout_box, 0.0, 0.0, viewport);

        // Verify sticky child is not stuck
        let sticky = layout_box.children[0].sticky_state.as_ref().unwrap();
        assert!(!sticky.is_stuck);

        // Build with scroll = 150 (should be stuck)
        let _display_list = DisplayList::build_with_scroll(&mut layout_box, 0.0, 150.0, viewport);

        // Verify sticky child is now stuck
        let sticky = layout_box.children[0].sticky_state.as_ref().unwrap();
        assert!(sticky.is_stuck);
    }
}
