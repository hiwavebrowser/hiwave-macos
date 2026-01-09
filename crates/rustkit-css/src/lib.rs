//! # RustKit CSS
//!
//! CSS parsing and style computation for the RustKit browser engine.
//!
//! ## Design Goals
//!
//! 1. **Property parsing**: Parse CSS property values
//! 2. **Cascade**: Apply specificity and origin rules
//! 3. **Inheritance**: Propagate inherited properties to children
//! 4. **Computed values**: Resolve relative units and keywords

use thiserror::Error;
use tracing::debug;
use rustkit_cssparser::parse_stylesheet;

/// Errors that can occur in CSS operations.
#[derive(Error, Debug)]
pub enum CssError {
    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Invalid value: {0}")]
    InvalidValue(String),
}

/// A CSS color value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f32,
}

impl Color {
    pub const TRANSPARENT: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 0.0,
    };
    pub const BLACK: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 1.0,
    };
    pub const WHITE: Color = Color {
        r: 255,
        g: 255,
        b: 255,
        a: 1.0,
    };

    pub fn new(r: u8, g: u8, b: u8, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Convert to [f64; 4] for rendering.
    pub fn to_f64_array(&self) -> [f64; 4] {
        [
            self.r as f64 / 255.0,
            self.g as f64 / 255.0,
            self.b as f64 / 255.0,
            self.a as f64,
        ]
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::BLACK
    }
}

/// A CSS length value.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Length {
    /// Pixels.
    Px(f32),
    /// Em (relative to font size).
    Em(f32),
    /// Rem (relative to root font size).
    Rem(f32),
    /// Percentage.
    Percent(f32),
    /// Viewport width (1vw = 1% of viewport width).
    Vw(f32),
    /// Viewport height (1vh = 1% of viewport height).
    Vh(f32),
    /// Viewport min (1vmin = 1% of smaller viewport dimension).
    Vmin(f32),
    /// Viewport max (1vmax = 1% of larger viewport dimension).
    Vmax(f32),
    /// Auto.
    Auto,
    /// Zero.
    #[default]
    Zero,
    /// min(a, b) - returns the smaller of two lengths.
    Min(Box<(Length, Length)>),
    /// max(a, b) - returns the larger of two lengths.
    Max(Box<(Length, Length)>),
    /// clamp(min, preferred, max) - clamps preferred between min and max.
    Clamp(Box<(Length, Length, Length)>),
}

impl Length {
    /// Compute the absolute pixel value.
    /// 
    /// For viewport units, pass viewport dimensions via `viewport_width` and `viewport_height`.
    pub fn to_px(&self, font_size: f32, root_font_size: f32, container_size: f32) -> f32 {
        self.to_px_with_viewport(font_size, root_font_size, container_size, 0.0, 0.0)
    }
    
    /// Compute the absolute pixel value with viewport dimensions for vh/vw units.
    pub fn to_px_with_viewport(
        &self,
        font_size: f32,
        root_font_size: f32,
        container_size: f32,
        viewport_width: f32,
        viewport_height: f32,
    ) -> f32 {
        match self {
            Length::Px(px) => *px,
            Length::Em(em) => em * font_size,
            Length::Rem(rem) => rem * root_font_size,
            Length::Percent(pct) => pct / 100.0 * container_size,
            Length::Vw(vw) => vw / 100.0 * viewport_width,
            Length::Vh(vh) => vh / 100.0 * viewport_height,
            Length::Vmin(vmin) => vmin / 100.0 * viewport_width.min(viewport_height),
            Length::Vmax(vmax) => vmax / 100.0 * viewport_width.max(viewport_height),
            Length::Auto => 0.0, // Context-dependent
            Length::Zero => 0.0,
            Length::Min(pair) => {
                let a = pair.0.to_px_with_viewport(font_size, root_font_size, container_size, viewport_width, viewport_height);
                let b = pair.1.to_px_with_viewport(font_size, root_font_size, container_size, viewport_width, viewport_height);
                a.min(b)
            }
            Length::Max(pair) => {
                let a = pair.0.to_px_with_viewport(font_size, root_font_size, container_size, viewport_width, viewport_height);
                let b = pair.1.to_px_with_viewport(font_size, root_font_size, container_size, viewport_width, viewport_height);
                a.max(b)
            }
            Length::Clamp(triple) => {
                let min_val = triple.0.to_px_with_viewport(font_size, root_font_size, container_size, viewport_width, viewport_height);
                let pref = triple.1.to_px_with_viewport(font_size, root_font_size, container_size, viewport_width, viewport_height);
                let max_val = triple.2.to_px_with_viewport(font_size, root_font_size, container_size, viewport_width, viewport_height);
                pref.clamp(min_val, max_val)
            }
        }
    }
}

/// A CSS box-shadow value.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BoxShadow {
    /// Horizontal offset (positive = right).
    pub offset_x: f32,
    /// Vertical offset (positive = down).
    pub offset_y: f32,
    /// Blur radius (0 = sharp edge).
    pub blur_radius: f32,
    /// Spread radius (positive = larger shadow).
    pub spread_radius: f32,
    /// Shadow color.
    pub color: Color,
    /// Whether this is an inset shadow.
    pub inset: bool,
}

impl BoxShadow {
    /// Create a new box shadow with default values.
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Create a simple drop shadow.
    pub fn drop_shadow(offset_x: f32, offset_y: f32, blur: f32, color: Color) -> Self {
        Self {
            offset_x,
            offset_y,
            blur_radius: blur,
            spread_radius: 0.0,
            color,
            inset: false,
        }
    }
    
    /// Check if this shadow is visible (non-zero offset, blur, or spread with non-transparent color).
    pub fn is_visible(&self) -> bool {
        self.color.a > 0.0 && 
        (self.offset_x != 0.0 || self.offset_y != 0.0 || self.blur_radius > 0.0 || self.spread_radius != 0.0)
    }
}

/// A color stop for gradients.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorStop {
    /// The color at this stop.
    pub color: Color,
    /// Position along the gradient (0.0 to 1.0, or None for auto).
    pub position: Option<f32>,
}

impl ColorStop {
    pub fn new(color: Color, position: Option<f32>) -> Self {
        Self { color, position }
    }
}

/// Direction for linear gradients.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum GradientDirection {
    /// Angle in degrees (0 = to top, 90 = to right, 180 = to bottom, 270 = to left).
    Angle(f32),
    /// To top (0deg).
    #[default]
    ToTop,
    /// To right (90deg).
    ToRight,
    /// To bottom (180deg).
    ToBottom,
    /// To left (270deg).
    ToLeft,
    /// To top-right (45deg).
    ToTopRight,
    /// To top-left (315deg).
    ToTopLeft,
    /// To bottom-right (135deg).
    ToBottomRight,
    /// To bottom-left (225deg).
    ToBottomLeft,
}

impl GradientDirection {
    /// Convert to angle in degrees.
    pub fn to_degrees(&self) -> f32 {
        match self {
            GradientDirection::Angle(deg) => *deg,
            GradientDirection::ToTop => 0.0,
            GradientDirection::ToRight => 90.0,
            GradientDirection::ToBottom => 180.0,
            GradientDirection::ToLeft => 270.0,
            GradientDirection::ToTopRight => 45.0,
            GradientDirection::ToTopLeft => 315.0,
            GradientDirection::ToBottomRight => 135.0,
            GradientDirection::ToBottomLeft => 225.0,
        }
    }
}

/// A CSS linear gradient.
#[derive(Debug, Clone, PartialEq)]
pub struct LinearGradient {
    /// Direction of the gradient.
    pub direction: GradientDirection,
    /// Color stops.
    pub stops: Vec<ColorStop>,
}

impl LinearGradient {
    pub fn new(direction: GradientDirection, stops: Vec<ColorStop>) -> Self {
        Self { direction, stops }
    }
}

/// A CSS radial gradient.
#[derive(Debug, Clone, PartialEq)]
pub struct RadialGradient {
    /// Shape: "circle" or "ellipse".
    pub shape: RadialShape,
    /// Size of the gradient.
    pub size: RadialSize,
    /// Center position (0.0 to 1.0, default 0.5).
    pub center: (f32, f32),
    /// Color stops.
    pub stops: Vec<ColorStop>,
}

impl RadialGradient {
    pub fn new(shape: RadialShape, size: RadialSize, center: (f32, f32), stops: Vec<ColorStop>) -> Self {
        Self { shape, size, center, stops }
    }
}

/// Shape of a radial gradient.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RadialShape {
    /// Circle (equal radius in all directions).
    Circle,
    /// Ellipse (can stretch in one direction).
    #[default]
    Ellipse,
}

/// Size of a radial gradient.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum RadialSize {
    /// Closest side.
    ClosestSide,
    /// Farthest side.
    #[default]
    FarthestSide,
    /// Closest corner.
    ClosestCorner,
    /// Farthest corner.
    FarthestCorner,
    /// Explicit radius (for circles) or radii (for ellipses).
    Explicit(f32, f32),
}

/// A CSS gradient (linear or radial).
#[derive(Debug, Clone, PartialEq)]
pub enum Gradient {
    Linear(LinearGradient),
    Radial(RadialGradient),
}

/// Display property values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Display {
    #[default]
    Block,
    Inline,
    InlineBlock,
    Flex,
    InlineFlex,
    Grid,
    InlineGrid,
    None,
}


impl Display {
    /// Check if this is a flex container.
    pub fn is_flex(self) -> bool {
        matches!(self, Display::Flex | Display::InlineFlex)
    }

    /// Check if this is a grid container.
    pub fn is_grid(self) -> bool {
        matches!(self, Display::Grid | Display::InlineGrid)
    }
}

// ==================== Flexbox Types ====================

/// Flex direction property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlexDirection {
    #[default]
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

impl FlexDirection {
    /// Check if this direction is reversed.
    pub fn is_reverse(self) -> bool {
        matches!(self, FlexDirection::RowReverse | FlexDirection::ColumnReverse)
    }

    /// Check if this is a row direction.
    pub fn is_row(self) -> bool {
        matches!(self, FlexDirection::Row | FlexDirection::RowReverse)
    }

    /// Check if this is a column direction.
    pub fn is_column(self) -> bool {
        matches!(self, FlexDirection::Column | FlexDirection::ColumnReverse)
    }
}

/// Flex wrap property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlexWrap {
    #[default]
    NoWrap,
    Wrap,
    WrapReverse,
}

/// Justify content property (main axis alignment).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JustifyContent {
    #[default]
    FlexStart,
    FlexEnd,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// Align items property (cross axis alignment for all items).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlignItems {
    #[default]
    Stretch,
    FlexStart,
    FlexEnd,
    Center,
    Baseline,
}

/// Align content property (multi-line cross axis alignment).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlignContent {
    #[default]
    Stretch,
    FlexStart,
    FlexEnd,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// Align self property (cross axis alignment for individual item).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlignSelf {
    #[default]
    Auto,
    FlexStart,
    FlexEnd,
    Center,
    Baseline,
    Stretch,
}

/// Flex basis property.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum FlexBasis {
    /// Use the item's main size property (width or height).
    #[default]
    Auto,
    /// Size based on content.
    Content,
    /// Explicit length.
    Length(f32),
    /// Percentage of container.
    Percent(f32),
}

// ==================== Grid Types ====================

/// A grid track size.
#[derive(Debug, Clone, PartialEq)]
pub enum TrackSize {
    /// Fixed length in pixels.
    Px(f32),
    /// Percentage of container.
    Percent(f32),
    /// Fractional unit (flexible).
    Fr(f32),
    /// Size based on content minimum.
    MinContent,
    /// Size based on content maximum.
    MaxContent,
    /// Auto sizing.
    Auto,
    /// Minimum/maximum constraint.
    MinMax(Box<TrackSize>, Box<TrackSize>),
    /// Fit content with maximum.
    FitContent(f32),
}

impl Default for TrackSize {
    fn default() -> Self {
        TrackSize::Auto
    }
}

impl TrackSize {
    /// Create a fixed pixel size.
    pub fn px(value: f32) -> Self {
        TrackSize::Px(value)
    }

    /// Create a fractional size.
    pub fn fr(value: f32) -> Self {
        TrackSize::Fr(value)
    }

    /// Create a minmax constraint.
    pub fn minmax(min: TrackSize, max: TrackSize) -> Self {
        TrackSize::MinMax(Box::new(min), Box::new(max))
    }

    /// Check if this is a flexible track (contains fr units).
    pub fn is_flexible(&self) -> bool {
        match self {
            TrackSize::Fr(_) => true,
            TrackSize::MinMax(_, max) => max.is_flexible(),
            _ => false,
        }
    }

    /// Get the minimum size contribution.
    pub fn min_size(&self) -> f32 {
        match self {
            TrackSize::Px(v) => *v,
            TrackSize::MinMax(min, _) => min.min_size(),
            TrackSize::FitContent(max) => 0.0_f32.min(*max),
            _ => 0.0,
        }
    }
}

/// A grid track definition (for grid-template-columns/rows).
#[derive(Debug, Clone, PartialEq)]
pub struct TrackDefinition {
    /// Track sizing.
    pub size: TrackSize,
    /// Optional line name(s) before this track.
    pub line_names: Vec<String>,
}

impl TrackDefinition {
    /// Create a simple track without line names.
    pub fn simple(size: TrackSize) -> Self {
        Self {
            size,
            line_names: Vec::new(),
        }
    }

    /// Create a track with line name.
    pub fn named(size: TrackSize, name: &str) -> Self {
        Self {
            size,
            line_names: vec![name.to_string()],
        }
    }
}

/// Repeat function for grid tracks.
#[derive(Debug, Clone, PartialEq)]
pub enum TrackRepeat {
    /// Repeat a fixed number of times.
    Count(u32, Vec<TrackDefinition>),
    /// Auto-fill: as many as fit.
    AutoFill(Vec<TrackDefinition>),
    /// Auto-fit: as many as fit, collapsing empty tracks.
    AutoFit(Vec<TrackDefinition>),
}

/// Grid template definition.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GridTemplate {
    /// Explicit track definitions.
    pub tracks: Vec<TrackDefinition>,
    /// Repeat patterns.
    pub repeats: Vec<(usize, TrackRepeat)>, // (insert_position, repeat)
    /// Final line names.
    pub final_line_names: Vec<String>,
}

impl GridTemplate {
    /// Create an empty template (no explicit tracks).
    pub fn none() -> Self {
        Self::default()
    }

    /// Create from a list of track sizes.
    pub fn from_sizes(sizes: Vec<TrackSize>) -> Self {
        Self {
            tracks: sizes.into_iter().map(TrackDefinition::simple).collect(),
            repeats: Vec::new(),
            final_line_names: Vec::new(),
        }
    }

    /// Get the number of explicit tracks.
    pub fn track_count(&self) -> usize {
        self.tracks.len()
    }
}

/// Named grid area.
#[derive(Debug, Clone, PartialEq)]
pub struct GridArea {
    pub name: String,
    pub row_start: i32,
    pub row_end: i32,
    pub column_start: i32,
    pub column_end: i32,
}

/// Grid template areas.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GridTemplateAreas {
    /// Row strings (e.g., ["header header", "nav main", "footer footer"]).
    pub rows: Vec<Vec<Option<String>>>,
    /// Named areas derived from rows.
    pub areas: Vec<GridArea>,
}

impl GridTemplateAreas {
    /// Parse grid-template-areas value.
    pub fn parse(value: &str) -> Option<Self> {
        let mut rows = Vec::new();
        
        for line in value.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            // Remove quotes if present
            let line = line.trim_matches('"').trim_matches('\'');
            
            let cells: Vec<Option<String>> = line
                .split_whitespace()
                .map(|s| {
                    if s == "." {
                        None
                    } else {
                        Some(s.to_string())
                    }
                })
                .collect();
            
            rows.push(cells);
        }

        if rows.is_empty() {
            return None;
        }

        // Extract named areas
        let mut areas = Vec::new();
        let mut area_names: std::collections::HashSet<String> = std::collections::HashSet::new();
        
        for (row_idx, row) in rows.iter().enumerate() {
            for (col_idx, cell) in row.iter().enumerate() {
                if let Some(name) = cell {
                    if !area_names.contains(name) {
                        // Find extent of this area
                        let (row_end, col_end) = Self::find_area_extent(&rows, row_idx, col_idx, name);
                        areas.push(GridArea {
                            name: name.clone(),
                            row_start: row_idx as i32 + 1,
                            row_end: row_end as i32 + 1,
                            column_start: col_idx as i32 + 1,
                            column_end: col_end as i32 + 1,
                        });
                        area_names.insert(name.clone());
                    }
                }
            }
        }

        Some(Self { rows, areas })
    }

    fn find_area_extent(rows: &[Vec<Option<String>>], start_row: usize, start_col: usize, name: &str) -> (usize, usize) {
        let mut row_end = start_row;
        let mut col_end = start_col;

        // Find column extent
        for col in start_col..rows[start_row].len() {
            if rows[start_row].get(col) == Some(&Some(name.to_string())) {
                col_end = col + 1;
            } else {
                break;
            }
        }

        // Find row extent
        for row in start_row..rows.len() {
            if rows[row].get(start_col) == Some(&Some(name.to_string())) {
                row_end = row + 1;
            } else {
                break;
            }
        }

        (row_end, col_end)
    }

    /// Get area by name.
    pub fn get_area(&self, name: &str) -> Option<&GridArea> {
        self.areas.iter().find(|a| a.name == name)
    }
}

/// Grid auto flow direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GridAutoFlow {
    #[default]
    Row,
    Column,
    RowDense,
    ColumnDense,
}

impl GridAutoFlow {
    /// Check if this is a row-based flow.
    pub fn is_row(self) -> bool {
        matches!(self, GridAutoFlow::Row | GridAutoFlow::RowDense)
    }

    /// Check if this uses dense packing.
    pub fn is_dense(self) -> bool {
        matches!(self, GridAutoFlow::RowDense | GridAutoFlow::ColumnDense)
    }
}

/// Grid line reference (for grid-column-start, etc.).
#[derive(Debug, Clone, PartialEq)]
pub enum GridLine {
    /// Auto placement.
    Auto,
    /// Specific line number (1-based, can be negative).
    Number(i32),
    /// Named line.
    Name(String),
    /// Span a number of tracks.
    Span(u32),
    /// Span to a named line.
    SpanName(String),
}

impl Default for GridLine {
    fn default() -> Self {
        GridLine::Auto
    }
}

/// Grid placement for an item.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GridPlacement {
    /// Column start line.
    pub column_start: GridLine,
    /// Column end line.
    pub column_end: GridLine,
    /// Row start line.
    pub row_start: GridLine,
    /// Row end line.
    pub row_end: GridLine,
}

impl GridPlacement {
    /// Create placement from a named area.
    pub fn from_area(name: &str) -> Self {
        Self {
            column_start: GridLine::Name(format!("{}-start", name)),
            column_end: GridLine::Name(format!("{}-end", name)),
            row_start: GridLine::Name(format!("{}-start", name)),
            row_end: GridLine::Name(format!("{}-end", name)),
        }
    }

    /// Create placement from explicit lines.
    pub fn from_lines(col_start: i32, col_end: i32, row_start: i32, row_end: i32) -> Self {
        Self {
            column_start: GridLine::Number(col_start),
            column_end: GridLine::Number(col_end),
            row_start: GridLine::Number(row_start),
            row_end: GridLine::Number(row_end),
        }
    }
}

/// Justify items (horizontal alignment in grid cells).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JustifyItems {
    #[default]
    Stretch,
    Start,
    End,
    Center,
}

/// Justify self (horizontal alignment for individual item).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JustifySelf {
    #[default]
    Auto,
    Stretch,
    Start,
    End,
    Center,
}

/// Position property values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Position {
    #[default]
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

/// Font weight values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontWeight(pub u16);

impl FontWeight {
    pub const NORMAL: FontWeight = FontWeight(400);
    pub const BOLD: FontWeight = FontWeight(700);
}

impl Default for FontWeight {
    fn default() -> Self {
        Self::NORMAL
    }
}

/// Font style values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
    Oblique,
}

/// Text alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Right,
    Center,
    Justify,
}

/// Overflow behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Overflow {
    #[default]
    Visible,
    Hidden,
    Scroll,
    Auto,
    Clip,
}

impl Overflow {
    /// Check if this overflow creates a scroll container.
    pub fn is_scrollable(self) -> bool {
        matches!(self, Overflow::Scroll | Overflow::Auto)
    }

    /// Check if content is clipped.
    pub fn clips_content(self) -> bool {
        !matches!(self, Overflow::Visible)
    }
}

/// Scroll behavior for smooth scrolling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollBehavior {
    #[default]
    Auto,
    Smooth,
}

/// Overscroll behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OverscrollBehavior {
    #[default]
    Auto,
    Contain,
    None,
}

/// Scrollbar width.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollbarWidth {
    #[default]
    Auto,
    Thin,
    None,
}

/// Scrollbar gutter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollbarGutter {
    #[default]
    Auto,
    Stable,
    BothEdges,
}

/// Text decoration line values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextDecorationLine {
    pub underline: bool,
    pub overline: bool,
    pub line_through: bool,
}

impl TextDecorationLine {
    pub const NONE: TextDecorationLine = TextDecorationLine {
        underline: false,
        overline: false,
        line_through: false,
    };

    pub const UNDERLINE: TextDecorationLine = TextDecorationLine {
        underline: true,
        overline: false,
        line_through: false,
    };

    pub const OVERLINE: TextDecorationLine = TextDecorationLine {
        underline: false,
        overline: true,
        line_through: false,
    };

    pub const LINE_THROUGH: TextDecorationLine = TextDecorationLine {
        underline: false,
        overline: false,
        line_through: true,
    };
}

/// Text decoration style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextDecorationStyle {
    #[default]
    Solid,
    Double,
    Dotted,
    Dashed,
    Wavy,
}

/// Font stretch values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontStretch {
    UltraCondensed,
    ExtraCondensed,
    Condensed,
    SemiCondensed,
    #[default]
    Normal,
    SemiExpanded,
    Expanded,
    ExtraExpanded,
    UltraExpanded,
}

impl FontStretch {
    /// Convert to DirectWrite font stretch value (1-9).
    pub fn to_dwrite_value(&self) -> u32 {
        match self {
            FontStretch::UltraCondensed => 1,
            FontStretch::ExtraCondensed => 2,
            FontStretch::Condensed => 3,
            FontStretch::SemiCondensed => 4,
            FontStretch::Normal => 5,
            FontStretch::SemiExpanded => 6,
            FontStretch::Expanded => 7,
            FontStretch::ExtraExpanded => 8,
            FontStretch::UltraExpanded => 9,
        }
    }
}

/// White space handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WhiteSpace {
    #[default]
    Normal,
    Nowrap,
    Pre,
    PreWrap,
    PreLine,
    BreakSpaces,
}

/// Word break behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WordBreak {
    #[default]
    Normal,
    BreakAll,
    KeepAll,
    BreakWord,
}

/// Vertical alignment.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum VerticalAlign {
    #[default]
    Baseline,
    Sub,
    Super,
    Top,
    TextTop,
    Middle,
    Bottom,
    TextBottom,
    Length(f32),
}

/// Writing mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WritingMode {
    #[default]
    HorizontalTb,
    VerticalRl,
    VerticalLr,
}

/// Text transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextTransform {
    #[default]
    None,
    Capitalize,
    Uppercase,
    Lowercase,
}

/// Direction for bidi text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}

// ==================== Transform Types ====================

/// A single 2D transform operation.
#[derive(Debug, Clone, PartialEq)]
pub enum TransformOp {
    /// translate(x, y)
    Translate(Length, Length),
    /// translateX(x)
    TranslateX(Length),
    /// translateY(y)
    TranslateY(Length),
    /// scale(x, y) or scale(s)
    Scale(f32, f32),
    /// scaleX(s)
    ScaleX(f32),
    /// scaleY(s)
    ScaleY(f32),
    /// rotate(angle) - angle in degrees
    Rotate(f32),
    /// skewX(angle) - angle in degrees
    SkewX(f32),
    /// skewY(angle) - angle in degrees
    SkewY(f32),
    /// skew(x, y) - angles in degrees
    Skew(f32, f32),
    /// matrix(a, b, c, d, e, f) - 2D affine transform
    Matrix(f32, f32, f32, f32, f32, f32),
}

/// A list of transform operations (applied in order).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TransformList {
    pub ops: Vec<TransformOp>,
}

impl TransformList {
    /// Create an empty (identity) transform list.
    pub fn none() -> Self {
        Self { ops: Vec::new() }
    }

    /// Check if this is the identity transform.
    pub fn is_identity(&self) -> bool {
        self.ops.is_empty()
    }

    /// Compute the 3x3 affine transform matrix.
    /// Returns [a, b, c, d, e, f] where the matrix is:
    /// | a c e |
    /// | b d f |
    /// | 0 0 1 |
    pub fn to_matrix(&self, container_width: f32, container_height: f32) -> [f32; 6] {
        let mut result = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]; // Identity

        for op in &self.ops {
            let m = match op {
                TransformOp::Translate(x, y) => {
                    let tx = x.to_px(16.0, 16.0, container_width);
                    let ty = y.to_px(16.0, 16.0, container_height);
                    [1.0, 0.0, 0.0, 1.0, tx, ty]
                }
                TransformOp::TranslateX(x) => {
                    let tx = x.to_px(16.0, 16.0, container_width);
                    [1.0, 0.0, 0.0, 1.0, tx, 0.0]
                }
                TransformOp::TranslateY(y) => {
                    let ty = y.to_px(16.0, 16.0, container_height);
                    [1.0, 0.0, 0.0, 1.0, 0.0, ty]
                }
                TransformOp::Scale(sx, sy) => [*sx, 0.0, 0.0, *sy, 0.0, 0.0],
                TransformOp::ScaleX(s) => [*s, 0.0, 0.0, 1.0, 0.0, 0.0],
                TransformOp::ScaleY(s) => [1.0, 0.0, 0.0, *s, 0.0, 0.0],
                TransformOp::Rotate(deg) => {
                    let rad = deg.to_radians();
                    let cos = rad.cos();
                    let sin = rad.sin();
                    [cos, sin, -sin, cos, 0.0, 0.0]
                }
                TransformOp::SkewX(deg) => {
                    let tan = deg.to_radians().tan();
                    [1.0, 0.0, tan, 1.0, 0.0, 0.0]
                }
                TransformOp::SkewY(deg) => {
                    let tan = deg.to_radians().tan();
                    [1.0, tan, 0.0, 1.0, 0.0, 0.0]
                }
                TransformOp::Skew(dx, dy) => {
                    let tan_x = dx.to_radians().tan();
                    let tan_y = dy.to_radians().tan();
                    [1.0, tan_y, tan_x, 1.0, 0.0, 0.0]
                }
                TransformOp::Matrix(a, b, c, d, e, f) => [*a, *b, *c, *d, *e, *f],
            };

            // Multiply: result = result * m
            result = multiply_matrices(result, m);
        }

        result
    }
}

/// Multiply two 2D affine matrices.
fn multiply_matrices(a: [f32; 6], b: [f32; 6]) -> [f32; 6] {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}

/// Transform origin (default: 50% 50%).
#[derive(Debug, Clone, PartialEq)]
pub struct TransformOrigin {
    pub x: Length,
    pub y: Length,
}

impl Default for TransformOrigin {
    fn default() -> Self {
        Self {
            x: Length::Percent(50.0),
            y: Length::Percent(50.0),
        }
    }
}

// ==================== Animation/Transition Types ====================

/// Animation timing function.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum TimingFunction {
    #[default]
    Ease,
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    StepStart,
    StepEnd,
    Steps(u32, bool), // (count, jump_start)
    CubicBezier(f32, f32, f32, f32),
}

/// Animation fill mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnimationFillMode {
    #[default]
    None,
    Forwards,
    Backwards,
    Both,
}

/// Animation play state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnimationPlayState {
    #[default]
    Running,
    Paused,
}

/// Animation direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnimationDirection {
    #[default]
    Normal,
    Reverse,
    Alternate,
    AlternateReverse,
}

/// Animation iteration count.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum AnimationIterationCount {
    #[default]
    One,
    Infinite,
    Count(f32),
}

/// Box sizing model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoxSizing {
    #[default]
    ContentBox,
    BorderBox,
}

/// Background clip mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackgroundClip {
    #[default]
    BorderBox,
    PaddingBox,
    ContentBox,
    /// Clip to text (for gradient text effects).
    Text,
}

/// Computed style for an element.
#[derive(Debug, Clone, Default)]
pub struct ComputedStyle {
    // Box model
    pub display: Display,
    pub position: Position,
    pub width: Length,
    pub height: Length,
    pub min_width: Length,
    pub min_height: Length,
    pub max_width: Length,
    pub max_height: Length,
    pub aspect_ratio: Option<f32>,  // width / height ratio

    // Margin
    pub margin_top: Length,
    pub margin_right: Length,
    pub margin_bottom: Length,
    pub margin_left: Length,

    // Padding
    pub padding_top: Length,
    pub padding_right: Length,
    pub padding_bottom: Length,
    pub padding_left: Length,

    // Border
    pub border_top_width: Length,
    pub border_right_width: Length,
    pub border_bottom_width: Length,
    pub border_left_width: Length,
    pub border_top_color: Color,
    pub border_right_color: Color,
    pub border_bottom_color: Color,
    pub border_left_color: Color,

    // Border radius (for rounded corners)
    pub border_top_left_radius: Length,
    pub border_top_right_radius: Length,
    pub border_bottom_right_radius: Length,
    pub border_bottom_left_radius: Length,

    // Colors
    pub color: Color,
    pub background_color: Color,
    pub background_gradient: Option<Gradient>,

    // Typography - Basic
    pub font_size: Length,
    pub font_weight: FontWeight,
    pub font_style: FontStyle,
    pub font_family: String,
    pub line_height: f32,
    pub text_align: TextAlign,

    // Typography - Advanced
    pub font_stretch: FontStretch,
    pub letter_spacing: Length,
    pub word_spacing: Length,
    pub text_indent: Length,
    pub text_decoration_line: TextDecorationLine,
    pub text_decoration_color: Option<Color>,
    pub text_decoration_style: TextDecorationStyle,
    pub text_decoration_thickness: Length,
    pub text_transform: TextTransform,
    pub white_space: WhiteSpace,
    pub word_break: WordBreak,
    pub vertical_align: VerticalAlign,
    pub writing_mode: WritingMode,
    pub direction: Direction,

    // Positioning offsets
    pub top: Option<Length>,
    pub right: Option<Length>,
    pub bottom: Option<Length>,
    pub left: Option<Length>,
    pub z_index: i32,

    // Transforms
    pub transform: TransformList,
    pub transform_origin: TransformOrigin,

    // Transitions (parsed but not executed during parity capture)
    pub transition_property: String,
    pub transition_duration: f32, // seconds
    pub transition_timing_function: TimingFunction,
    pub transition_delay: f32, // seconds

    // Animations (parsed but not executed during parity capture)
    pub animation_name: String,
    pub animation_duration: f32, // seconds
    pub animation_timing_function: TimingFunction,
    pub animation_delay: f32, // seconds
    pub animation_iteration_count: AnimationIterationCount,
    pub animation_direction: AnimationDirection,
    pub animation_fill_mode: AnimationFillMode,
    pub animation_play_state: AnimationPlayState,

    // Box sizing
    pub box_sizing: BoxSizing,

    // Visual
    pub opacity: f32,
    pub overflow_x: Overflow,
    pub overflow_y: Overflow,
    
    // Box shadows (multiple shadows supported)
    pub box_shadows: Vec<BoxShadow>,
    
    // Image/replaced element
    pub image_url: Option<String>,
    pub object_fit: String,  // "fill", "contain", "cover", "none", "scale-down"
    pub object_position: (f32, f32),

    // Flexbox Container
    pub flex_direction: FlexDirection,
    pub flex_wrap: FlexWrap,
    pub justify_content: JustifyContent,
    pub align_items: AlignItems,
    pub align_content: AlignContent,
    pub row_gap: Length,
    pub column_gap: Length,

    // Flexbox Item
    pub order: i32,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: FlexBasis,
    pub align_self: AlignSelf,

    // Scrolling
    pub scroll_behavior: ScrollBehavior,
    pub overscroll_behavior_x: OverscrollBehavior,
    pub overscroll_behavior_y: OverscrollBehavior,
    pub scrollbar_width: ScrollbarWidth,
    pub scrollbar_gutter: ScrollbarGutter,
    pub scrollbar_color: Option<(Color, Color)>, // (thumb, track)

    // Grid Container
    pub grid_template_columns: GridTemplate,
    pub grid_template_rows: GridTemplate,
    pub grid_template_areas: Option<GridTemplateAreas>,
    pub grid_auto_columns: TrackSize,
    pub grid_auto_rows: TrackSize,
    pub grid_auto_flow: GridAutoFlow,

    // Grid Item
    pub grid_column_start: GridLine,
    pub grid_column_end: GridLine,
    pub grid_row_start: GridLine,
    pub grid_row_end: GridLine,

    // Grid Alignment (also used by Flexbox)
    pub justify_items: JustifyItems,
    pub justify_self: JustifySelf,

    // Pseudo-element content
    /// The `content` property for ::before/::after pseudo-elements.
    /// None means no content (element not rendered).
    /// Some("") means empty content (element rendered but empty).
    /// Some("text") means text content.
    pub content: Option<String>,

    // Background clip for gradient text
    pub background_clip: BackgroundClip,
    pub webkit_text_fill_color: Option<Color>,
}

impl ComputedStyle {
    /// Create default style.
    pub fn new() -> Self {
        Self {
            font_size: Length::Px(16.0),
            line_height: 1.2,
            opacity: 1.0,
            color: Color::BLACK,
            background_color: Color::TRANSPARENT,
            font_family: "sans-serif".to_string(),
            text_decoration_line: TextDecorationLine::NONE,
            text_decoration_color: None,
            text_decoration_thickness: Length::Auto,
            // Flexbox item defaults
            flex_shrink: 1.0, // Default is 1, not 0
            // Width/height defaults to auto (fill available space)
            width: Length::Auto,
            height: Length::Auto,
            min_width: Length::Zero,
            min_height: Length::Zero,
            max_width: Length::Auto, // No max constraint
            max_height: Length::Auto,
            // Image/replaced element defaults
            image_url: None,
            object_fit: "contain".to_string(),
            object_position: (0.5, 0.5), // center center
            ..Default::default()
        }
    }

    /// Create style with inheritance from parent.
    pub fn inherit_from(parent: &ComputedStyle) -> Self {
        Self {
            // Inherited properties
            color: parent.color,
            font_size: parent.font_size.clone(),
            font_weight: parent.font_weight,
            font_style: parent.font_style,
            font_stretch: parent.font_stretch,
            font_family: parent.font_family.clone(),
            line_height: parent.line_height,
            text_align: parent.text_align,
            letter_spacing: parent.letter_spacing.clone(),
            word_spacing: parent.word_spacing.clone(),
            text_indent: parent.text_indent.clone(),
            text_transform: parent.text_transform,
            white_space: parent.white_space,
            word_break: parent.word_break,
            direction: parent.direction,
            writing_mode: parent.writing_mode,

            // Text decoration is NOT inherited (each element sets its own)
            text_decoration_line: TextDecorationLine::NONE,
            text_decoration_color: None,
            text_decoration_style: TextDecorationStyle::Solid,
            text_decoration_thickness: Length::Auto,

            // Non-inherited get defaults
            ..Default::default()
        }
    }
}

/// CSS property value (unparsed or parsed).
#[derive(Debug, Clone)]
pub enum PropertyValue {
    /// Inherit from parent.
    Inherit,
    /// Initial value.
    Initial,
    /// Specific value.
    Specified(String),
}

/// A CSS declaration (property: value).
#[derive(Debug, Clone)]
pub struct Declaration {
    pub property: String,
    pub value: PropertyValue,
    pub important: bool,
}

/// A CSS rule (selector + declarations).
#[derive(Debug, Clone)]
pub struct Rule {
    pub selector: String,
    pub declarations: Vec<Declaration>,
}

/// A complete stylesheet.
#[derive(Debug, Default, Clone)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
}

impl Stylesheet {
    /// Create an empty stylesheet.
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Parse a CSS string into a stylesheet.
    pub fn parse(css: &str) -> Result<Self, CssError> {
        debug!(len = css.len(), "Parsing CSS");
        let ast = parse_stylesheet(css).map_err(|e| CssError::ParseError(e.to_string()))?;

        let rules = ast
            .rules
            .into_iter()
            .map(|r| Rule {
                selector: r.selector,
                declarations: r
                    .declarations
                    .into_iter()
                    .map(|d| Declaration {
                        property: d.property,
                        value: PropertyValue::Specified(d.value),
                        important: d.important,
                    })
                    .collect(),
            })
            .collect::<Vec<_>>();

        debug!(rule_count = rules.len(), "CSS parsed");
        Ok(Stylesheet { rules })
    }

    /// Get the number of rules in this stylesheet.
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }
}

/// Parse a color value.
pub fn parse_color(value: &str) -> Option<Color> {
    let value = value.trim();

    // Named colors (CSS Color Level 4)
    match value.to_lowercase().as_str() {
        "transparent" => return Some(Color::TRANSPARENT),
        "black" => return Some(Color::BLACK),
        "white" => return Some(Color::WHITE),
        "red" => return Some(Color::from_rgb(255, 0, 0)),
        "green" => return Some(Color::from_rgb(0, 128, 0)),
        "blue" => return Some(Color::from_rgb(0, 0, 255)),
        "yellow" => return Some(Color::from_rgb(255, 255, 0)),
        "gray" | "grey" => return Some(Color::from_rgb(128, 128, 128)),
        // Extended named colors
        "coral" => return Some(Color::from_rgb(255, 127, 80)),
        "orange" => return Some(Color::from_rgb(255, 165, 0)),
        "pink" => return Some(Color::from_rgb(255, 192, 203)),
        "purple" => return Some(Color::from_rgb(128, 0, 128)),
        "cyan" => return Some(Color::from_rgb(0, 255, 255)),
        "magenta" | "fuchsia" => return Some(Color::from_rgb(255, 0, 255)),
        "lime" => return Some(Color::from_rgb(0, 255, 0)),
        "navy" => return Some(Color::from_rgb(0, 0, 128)),
        "teal" => return Some(Color::from_rgb(0, 128, 128)),
        "olive" => return Some(Color::from_rgb(128, 128, 0)),
        "maroon" => return Some(Color::from_rgb(128, 0, 0)),
        "aqua" => return Some(Color::from_rgb(0, 255, 255)),
        "silver" => return Some(Color::from_rgb(192, 192, 192)),
        "lightgray" | "lightgrey" => return Some(Color::from_rgb(211, 211, 211)),
        "darkgray" | "darkgrey" => return Some(Color::from_rgb(169, 169, 169)),
        "dimgray" | "dimgrey" => return Some(Color::from_rgb(105, 105, 105)),
        "lightblue" => return Some(Color::from_rgb(173, 216, 230)),
        "lightgreen" => return Some(Color::from_rgb(144, 238, 144)),
        "lightyellow" => return Some(Color::from_rgb(255, 255, 224)),
        "lightpink" => return Some(Color::from_rgb(255, 182, 193)),
        "lightcoral" => return Some(Color::from_rgb(240, 128, 128)),
        "darkblue" => return Some(Color::from_rgb(0, 0, 139)),
        "darkgreen" => return Some(Color::from_rgb(0, 100, 0)),
        "darkred" => return Some(Color::from_rgb(139, 0, 0)),
        "gold" => return Some(Color::from_rgb(255, 215, 0)),
        "brown" => return Some(Color::from_rgb(165, 42, 42)),
        "beige" => return Some(Color::from_rgb(245, 245, 220)),
        "ivory" => return Some(Color::from_rgb(255, 255, 240)),
        "wheat" => return Some(Color::from_rgb(245, 222, 179)),
        "tan" => return Some(Color::from_rgb(210, 180, 140)),
        "khaki" => return Some(Color::from_rgb(240, 230, 140)),
        "salmon" => return Some(Color::from_rgb(250, 128, 114)),
        "tomato" => return Some(Color::from_rgb(255, 99, 71)),
        "crimson" => return Some(Color::from_rgb(220, 20, 60)),
        "indianred" => return Some(Color::from_rgb(205, 92, 92)),
        "firebrick" => return Some(Color::from_rgb(178, 34, 34)),
        "orangered" => return Some(Color::from_rgb(255, 69, 0)),
        "chocolate" => return Some(Color::from_rgb(210, 105, 30)),
        "sienna" => return Some(Color::from_rgb(160, 82, 45)),
        "peru" => return Some(Color::from_rgb(205, 133, 63)),
        "sandybrown" => return Some(Color::from_rgb(244, 164, 96)),
        "goldenrod" => return Some(Color::from_rgb(218, 165, 32)),
        "darkgoldenrod" => return Some(Color::from_rgb(184, 134, 11)),
        "lemonchiffon" => return Some(Color::from_rgb(255, 250, 205)),
        "palegoldenrod" => return Some(Color::from_rgb(238, 232, 170)),
        "greenyellow" => return Some(Color::from_rgb(173, 255, 47)),
        "chartreuse" => return Some(Color::from_rgb(127, 255, 0)),
        "lawngreen" => return Some(Color::from_rgb(124, 252, 0)),
        "springgreen" => return Some(Color::from_rgb(0, 255, 127)),
        "mediumspringgreen" => return Some(Color::from_rgb(0, 250, 154)),
        "seagreen" => return Some(Color::from_rgb(46, 139, 87)),
        "forestgreen" => return Some(Color::from_rgb(34, 139, 34)),
        "limegreen" => return Some(Color::from_rgb(50, 205, 50)),
        "palegreen" => return Some(Color::from_rgb(152, 251, 152)),
        "mediumseagreen" => return Some(Color::from_rgb(60, 179, 113)),
        "aquamarine" => return Some(Color::from_rgb(127, 255, 212)),
        "turquoise" => return Some(Color::from_rgb(64, 224, 208)),
        "mediumturquoise" => return Some(Color::from_rgb(72, 209, 204)),
        "darkturquoise" => return Some(Color::from_rgb(0, 206, 209)),
        "cadetblue" => return Some(Color::from_rgb(95, 158, 160)),
        "steelblue" => return Some(Color::from_rgb(70, 130, 180)),
        "lightsteelblue" => return Some(Color::from_rgb(176, 196, 222)),
        "powderblue" => return Some(Color::from_rgb(176, 224, 230)),
        "skyblue" => return Some(Color::from_rgb(135, 206, 235)),
        "lightskyblue" => return Some(Color::from_rgb(135, 206, 250)),
        "deepskyblue" => return Some(Color::from_rgb(0, 191, 255)),
        "dodgerblue" => return Some(Color::from_rgb(30, 144, 255)),
        "cornflowerblue" => return Some(Color::from_rgb(100, 149, 237)),
        "royalblue" => return Some(Color::from_rgb(65, 105, 225)),
        "mediumblue" => return Some(Color::from_rgb(0, 0, 205)),
        "midnightblue" => return Some(Color::from_rgb(25, 25, 112)),
        "slateblue" => return Some(Color::from_rgb(106, 90, 205)),
        "darkslateblue" => return Some(Color::from_rgb(72, 61, 139)),
        "mediumslateblue" => return Some(Color::from_rgb(123, 104, 238)),
        "mediumpurple" => return Some(Color::from_rgb(147, 112, 219)),
        "blueviolet" => return Some(Color::from_rgb(138, 43, 226)),
        "darkorchid" => return Some(Color::from_rgb(153, 50, 204)),
        "darkviolet" => return Some(Color::from_rgb(148, 0, 211)),
        "mediumorchid" => return Some(Color::from_rgb(186, 85, 211)),
        "orchid" => return Some(Color::from_rgb(218, 112, 214)),
        "plum" => return Some(Color::from_rgb(221, 160, 221)),
        "violet" => return Some(Color::from_rgb(238, 130, 238)),
        "thistle" => return Some(Color::from_rgb(216, 191, 216)),
        "lavender" => return Some(Color::from_rgb(230, 230, 250)),
        "mistyrose" => return Some(Color::from_rgb(255, 228, 225)),
        "antiquewhite" => return Some(Color::from_rgb(250, 235, 215)),
        "linen" => return Some(Color::from_rgb(250, 240, 230)),
        "oldlace" => return Some(Color::from_rgb(253, 245, 230)),
        "papayawhip" => return Some(Color::from_rgb(255, 239, 213)),
        "seashell" => return Some(Color::from_rgb(255, 245, 238)),
        "mintcream" => return Some(Color::from_rgb(245, 255, 250)),
        "slategray" | "slategrey" => return Some(Color::from_rgb(112, 128, 144)),
        "lightslategray" | "lightslategrey" => return Some(Color::from_rgb(119, 136, 153)),
        "gainsboro" => return Some(Color::from_rgb(220, 220, 220)),
        "whitesmoke" => return Some(Color::from_rgb(245, 245, 245)),
        "floralwhite" => return Some(Color::from_rgb(255, 250, 240)),
        "ghostwhite" => return Some(Color::from_rgb(248, 248, 255)),
        "honeydew" => return Some(Color::from_rgb(240, 255, 240)),
        "azure" => return Some(Color::from_rgb(240, 255, 255)),
        "aliceblue" => return Some(Color::from_rgb(240, 248, 255)),
        "snow" => return Some(Color::from_rgb(255, 250, 250)),
        "darkcyan" => return Some(Color::from_rgb(0, 139, 139)),
        "darkmagenta" => return Some(Color::from_rgb(139, 0, 139)),
        "darkorange" => return Some(Color::from_rgb(255, 140, 0)),
        "darksalmon" => return Some(Color::from_rgb(233, 150, 122)),
        "darkseagreen" => return Some(Color::from_rgb(143, 188, 143)),
        "darkslategray" | "darkslategrey" => return Some(Color::from_rgb(47, 79, 79)),
        "deeppink" => return Some(Color::from_rgb(255, 20, 147)),
        "hotpink" => return Some(Color::from_rgb(255, 105, 180)),
        "mediumvioletred" => return Some(Color::from_rgb(199, 21, 133)),
        "palevioletred" => return Some(Color::from_rgb(219, 112, 147)),
        "rosybrown" => return Some(Color::from_rgb(188, 143, 143)),
        "saddlebrown" => return Some(Color::from_rgb(139, 69, 19)),
        "yellowgreen" => return Some(Color::from_rgb(154, 205, 50)),
        "olivedrab" => return Some(Color::from_rgb(107, 142, 35)),
        "darkolivegreen" => return Some(Color::from_rgb(85, 107, 47)),
        "mediumaquamarine" => return Some(Color::from_rgb(102, 205, 170)),
        "lightcyan" => return Some(Color::from_rgb(224, 255, 255)),
        "paleturquoise" => return Some(Color::from_rgb(175, 238, 238)),
        "lightseagreen" => return Some(Color::from_rgb(32, 178, 170)),
        "cornsilk" => return Some(Color::from_rgb(255, 248, 220)),
        "blanchedalmond" => return Some(Color::from_rgb(255, 235, 205)),
        "bisque" => return Some(Color::from_rgb(255, 228, 196)),
        "navajowhite" => return Some(Color::from_rgb(255, 222, 173)),
        "moccasin" => return Some(Color::from_rgb(255, 228, 181)),
        "peachpuff" => return Some(Color::from_rgb(255, 218, 185)),
        "burlywood" => return Some(Color::from_rgb(222, 184, 135)),
        "lavenderblush" => return Some(Color::from_rgb(255, 240, 245)),
        "currentcolor" => return None, // Special case - needs context
        "inherit" => return None, // Special case - needs context
        _ => {}
    }

    // Hex colors
    if let Some(hex) = value.strip_prefix('#') {
        let (r, g, b, a) = match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
                (r, g, b, 1.0)
            }
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                (r, g, b, 1.0)
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()? as f32 / 255.0;
                (r, g, b, a)
            }
            _ => return None,
        };
        return Some(Color::new(r, g, b, a));
    }

    // rgb() / rgba()
    if value.starts_with("rgb") {
        // Simplified parsing
        let inner = value
            .trim_start_matches("rgba(")
            .trim_start_matches("rgb(")
            .trim_end_matches(')');
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() >= 3 {
            let r = parts[0].trim().parse::<u8>().ok()?;
            let g = parts[1].trim().parse::<u8>().ok()?;
            let b = parts[2].trim().parse::<u8>().ok()?;
            let a = if parts.len() >= 4 {
                parts[3].trim().parse::<f32>().ok()?
            } else {
                1.0
            };
            return Some(Color::new(r, g, b, a));
        }
    }

    // hsl() / hsla()
    if value.starts_with("hsl") {
        let inner = value
            .trim_start_matches("hsla(")
            .trim_start_matches("hsl(")
            .trim_end_matches(')');
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() >= 3 {
            let h = parts[0].trim().trim_end_matches("deg").parse::<f32>().ok()?;
            let s = parts[1].trim().trim_end_matches('%').parse::<f32>().ok()? / 100.0;
            let l = parts[2].trim().trim_end_matches('%').parse::<f32>().ok()? / 100.0;
            let a = if parts.len() >= 4 {
                parts[3].trim().parse::<f32>().ok()?
            } else {
                1.0
            };
            
            // HSL to RGB conversion
            let (r, g, b) = hsl_to_rgb(h, s, l);
            return Some(Color::new(r, g, b, a));
        }
    }

    None
}

/// Convert HSL to RGB
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    if s == 0.0 {
        // Achromatic (gray)
        let v = (l * 255.0).round() as u8;
        return (v, v, v);
    }

    let h = h / 360.0;
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;

    let r = hue_to_rgb(p, q, h + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, h);
    let b = hue_to_rgb(p, q, h - 1.0 / 3.0);

    (
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    )
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 { t += 1.0; }
    if t > 1.0 { t -= 1.0; }
    
    if t < 1.0 / 6.0 {
        return p + (q - p) * 6.0 * t;
    }
    if t < 1.0 / 2.0 {
        return q;
    }
    if t < 2.0 / 3.0 {
        return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    }
    p
}

/// Parse a length value.
pub fn parse_length(value: &str) -> Option<Length> {
    let value = value.trim();

    if value == "auto" {
        return Some(Length::Auto);
    }
    if value == "0" {
        return Some(Length::Zero);
    }

    // Handle min(), max(), clamp() CSS math functions
    if value.starts_with("min(") && value.ends_with(')') {
        let inner = &value[4..value.len() - 1];
        let args = split_css_function_args(inner);
        if args.len() >= 2 {
            let a = parse_length(args[0])?;
            let b = parse_length(args[1])?;
            return Some(Length::Min(Box::new((a, b))));
        }
        return None;
    }

    if value.starts_with("max(") && value.ends_with(')') {
        let inner = &value[4..value.len() - 1];
        let args = split_css_function_args(inner);
        if args.len() >= 2 {
            let a = parse_length(args[0])?;
            let b = parse_length(args[1])?;
            return Some(Length::Max(Box::new((a, b))));
        }
        return None;
    }

    if value.starts_with("clamp(") && value.ends_with(')') {
        let inner = &value[6..value.len() - 1];
        let args = split_css_function_args(inner);
        if args.len() >= 3 {
            let min = parse_length(args[0])?;
            let preferred = parse_length(args[1])?;
            let max = parse_length(args[2])?;
            return Some(Length::Clamp(Box::new((min, preferred, max))));
        }
        return None;
    }

    // Handle calc() - simplified support
    if value.starts_with("calc(") && value.ends_with(')') {
        // For now, try to extract a simple value from calc
        // Full calc support would require expression parsing
        let inner = &value[5..value.len() - 1].trim();
        // If it's a simple value wrapped in calc, parse it
        if let Some(len) = parse_length(inner) {
            return Some(len);
        }
        return None;
    }

    if value.ends_with("px") {
        let num = value.trim_end_matches("px").parse::<f32>().ok()?;
        return Some(Length::Px(num));
    }
    if value.ends_with("em") {
        let num = value.trim_end_matches("em").parse::<f32>().ok()?;
        return Some(Length::Em(num));
    }
    if value.ends_with("rem") {
        let num = value.trim_end_matches("rem").parse::<f32>().ok()?;
        return Some(Length::Rem(num));
    }
    if value.ends_with("vh") {
        let num = value.trim_end_matches("vh").parse::<f32>().ok()?;
        return Some(Length::Vh(num));
    }
    if value.ends_with("vw") {
        let num = value.trim_end_matches("vw").parse::<f32>().ok()?;
        return Some(Length::Vw(num));
    }
    if value.ends_with("vmin") {
        let num = value.trim_end_matches("vmin").parse::<f32>().ok()?;
        return Some(Length::Vmin(num));
    }
    if value.ends_with("vmax") {
        let num = value.trim_end_matches("vmax").parse::<f32>().ok()?;
        return Some(Length::Vmax(num));
    }
    if value.ends_with('%') {
        let num = value.trim_end_matches('%').parse::<f32>().ok()?;
        return Some(Length::Percent(num));
    }

    // Try plain number (treated as px)
    if let Ok(num) = value.parse::<f32>() {
        return Some(Length::Px(num));
    }

    None
}

/// Split CSS function arguments, handling nested parentheses.
fn split_css_function_args(args: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut depth = 0;
    let mut start = 0;

    for (i, c) in args.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                result.push(args[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }

    // Add the last argument
    let last = args[start..].trim();
    if !last.is_empty() {
        result.push(last);
    }

    result
}

/// Parse display value.
pub fn parse_display(value: &str) -> Option<Display> {
    match value.trim().to_lowercase().as_str() {
        "block" => Some(Display::Block),
        "inline" => Some(Display::Inline),
        "inline-block" => Some(Display::InlineBlock),
        "flex" => Some(Display::Flex),
        "inline-flex" => Some(Display::InlineFlex),
        "grid" => Some(Display::Grid),
        "inline-grid" => Some(Display::InlineGrid),
        "none" => Some(Display::None),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_color_hex() {
        assert_eq!(parse_color("#fff"), Some(Color::from_rgb(255, 255, 255)));
        assert_eq!(parse_color("#000000"), Some(Color::BLACK));
        assert_eq!(parse_color("#ff0000"), Some(Color::from_rgb(255, 0, 0)));
    }

    #[test]
    fn test_parse_color_named() {
        assert_eq!(parse_color("red"), Some(Color::from_rgb(255, 0, 0)));
        assert_eq!(parse_color("black"), Some(Color::BLACK));
        assert_eq!(parse_color("transparent"), Some(Color::TRANSPARENT));
    }

    #[test]
    fn test_parse_length() {
        assert_eq!(parse_length("10px"), Some(Length::Px(10.0)));
        assert_eq!(parse_length("1.5em"), Some(Length::Em(1.5)));
        assert_eq!(parse_length("50%"), Some(Length::Percent(50.0)));
        assert_eq!(parse_length("auto"), Some(Length::Auto));
    }

    #[test]
    fn test_parse_length_math_functions() {
        // Test min()
        let min_result = parse_length("min(700px, 100%)");
        assert!(min_result.is_some());
        if let Some(Length::Min(pair)) = min_result {
            assert_eq!(pair.0, Length::Px(700.0));
            assert_eq!(pair.1, Length::Percent(100.0));
        } else {
            panic!("Expected Length::Min");
        }

        // Test max()
        let max_result = parse_length("max(50%, 300px)");
        assert!(max_result.is_some());
        if let Some(Length::Max(pair)) = max_result {
            assert_eq!(pair.0, Length::Percent(50.0));
            assert_eq!(pair.1, Length::Px(300.0));
        } else {
            panic!("Expected Length::Max");
        }

        // Test clamp()
        let clamp_result = parse_length("clamp(200px, 50%, 800px)");
        assert!(clamp_result.is_some());
        if let Some(Length::Clamp(triple)) = clamp_result {
            assert_eq!(triple.0, Length::Px(200.0));
            assert_eq!(triple.1, Length::Percent(50.0));
            assert_eq!(triple.2, Length::Px(800.0));
        } else {
            panic!("Expected Length::Clamp");
        }
    }

    #[test]
    fn test_parse_length_viewport_units() {
        assert_eq!(parse_length("100vh"), Some(Length::Vh(100.0)));
        assert_eq!(parse_length("50vw"), Some(Length::Vw(50.0)));
        assert_eq!(parse_length("10vmin"), Some(Length::Vmin(10.0)));
        assert_eq!(parse_length("20vmax"), Some(Length::Vmax(20.0)));
    }

    #[test]
    fn test_parse_stylesheet() {
        let css = r#"
            body {
                color: black;
            }
            .container {
                width: 100%;
            }
        "#;

        let stylesheet = Stylesheet::parse(css).unwrap();
        assert!(stylesheet.rules.len() >= 2);
    }

    #[test]
    fn test_computed_style_inherit() {
        let parent = ComputedStyle {
            color: Color::from_rgb(255, 0, 0),
            font_size: Length::Px(20.0),
            ..Default::default()
        };

        let child = ComputedStyle::inherit_from(&parent);
        assert_eq!(child.color, parent.color);
        assert_eq!(child.font_size, parent.font_size);
        // Non-inherited properties should be default
        assert_eq!(child.display, Display::Block);
    }
}
