//! # Text Rendering Module
//!
//! Comprehensive text rendering support using DirectWrite on Windows and Core Text on macOS.
//! Provides font fallback, text shaping, text decoration, and line height calculation.
//!
//! ## Features
//!
//! - **Font Fallback Chain**: Automatic fallback for missing glyphs
//! - **Complex Script Support**: Full Unicode shaping via DirectWrite/Core Text
//! - **Text Decoration**: Underline, strikethrough, overline
//! - **Line Height**: Proper line-height calculation with various units
//! - **Font Variants**: Bold, italic, weights, stretches
//! - **Metrics**: Accurate glyph and line metrics
//! - **Bidirectional Text**: Support for mixed LTR/RTL text via UAX #9
//! - **Line Breaking**: Text wrapping with CSS word-break support via UAX #14

use rustkit_css::{
    Color, Direction as CssDirection, FontStretch, FontStyle, FontWeight, Length,
    TextDecorationLine, TextDecorationStyle, TextTransform, WhiteSpace,
    WordBreak as CssWordBreak,
};
use rustkit_text::bidi::{BidiInfo, Direction as BidiDirection};
use rustkit_text::line_break::{LineBreaker, WordBreak as LineBreakWordBreak, OverflowWrap};
use std::collections::HashMap;
use std::sync::RwLock;
use thiserror::Error;

#[cfg(windows)]
use std::sync::Arc;
#[cfg(windows)]
use rustkit_text::{FontCollection as RkFontCollection, FontStretch as RkFontStretch, FontStyle as RkFontStyle, FontWeight as RkFontWeight};

#[cfg(target_os = "macos")]
use core_foundation::base::TCFType;
#[cfg(target_os = "macos")]
use core_graphics::geometry::CGSize;
#[cfg(target_os = "macos")]
use core_text::font as ct_font;

/// Errors that can occur in text operations.
#[derive(Error, Debug)]
pub enum TextError {
    #[error("Font not found: {0}")]
    FontNotFound(String),

    #[error("Text shaping failed: {0}")]
    ShapingFailed(String),

    #[error("Font loading failed: {0}")]
    FontLoadFailed(String),

    #[error("DirectWrite error: {0}")]
    DirectWriteError(String),
}

/// A font family with fallback chain.
#[derive(Debug, Clone)]
pub struct FontFamilyChain {
    /// Primary font family name.
    pub primary: String,
    /// Fallback font families in order.
    pub fallbacks: Vec<String>,
}

impl FontFamilyChain {
    /// Create a new font family chain.
    pub fn new(primary: impl Into<String>) -> Self {
        Self {
            primary: primary.into(),
            fallbacks: Vec::new(),
        }
    }

    /// Add a fallback font.
    pub fn with_fallback(mut self, family: impl Into<String>) -> Self {
        self.fallbacks.push(family.into());
        self
    }

    /// Get all families in order (primary + fallbacks).
    pub fn all_families(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.primary.as_str()).chain(self.fallbacks.iter().map(|s| s.as_str()))
    }

    /// Create default font chain for sans-serif.
    #[cfg(target_os = "macos")]
    pub fn sans_serif() -> Self {
        Self::new("SF Pro")
            .with_fallback(".AppleSystemUIFont")
            .with_fallback("Helvetica Neue")
            .with_fallback("Helvetica")
            .with_fallback("Arial")
            .with_fallback("PingFang SC")
            .with_fallback("Hiragino Sans")
            .with_fallback("sans-serif")
    }
    
    /// Create default font chain for sans-serif.
    #[cfg(not(target_os = "macos"))]
    pub fn sans_serif() -> Self {
        Self::new("Segoe UI")
            .with_fallback("Arial")
            .with_fallback("Helvetica")
            .with_fallback("Noto Sans")
            .with_fallback("Noto Sans CJK SC")
            .with_fallback("Microsoft YaHei")
            .with_fallback("sans-serif")
    }

    /// Create default font chain for serif.
    #[cfg(target_os = "macos")]
    pub fn serif() -> Self {
        Self::new("New York")
            .with_fallback("Times New Roman")
            .with_fallback("Georgia")
            .with_fallback("Songti SC")
            .with_fallback("serif")
    }
    
    /// Create default font chain for serif.
    #[cfg(not(target_os = "macos"))]
    pub fn serif() -> Self {
        Self::new("Times New Roman")
            .with_fallback("Georgia")
            .with_fallback("Noto Serif")
            .with_fallback("Noto Serif CJK SC")
            .with_fallback("SimSun")
            .with_fallback("serif")
    }

    /// Create default font chain for monospace.
    #[cfg(target_os = "macos")]
    pub fn monospace() -> Self {
        Self::new("SF Mono")
            .with_fallback("Menlo")
            .with_fallback("Monaco")
            .with_fallback("Courier New")
            .with_fallback("monospace")
    }
    
    /// Create default font chain for monospace.
    #[cfg(not(target_os = "macos"))]
    pub fn monospace() -> Self {
        Self::new("Cascadia Code")
            .with_fallback("Consolas")
            .with_fallback("Courier New")
            .with_fallback("Noto Sans Mono")
            .with_fallback("monospace")
    }

    /// Create system-ui font chain (platform-specific).
    #[cfg(target_os = "macos")]
    pub fn system_ui() -> Self {
        Self::new(".AppleSystemUIFont")
            .with_fallback("SF Pro")
            .with_fallback("Helvetica Neue")
            .with_fallback("Helvetica")
            .with_fallback("Arial")
    }

    /// Create system-ui font chain (platform-specific).
    #[cfg(not(target_os = "macos"))]
    pub fn system_ui() -> Self {
        Self::new("Segoe UI")
            .with_fallback("Roboto")
            .with_fallback("Arial")
            .with_fallback("Noto Sans")
    }

    /// Resolve a CSS font-family value to a chain.
    pub fn from_css_value(value: &str) -> Self {
        let families: Vec<&str> = value
            .split(',')
            .map(|s| s.trim().trim_matches('"').trim_matches('\''))
            .collect();

        if families.is_empty() {
            return Self::sans_serif();
        }

        let primary = families[0];

        // Handle generic families
        match primary.to_lowercase().as_str() {
            "sans-serif" => Self::sans_serif(),
            "serif" => Self::serif(),
            "monospace" => Self::monospace(),
            "system-ui" | "-apple-system" | "blinkmacsystemfont" => Self::system_ui(),
            "cursive" => Self::new("Comic Sans MS")
                .with_fallback("Brush Script MT")
                .with_fallback("cursive"),
            "fantasy" => Self::new("Impact")
                .with_fallback("Papyrus")
                .with_fallback("fantasy"),
            _ => {
                let mut chain = Self::new(primary);
                for fallback in families.iter().skip(1) {
                    // Recursively handle generic families in fallback chain
                    let lower = fallback.to_lowercase();
                    if lower == "system-ui" || lower == "-apple-system" || lower == "blinkmacsystemfont" {
                        let sys_chain = Self::system_ui();
                        chain.fallbacks.push(sys_chain.primary);
                        chain.fallbacks.extend(sys_chain.fallbacks);
                    } else if lower == "sans-serif" {
                        let sans_chain = Self::sans_serif();
                        chain.fallbacks.push(sans_chain.primary);
                        chain.fallbacks.extend(sans_chain.fallbacks);
                    } else {
                        chain.fallbacks.push(fallback.to_string());
                    }
                }
                // Add platform-specific system fallbacks
                #[cfg(target_os = "macos")]
                {
                    chain.fallbacks.push(".AppleSystemUIFont".to_string());
                    chain.fallbacks.push("Helvetica".to_string());
                }
                #[cfg(not(target_os = "macos"))]
                {
                    chain.fallbacks.push("Segoe UI".to_string());
                    chain.fallbacks.push("Arial".to_string());
                }
                chain
            }
        }
    }
}

/// Text metrics from shaping.
#[derive(Debug, Clone, Default)]
pub struct TextMetrics {
    /// Total width of the text run.
    pub width: f32,
    /// Total height (ascent + descent + line gap).
    pub height: f32,
    /// Distance from baseline to top of highest glyph.
    pub ascent: f32,
    /// Distance from baseline to bottom of lowest glyph.
    pub descent: f32,
    /// Leading (line gap).
    pub leading: f32,
    /// Underline position relative to baseline.
    pub underline_offset: f32,
    /// Underline thickness.
    pub underline_thickness: f32,
    /// Strikethrough position relative to baseline.
    pub strikethrough_offset: f32,
    /// Strikethrough thickness.
    pub strikethrough_thickness: f32,
    /// Overline position relative to baseline (top of text).
    pub overline_offset: f32,
}

impl TextMetrics {
    /// Create metrics with baseline values.
    /// Ratios based on SF Pro font metrics (macOS system font).
    /// SF Pro: ~0.82 ascent, ~0.21 descent (measured from actual Core Text metrics).
    /// Previous values (0.88/0.24) were too large and caused baseline shifts.
    pub fn with_font_size(font_size: f32) -> Self {
        // Use SF Pro ratios as default - these match macOS system font better
        let ascent = font_size * 0.82;
        let descent = font_size * 0.21;
        let leading = 0.0;

        Self {
            width: 0.0,
            height: ascent + descent + leading,
            ascent,
            descent,
            leading,
            underline_offset: descent * 0.5,
            underline_thickness: font_size / 14.0,
            strikethrough_offset: -ascent * 0.35,
            strikethrough_thickness: font_size / 14.0,
            overline_offset: -ascent,
        }
    }

    /// Create metrics from a Core Text font (macOS).
    /// This provides accurate metrics directly from the font.
    #[cfg(target_os = "macos")]
    pub fn from_core_text_font(ct_font: &core_text::font::CTFont, width: f32) -> Self {
        let ascent = ct_font.ascent() as f32;
        let descent = ct_font.descent() as f32;
        let leading = ct_font.leading() as f32;
        let underline_position = ct_font.underline_position() as f32;
        let underline_thickness = ct_font.underline_thickness() as f32;
        let x_height = ct_font.x_height() as f32;
        let strikethrough_offset = x_height * 0.5;

        Self {
            width,
            height: ascent + descent + leading,
            ascent,
            descent,
            leading,
            underline_offset: underline_position,
            underline_thickness,
            strikethrough_offset,
            strikethrough_thickness: underline_thickness,
            overline_offset: -ascent,
        }
    }
}

/// A positioned glyph in a text run.
#[derive(Debug, Clone)]
pub struct PositionedGlyph {
    /// Glyph ID (font-specific).
    pub glyph_id: u16,
    /// X offset from the start of the run.
    pub x: f32,
    /// Y offset from the baseline.
    pub y: f32,
    /// Advance width.
    pub advance: f32,
    /// The character this glyph represents.
    pub character: char,
    /// Cluster index for multi-glyph characters.
    pub cluster: u32,
}

/// A shaped text run.
#[derive(Debug, Clone)]
pub struct ShapedRun {
    /// The original text.
    pub text: String,
    /// Positioned glyphs.
    pub glyphs: Vec<PositionedGlyph>,
    /// Font family used.
    pub font_family: String,
    /// Font weight.
    pub font_weight: FontWeight,
    /// Font style.
    pub font_style: FontStyle,
    /// Font stretch.
    pub font_stretch: FontStretch,
    /// Font size in pixels.
    pub font_size: f32,
    /// Text metrics.
    pub metrics: TextMetrics,
    /// Text direction (LTR or RTL).
    pub direction: TextDirection,
}

/// Text direction for a shaped run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextDirection {
    /// Left-to-right (Latin, Greek, Cyrillic, etc.)
    #[default]
    Ltr,
    /// Right-to-left (Arabic, Hebrew, etc.)
    Rtl,
}

impl TextDirection {
    /// Convert from CSS Direction.
    pub fn from_css(direction: CssDirection) -> Self {
        match direction {
            CssDirection::Ltr => TextDirection::Ltr,
            CssDirection::Rtl => TextDirection::Rtl,
        }
    }

    /// Convert from bidi Direction.
    pub fn from_bidi(direction: BidiDirection) -> Self {
        match direction {
            BidiDirection::Ltr => TextDirection::Ltr,
            BidiDirection::Rtl => TextDirection::Rtl,
        }
    }

    /// Convert to bidi Direction.
    pub fn to_bidi(self) -> BidiDirection {
        match self {
            TextDirection::Ltr => BidiDirection::Ltr,
            TextDirection::Rtl => BidiDirection::Rtl,
        }
    }

    /// Check if this is left-to-right.
    pub fn is_ltr(self) -> bool {
        self == TextDirection::Ltr
    }

    /// Check if this is right-to-left.
    pub fn is_rtl(self) -> bool {
        self == TextDirection::Rtl
    }
}

impl ShapedRun {
    /// Get the total width of the run.
    pub fn width(&self) -> f32 {
        self.metrics.width
    }

    /// Get the height of the run.
    pub fn height(&self) -> f32 {
        self.metrics.height
    }

    /// Apply letter-spacing to the shaped run.
    /// Letter-spacing adds extra space after each character.
    pub fn apply_letter_spacing(&mut self, letter_spacing: f32) {
        if letter_spacing == 0.0 || self.glyphs.is_empty() {
            return;
        }

        let mut accumulated_offset = 0.0;
        for glyph in &mut self.glyphs {
            // Shift glyph position by accumulated offset
            glyph.x += accumulated_offset;
            // Add letter-spacing to advance
            glyph.advance += letter_spacing;
            accumulated_offset += letter_spacing;
        }

        // Update total width
        self.metrics.width += accumulated_offset;
    }

    /// Apply word-spacing to the shaped run.
    /// Word-spacing adds extra space to whitespace characters.
    pub fn apply_word_spacing(&mut self, word_spacing: f32) {
        if word_spacing == 0.0 || self.glyphs.is_empty() {
            return;
        }

        let mut accumulated_offset = 0.0;
        for glyph in &mut self.glyphs {
            // Shift glyph position by accumulated offset
            glyph.x += accumulated_offset;

            // Add word-spacing to whitespace characters
            if glyph.character.is_whitespace() {
                glyph.advance += word_spacing;
                accumulated_offset += word_spacing;
            }
        }

        // Update total width
        self.metrics.width += accumulated_offset;
    }

    /// Apply both letter-spacing and word-spacing.
    pub fn apply_spacing(&mut self, letter_spacing: f32, word_spacing: f32) {
        // Apply word-spacing first, then letter-spacing
        // This matches CSS specification behavior
        self.apply_word_spacing(word_spacing);
        self.apply_letter_spacing(letter_spacing);
    }
}

/// Text decoration rendering information.
#[derive(Debug, Clone)]
pub struct TextDecoration {
    /// Decoration lines to draw.
    pub lines: TextDecorationLine,
    /// Decoration color (defaults to text color).
    pub color: Option<Color>,
    /// Decoration style.
    pub style: TextDecorationStyle,
    /// Decoration thickness (auto uses font metrics).
    pub thickness: Option<f32>,
}

impl TextDecoration {
    /// Create decoration from CSS properties.
    pub fn from_style(
        lines: TextDecorationLine,
        color: Option<Color>,
        style: TextDecorationStyle,
        thickness: Length,
        font_size: f32,
    ) -> Self {
        let thickness_px = match thickness {
            Length::Auto => None,
            Length::Px(px) => Some(px),
            Length::Em(em) => Some(em * font_size),
            Length::Rem(rem) => Some(rem * 16.0),
            _ => None,
        };

        Self {
            lines,
            color,
            style,
            thickness: thickness_px,
        }
    }

    /// Check if any decorations are active.
    pub fn has_decorations(&self) -> bool {
        self.lines.underline || self.lines.overline || self.lines.line_through
    }
}

/// Line height calculation modes.
#[derive(Debug, Clone, Copy)]
pub enum LineHeight {
    /// Normal line height (use font metrics).
    Normal,
    /// Multiplier (e.g., 1.5 = 150% of font size).
    Number(f32),
    /// Absolute length in pixels.
    Length(f32),
}

impl LineHeight {
    /// Parse from CSS line-height value.
    pub fn from_css(value: f32, is_number: bool) -> Self {
        if is_number {
            LineHeight::Number(value)
        } else {
            LineHeight::Length(value)
        }
    }

    /// Compute the actual line height in pixels.
    pub fn compute(&self, font_size: f32, metrics: &TextMetrics) -> f32 {
        match self {
            LineHeight::Normal => metrics.height,
            LineHeight::Number(n) => font_size * n,
            LineHeight::Length(px) => *px,
        }
    }

    /// Compute leading (extra space above/below text).
    pub fn compute_leading(&self, font_size: f32, metrics: &TextMetrics) -> f32 {
        let line_height = self.compute(font_size, metrics);
        let content_height = metrics.ascent + metrics.descent;
        (line_height - content_height).max(0.0)
    }
}

/// Apply text transform to a string.
pub fn apply_text_transform(text: &str, transform: TextTransform) -> String {
    match transform {
        TextTransform::None => text.to_string(),
        TextTransform::Uppercase => text.to_uppercase(),
        TextTransform::Lowercase => text.to_lowercase(),
        TextTransform::Capitalize => {
            let mut result = String::with_capacity(text.len());
            let mut capitalize_next = true;
            for c in text.chars() {
                if c.is_whitespace() {
                    capitalize_next = true;
                    result.push(c);
                } else if capitalize_next {
                    result.extend(c.to_uppercase());
                    capitalize_next = false;
                } else {
                    result.push(c);
                }
            }
            result
        }
    }
}

/// Collapse whitespace according to white-space property.
pub fn collapse_whitespace(text: &str, white_space: WhiteSpace) -> String {
    match white_space {
        WhiteSpace::Normal | WhiteSpace::Nowrap => {
            // Collapse sequences of whitespace to single space
            let mut result = String::with_capacity(text.len());
            let mut last_was_space = false;
            for c in text.chars() {
                if c.is_whitespace() {
                    if !last_was_space {
                        result.push(' ');
                        last_was_space = true;
                    }
                } else {
                    result.push(c);
                    last_was_space = false;
                }
            }
            result.trim().to_string()
        }
        WhiteSpace::Pre | WhiteSpace::PreWrap | WhiteSpace::BreakSpaces => {
            // Preserve whitespace
            text.to_string()
        }
        WhiteSpace::PreLine => {
            // Collapse spaces but preserve newlines
            let mut result = String::with_capacity(text.len());
            let mut last_was_space = false;
            for c in text.chars() {
                if c == '\n' {
                    result.push('\n');
                    last_was_space = false;
                } else if c.is_whitespace() {
                    if !last_was_space {
                        result.push(' ');
                        last_was_space = true;
                    }
                } else {
                    result.push(c);
                    last_was_space = false;
                }
            }
            result
        }
    }
}

/// Font cache for reusing font objects.
#[derive(Default)]
pub struct FontCache {
    #[cfg(windows)]
    fonts: RwLock<HashMap<FontKey, Arc<FontCacheEntry>>>,
    #[cfg(not(windows))]
    _fonts: RwLock<HashMap<FontKey, ()>>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct FontKey {
    family: String,
    weight: u16,
    style: u8,
    stretch: u8,
}

#[cfg(windows)]
struct FontCacheEntry {
    #[allow(dead_code)]
    font_face: rustkit_text::FontFace,
    metrics: TextMetrics,
}

impl FontCache {
    /// Create a new font cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get font metrics for a given font configuration.
    #[cfg(windows)]
    pub fn get_metrics(
        &self,
        family: &str,
        weight: FontWeight,
        style: FontStyle,
        stretch: FontStretch,
        size: f32,
    ) -> Result<TextMetrics, TextError> {
        let key = FontKey {
            family: family.to_string(),
            weight: weight.0,
            style: match style {
                FontStyle::Normal => 0,
                FontStyle::Italic => 1,
                FontStyle::Oblique => 2,
            },
            stretch: stretch.to_dwrite_value() as u8,
        };

        // Try cache first
        {
            let cache = self.fonts.read().unwrap();
            if let Some(entry) = cache.get(&key) {
                let mut metrics = entry.metrics.clone();
                // Scale metrics to requested size
                let scale = size / 16.0;
                metrics.width *= scale;
                metrics.height *= scale;
                metrics.ascent *= scale;
                metrics.descent *= scale;
                metrics.leading *= scale;
                metrics.underline_offset *= scale;
                metrics.underline_thickness *= scale;
                metrics.strikethrough_offset *= scale;
                metrics.strikethrough_thickness *= scale;
                metrics.overline_offset *= scale;
                return Ok(metrics);
            }
        }

        // Load font and get metrics
        self.load_font_metrics(family, weight, style, stretch, size)
    }

    #[cfg(windows)]
    fn load_font_metrics(
        &self,
        family: &str,
        weight: FontWeight,
        style: FontStyle,
        stretch: FontStretch,
        size: f32,
    ) -> Result<TextMetrics, TextError> {
        let collection = RkFontCollection::system().map_err(|e| TextError::DirectWriteError(e.to_string()))?;

        // Try to find the font family
        let dw_family = collection
            .font_family_by_name(family)
            .map_err(|e| TextError::DirectWriteError(e.to_string()))?
            .or_else(|| {
                collection
                    .font_family_by_name("Segoe UI")
                    .ok()
                    .flatten()
            });

        if let Some(family) = dw_family {
            let dw_weight = RkFontWeight::from_u32(weight.0 as u32);
            let dw_style = match style {
                FontStyle::Normal => RkFontStyle::Normal,
                FontStyle::Italic => RkFontStyle::Italic,
                FontStyle::Oblique => RkFontStyle::Oblique,
            };
            let dw_stretch = RkFontStretch::from_u32(stretch.to_dwrite_value());

            if let Ok(font) = family.first_matching_font(dw_weight, dw_stretch, dw_style) {
                let face = font
                    .create_font_face()
                    .map_err(|e| TextError::DirectWriteError(e.to_string()))?;
                let design_metrics = face
                    .metrics()
                    .map_err(|e| TextError::DirectWriteError(e.to_string()))?;

                // Convert design units to pixels (DWRITE uses camelCase)
                let units_per_em = design_metrics.design_units_per_em as f32;
                let scale = size / units_per_em;

                let ascent = design_metrics.ascent as f32 * scale;
                let descent = design_metrics.descent as f32 * scale;
                let leading = design_metrics.line_gap as f32 * scale;

                return Ok(TextMetrics {
                    width: 0.0,
                    height: ascent + descent + leading,
                    ascent,
                    descent,
                    leading,
                    underline_offset: design_metrics.underline_position as f32 * scale,
                    underline_thickness: design_metrics.underline_thickness as f32 * scale,
                    strikethrough_offset: design_metrics.strikethrough_position as f32 * scale,
                    strikethrough_thickness: design_metrics.strikethrough_thickness as f32 * scale,
                    overline_offset: -ascent,
                });
            }
        }

        // Fallback to computed metrics
        Ok(TextMetrics::with_font_size(size))
    }

    #[cfg(target_os = "macos")]
    pub fn get_metrics(
        &self,
        family: &str,
        weight: FontWeight,
        style: FontStyle,
        _stretch: FontStretch,
        size: f32,
    ) -> Result<TextMetrics, TextError> {
        // Try to get real Core Text metrics for the requested font
        if let Ok(font) = TextShaper::create_ct_font_with_traits(family, size, weight.0, style == FontStyle::Italic) {
            return Ok(TextMetrics::from_core_text_font(&font, 0.0));
        }

        // Try system font as fallback
        if let Ok(font) = ct_font::new_from_name("Helvetica", size as f64) {
            return Ok(TextMetrics::from_core_text_font(&font, 0.0));
        }

        // Ultimate fallback to computed metrics
        Ok(TextMetrics::with_font_size(size))
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    pub fn get_metrics(
        &self,
        _family: &str,
        _weight: FontWeight,
        _style: FontStyle,
        _stretch: FontStretch,
        size: f32,
    ) -> Result<TextMetrics, TextError> {
        // Fallback metrics for other platforms (Linux, etc.)
        Ok(TextMetrics::with_font_size(size))
    }
}

/// Text shaper for complex text layout.
pub struct TextShaper {
    #[allow(dead_code)]
    cache: FontCache,
}

impl TextShaper {
    /// Create a new text shaper.
    pub fn new() -> Self {
        Self {
            cache: FontCache::new(),
        }
    }

    /// Shape text with the given style.
    #[cfg(windows)]
    pub fn shape(
        &self,
        text: &str,
        font_chain: &FontFamilyChain,
        weight: FontWeight,
        style: FontStyle,
        stretch: FontStretch,
        size: f32,
    ) -> Result<ShapedRun, TextError> {
        if text.is_empty() {
            return Ok(ShapedRun {
                text: String::new(),
                glyphs: Vec::new(),
                font_family: font_chain.primary.clone(),
                font_weight: weight,
                font_style: style,
                font_stretch: stretch,
                font_size: size,
                metrics: TextMetrics::with_font_size(size),
                direction: TextDirection::Ltr,
            });
        }

        let collection = RkFontCollection::system().map_err(|e| TextError::DirectWriteError(e.to_string()))?;

        // Find first available font in chain
        let mut font_family_name = font_chain.primary.clone();
        let mut found_font = None;

        for family_name in font_chain.all_families() {
            if let Ok(Some(family)) = collection.font_family_by_name(family_name) {
                let dw_weight = RkFontWeight::from_u32(weight.0 as u32);
                let dw_style = match style {
                    FontStyle::Normal => RkFontStyle::Normal,
                    FontStyle::Italic => RkFontStyle::Italic,
                    FontStyle::Oblique => RkFontStyle::Oblique,
                };
                let dw_stretch = RkFontStretch::from_u32(stretch.to_dwrite_value());

                if let Ok(font) = family.first_matching_font(dw_weight, dw_stretch, dw_style) {
                    font_family_name = family_name.to_string();
                    found_font = Some(font);
                    break;
                }
            }
        }

        // If we found a font, use DirectWrite for accurate shaping
        if let Some(font) = found_font {
            let face = font
                .create_font_face()
                .map_err(|e| TextError::DirectWriteError(e.to_string()))?;
            let design_metrics = face
                .metrics()
                .map_err(|e| TextError::DirectWriteError(e.to_string()))?;

            let units_per_em = design_metrics.design_units_per_em as f32;
            let scale = size / units_per_em;

            // Get glyph indices - handle Result
            let text_chars: Vec<char> = text.chars().collect();
            let codepoints: Vec<u32> = text_chars.iter().map(|c| *c as u32).collect();

            // Try to get glyph indices, fall back to simple shaping if it fails
            if let Ok(glyph_indices) = face.glyph_indices(&codepoints) {
                // Try to get glyph metrics
                if let Ok(glyph_metrics) = face.design_glyph_metrics(&glyph_indices, false) {
                    let mut glyphs = Vec::with_capacity(text_chars.len());
                    let mut x_offset: f32 = 0.0;

                    for (i, (&glyph_id, &c)) in
                        glyph_indices.iter().zip(text_chars.iter()).enumerate()
                    {
                        let advance = if i < glyph_metrics.len() {
                            glyph_metrics[i].advance_width as f32 * scale
                        } else {
                            size * 0.5
                        };

                        glyphs.push(PositionedGlyph {
                            glyph_id,
                            x: x_offset,
                            y: 0.0,
                            advance,
                            character: c,
                            cluster: i as u32,
                        });

                        x_offset += advance;
                    }

                    let ascent = design_metrics.ascent as f32 * scale;
                    let descent = design_metrics.descent as f32 * scale;
                    let leading = design_metrics.line_gap as f32 * scale;

                    let metrics = TextMetrics {
                        width: x_offset,
                        height: ascent + descent + leading,
                        ascent,
                        descent,
                        leading,
                        underline_offset: design_metrics.underline_position as f32 * scale,
                        underline_thickness: design_metrics.underline_thickness as f32 * scale,
                        strikethrough_offset: design_metrics.strikethrough_position as f32 * scale,
                        strikethrough_thickness: design_metrics.strikethrough_thickness as f32
                            * scale,
                        overline_offset: -ascent,
                    };

                    return Ok(ShapedRun {
                        text: text.to_string(),
                        glyphs,
                        font_family: font_family_name,
                        font_weight: weight,
                        font_style: style,
                        font_stretch: stretch,
                        font_size: size,
                        metrics,
                        direction: TextDirection::Ltr,
                    });
                }
            }
        }

        // Fallback to simple shaping
        self.shape_simple(text, font_chain, weight, style, stretch, size)
    }

    /// Simple shaping fallback when DirectWrite is unavailable.
    #[cfg(windows)]
    fn shape_simple(
        &self,
        text: &str,
        font_chain: &FontFamilyChain,
        weight: FontWeight,
        style: FontStyle,
        stretch: FontStretch,
        size: f32,
    ) -> Result<ShapedRun, TextError> {
        let avg_char_width = size * 0.5;
        let mut glyphs = Vec::with_capacity(text.len());
        let mut x_offset: f32 = 0.0;

        for (i, c) in text.chars().enumerate() {
            let advance = if c.is_ascii() {
                avg_char_width
            } else {
                size // CJK and other wide characters
            };

            glyphs.push(PositionedGlyph {
                glyph_id: c as u16,
                x: x_offset,
                y: 0.0,
                advance,
                character: c,
                cluster: i as u32,
            });

            x_offset += advance;
        }

        let metrics = TextMetrics {
            width: x_offset,
            ..TextMetrics::with_font_size(size)
        };

        Ok(ShapedRun {
            text: text.to_string(),
            glyphs,
            font_family: font_chain.primary.clone(),
            font_weight: weight,
            font_style: style,
            font_stretch: stretch,
            font_size: size,
            metrics,
            direction: TextDirection::Ltr,
        })
    }

    /// Shape text using Core Text on macOS.
    #[cfg(target_os = "macos")]
    pub fn shape(
        &self,
        text: &str,
        font_chain: &FontFamilyChain,
        weight: FontWeight,
        style: FontStyle,
        stretch: FontStretch,
        size: f32,
    ) -> Result<ShapedRun, TextError> {
        if text.is_empty() {
            return Ok(ShapedRun {
                text: String::new(),
                glyphs: Vec::new(),
                font_family: font_chain.primary.clone(),
                font_weight: weight,
                font_style: style,
                font_stretch: stretch,
                font_size: size,
                metrics: TextMetrics::with_font_size(size),
                direction: TextDirection::Ltr,
            });
        }

        // Try to find a font from the chain
        let mut ct_font_opt: Option<core_text::font::CTFont> = None;
        let mut used_family = font_chain.primary.clone();
        
        for family in font_chain.all_families() {
            // Try to create font with traits
            if let Ok(font) = Self::create_ct_font_with_traits(family, size, weight.0, style == FontStyle::Italic) {
                ct_font_opt = Some(font);
                used_family = family.to_string();
                break;
            }
        }
        
        // Fallback to system font if nothing found
        let ct_font = ct_font_opt.unwrap_or_else(|| {
            ct_font::new_from_name("Helvetica", size as f64)
                .unwrap_or_else(|_| ct_font::new_from_name(".AppleSystemUIFont", size as f64).unwrap())
        });
        
        // Convert text to UTF-16 for Core Text
        let utf16_chars: Vec<u16> = text.encode_utf16().collect();
        let char_count = utf16_chars.len();
        
        // Get glyph IDs
        let mut glyph_ids: Vec<u16> = vec![0; char_count];
        
        unsafe {
            extern "C" {
                fn CTFontGetGlyphsForCharacters(
                    font: core_text::font::CTFontRef,
                    characters: *const u16,
                    glyphs: *mut u16,
                    count: isize,
                ) -> bool;
                
                fn CTFontGetAdvancesForGlyphs(
                    font: core_text::font::CTFontRef,
                    orientation: u32,
                    glyphs: *const u16,
                    advances: *mut CGSize,
                    count: isize,
                ) -> f64;
            }
            
            let _success = CTFontGetGlyphsForCharacters(
                ct_font.as_concrete_TypeRef(),
                utf16_chars.as_ptr(),
                glyph_ids.as_mut_ptr(),
                char_count as isize,
            );
            
            // Get advances for each glyph
            let mut glyph_advances: Vec<CGSize> = vec![CGSize::new(0.0, 0.0); char_count];
            let _total_advance = CTFontGetAdvancesForGlyphs(
                ct_font.as_concrete_TypeRef(),
                0, // kCTFontOrientationHorizontal
                glyph_ids.as_ptr(),
                glyph_advances.as_mut_ptr(),
                char_count as isize,
            );
            
            // Build positioned glyphs
            let text_chars: Vec<char> = text.chars().collect();
            let mut glyphs = Vec::with_capacity(text_chars.len());
            let mut x_offset: f32 = 0.0;
            
            // Handle surrogate pairs - UTF-16 index to char index mapping
            let mut char_idx = 0;
            let mut utf16_idx = 0;
            
            while utf16_idx < char_count && char_idx < text_chars.len() {
                let c = text_chars[char_idx];
                let advance = glyph_advances[utf16_idx].width as f32;
                
                // Handle missing glyphs (glyph ID 0)
                let final_advance = if glyph_ids[utf16_idx] == 0 && advance == 0.0 {
                    size * 0.5 // Fallback advance
                } else {
                    advance
                };
                
                glyphs.push(PositionedGlyph {
                    glyph_id: glyph_ids[utf16_idx],
                    x: x_offset,
                    y: 0.0,
                    advance: final_advance,
                    character: c,
                    cluster: char_idx as u32,
                });
                
                x_offset += final_advance;
                
                // Advance UTF-16 index (handle surrogate pairs)
                utf16_idx += c.len_utf16();
                char_idx += 1;
            }
            
            // Get font metrics from Core Text
            let ascent = ct_font.ascent() as f32;
            let descent = ct_font.descent() as f32;
            let leading = ct_font.leading() as f32;
            let underline_position = ct_font.underline_position() as f32;
            let underline_thickness = ct_font.underline_thickness() as f32;
            
            // Calculate strikethrough position (approximately middle of x-height)
            let x_height = ct_font.x_height() as f32;
            let strikethrough_offset = x_height * 0.5;
            
            let metrics = TextMetrics {
                width: x_offset,
                height: ascent + descent + leading,
                ascent,
                descent,
                leading,
                underline_offset: underline_position,
                underline_thickness,
                strikethrough_offset,
                strikethrough_thickness: underline_thickness,
                overline_offset: -ascent,
            };
            
            Ok(ShapedRun {
                text: text.to_string(),
                glyphs,
                font_family: used_family,
                font_weight: weight,
                font_style: style,
                font_stretch: stretch,
                font_size: size,
                metrics,
                direction: TextDirection::Ltr,
            })
        }
    }

    /// Create a Core Text font with specific traits.
    #[cfg(target_os = "macos")]
    fn create_ct_font_with_traits(
        family: &str,
        size: f32,
        weight: u16,
        italic: bool,
    ) -> Result<core_text::font::CTFont, TextError> {
        // Try to find a font variant with the specified traits
        // First try appending -Bold, -Italic, etc. to the family name
        let mut variants_to_try = vec![family.to_string()];
        
        if weight >= 700 {
            variants_to_try.push(format!("{}-Bold", family));
            variants_to_try.push(format!("{}Bold", family));
            if italic {
                variants_to_try.push(format!("{}-BoldItalic", family));
                variants_to_try.push(format!("{}-BoldOblique", family));
            }
        }
        
        if italic {
            variants_to_try.push(format!("{}-Italic", family));
            variants_to_try.push(format!("{}-Oblique", family));
            variants_to_try.push(format!("{}Italic", family));
        }
        
        for variant in &variants_to_try {
            if let Ok(font) = ct_font::new_from_name(variant, size as f64) {
                return Ok(font);
            }
        }
        
        Err(TextError::FontNotFound(family.to_string()))
    }
    
    /// Simplified shaping fallback for non-Windows, non-macOS platforms.
    #[cfg(all(not(windows), not(target_os = "macos")))]
    pub fn shape(
        &self,
        text: &str,
        font_chain: &FontFamilyChain,
        weight: FontWeight,
        style: FontStyle,
        stretch: FontStretch,
        size: f32,
    ) -> Result<ShapedRun, TextError> {
        // Simplified shaping for other platforms
        let avg_char_width = size * 0.5;
        let mut glyphs = Vec::with_capacity(text.len());
        let mut x_offset: f32 = 0.0;

        for (i, c) in text.chars().enumerate() {
            let advance = if c.is_ascii() {
                avg_char_width
            } else {
                size // CJK characters are typically wider
            };

            glyphs.push(PositionedGlyph {
                glyph_id: c as u16,
                x: x_offset,
                y: 0.0,
                advance,
                character: c,
                cluster: i as u32,
            });

            x_offset += advance;
        }

        let metrics = TextMetrics {
            width: x_offset,
            ..TextMetrics::with_font_size(size)
        };

        Ok(ShapedRun {
            text: text.to_string(),
            glyphs,
            font_family: font_chain.primary.clone(),
            font_weight: weight,
            font_style: style,
            font_stretch: stretch,
            font_size: size,
            metrics,
            direction: TextDirection::Ltr,
        })
    }

    /// Measure text without full shaping (faster for layout).
    pub fn measure(
        &self,
        text: &str,
        font_family: &str,
        weight: FontWeight,
        style: FontStyle,
        stretch: FontStretch,
        size: f32,
    ) -> Result<TextMetrics, TextError> {
        let chain = FontFamilyChain::from_css_value(font_family);
        let run = self.shape(text, &chain, weight, style, stretch, size)?;
        Ok(run.metrics)
    }

    /// Shape text with bidirectional text support.
    ///
    /// This function analyzes the text for bidirectional content (mixed LTR/RTL)
    /// using the Unicode Bidirectional Algorithm (UAX #9) and produces separate
    /// shaped runs for each directional segment in visual order.
    ///
    /// # Arguments
    /// * `text` - The text to shape
    /// * `font_chain` - Font family chain with fallbacks
    /// * `weight` - Font weight
    /// * `style` - Font style (normal, italic, oblique)
    /// * `stretch` - Font stretch
    /// * `size` - Font size in pixels
    /// * `base_direction` - Base paragraph direction (from CSS `direction` property)
    ///
    /// # Returns
    /// A vector of `ShapedRun`s in visual (display) order, each with its own direction.
    /// For pure LTR or RTL text, this returns a single run.
    pub fn shape_with_bidi(
        &self,
        text: &str,
        font_chain: &FontFamilyChain,
        weight: FontWeight,
        style: FontStyle,
        stretch: FontStretch,
        size: f32,
        base_direction: Option<TextDirection>,
    ) -> Result<Vec<ShapedRun>, TextError> {
        if text.is_empty() {
            return Ok(vec![]);
        }

        // Convert to bidi direction for analysis
        let bidi_base = base_direction.map(|d| d.to_bidi());

        // Analyze bidirectional text
        let bidi_info = BidiInfo::with_base_direction(text, bidi_base);

        // Fast path: pure LTR or RTL text with single run
        let visual_runs = bidi_info.visual_runs();
        if visual_runs.len() == 1 && bidi_info.is_pure_ltr() {
            // Simple case: just shape the whole text as LTR
            let mut run = self.shape(text, font_chain, weight, style, stretch, size)?;
            run.direction = TextDirection::Ltr;
            return Ok(vec![run]);
        }

        // Handle mixed-direction text
        let mut shaped_runs = Vec::with_capacity(visual_runs.len());

        for bidi_run in visual_runs {
            let run_text = bidi_run.text(text);
            if run_text.is_empty() {
                continue;
            }

            // Shape this run
            let mut shaped = self.shape(run_text, font_chain, weight, style, stretch, size)?;
            shaped.direction = TextDirection::from_bidi(bidi_run.direction);

            // For RTL runs, we may need to reverse the glyph order
            // (depending on whether the underlying shaper already did this)
            // Note: Core Text and DirectWrite handle RTL internally,
            // so we typically don't need to reverse here.

            shaped_runs.push(shaped);
        }

        Ok(shaped_runs)
    }

    /// Shape text with bidirectional support using CSS direction property.
    ///
    /// Convenience wrapper around `shape_with_bidi` that takes a CSS direction value.
    pub fn shape_with_css_direction(
        &self,
        text: &str,
        font_chain: &FontFamilyChain,
        weight: FontWeight,
        style: FontStyle,
        stretch: FontStretch,
        size: f32,
        css_direction: CssDirection,
    ) -> Result<Vec<ShapedRun>, TextError> {
        self.shape_with_bidi(
            text,
            font_chain,
            weight,
            style,
            stretch,
            size,
            Some(TextDirection::from_css(css_direction)),
        )
    }

    /// Wrap text into lines that fit within the specified width.
    ///
    /// This function shapes text and breaks it into multiple lines based on:
    /// - Available width
    /// - CSS word-break property
    /// - UAX #14 line breaking rules
    ///
    /// # Arguments
    /// * `text` - The text to wrap
    /// * `font_chain` - Font family chain with fallbacks
    /// * `weight` - Font weight
    /// * `style` - Font style
    /// * `stretch` - Font stretch
    /// * `size` - Font size in pixels
    /// * `max_width` - Maximum line width in pixels
    /// * `word_break` - CSS word-break property value
    ///
    /// # Returns
    /// A vector of `WrappedLine` structs, each containing shaped runs for one line.
    pub fn wrap_text(
        &self,
        text: &str,
        font_chain: &FontFamilyChain,
        weight: FontWeight,
        style: FontStyle,
        stretch: FontStretch,
        size: f32,
        max_width: f32,
        word_break: CssWordBreak,
    ) -> Result<Vec<WrappedLine>, TextError> {
        if text.is_empty() {
            return Ok(vec![]);
        }

        // Convert CSS word-break to our line breaking enum
        let lb_word_break = match word_break {
            CssWordBreak::Normal => LineBreakWordBreak::Normal,
            CssWordBreak::BreakAll => LineBreakWordBreak::BreakAll,
            CssWordBreak::KeepAll => LineBreakWordBreak::KeepAll,
            CssWordBreak::BreakWord => LineBreakWordBreak::BreakWord,
        };

        let breaker = LineBreaker::new(lb_word_break, OverflowWrap::Normal);
        let mut lines = Vec::new();

        // First, handle mandatory line breaks
        for segment in rustkit_text::line_break::break_into_lines(text) {
            let segment_text = segment.text_without_break();
            if segment_text.is_empty() {
                // Empty line (just a line break)
                lines.push(WrappedLine {
                    runs: vec![],
                    width: 0.0,
                    start_offset: segment.start,
                    end_offset: segment.end,
                });
                continue;
            }

            // Now wrap this segment within max_width
            let segment_lines = self.wrap_segment(
                segment_text,
                font_chain,
                weight,
                style,
                stretch,
                size,
                max_width,
                &breaker,
                segment.start,
            )?;

            lines.extend(segment_lines);
        }

        // Handle case where text has no mandatory breaks
        if lines.is_empty() && !text.is_empty() {
            lines = self.wrap_segment(
                text,
                font_chain,
                weight,
                style,
                stretch,
                size,
                max_width,
                &breaker,
                0,
            )?;
        }

        Ok(lines)
    }

    /// Internal helper to wrap a single segment (no mandatory breaks).
    fn wrap_segment(
        &self,
        text: &str,
        font_chain: &FontFamilyChain,
        weight: FontWeight,
        style: FontStyle,
        stretch: FontStretch,
        size: f32,
        max_width: f32,
        breaker: &LineBreaker,
        base_offset: usize,
    ) -> Result<Vec<WrappedLine>, TextError> {
        if text.is_empty() {
            return Ok(vec![]);
        }

        let mut lines = Vec::new();
        let mut line_start = 0;

        while line_start < text.len() {
            // Shape the remaining text to find where we need to break
            let remaining = &text[line_start..];
            let shaped = self.shape(remaining, font_chain, weight, style, stretch, size)?;

            if shaped.metrics.width <= max_width {
                // Entire remaining text fits on one line
                let width = shaped.metrics.width;
                lines.push(WrappedLine {
                    runs: vec![shaped],
                    width,
                    start_offset: base_offset + line_start,
                    end_offset: base_offset + text.len(),
                });
                break;
            }

            // Need to find a break point
            // Binary search for the right break point
            let break_offset = self.find_line_break(
                remaining,
                font_chain,
                weight,
                style,
                stretch,
                size,
                max_width,
                breaker,
            )?;

            if break_offset == 0 {
                // Can't fit even one character - force break at first grapheme
                let first_grapheme_end = rustkit_text::segmentation::grapheme_boundaries(remaining)
                    .get(1)
                    .copied()
                    .unwrap_or(remaining.len());

                let line_text = &remaining[..first_grapheme_end];
                let shaped_line = self.shape(line_text, font_chain, weight, style, stretch, size)?;

                lines.push(WrappedLine {
                    runs: vec![shaped_line.clone()],
                    width: shaped_line.metrics.width,
                    start_offset: base_offset + line_start,
                    end_offset: base_offset + line_start + first_grapheme_end,
                });

                line_start += first_grapheme_end;
            } else {
                let line_text = &remaining[..break_offset];
                let shaped_line = self.shape(line_text, font_chain, weight, style, stretch, size)?;

                lines.push(WrappedLine {
                    runs: vec![shaped_line.clone()],
                    width: shaped_line.metrics.width,
                    start_offset: base_offset + line_start,
                    end_offset: base_offset + line_start + break_offset,
                });

                // Skip whitespace at the break point
                line_start += break_offset;
                while line_start < text.len() && text[line_start..].starts_with(char::is_whitespace) {
                    line_start += text[line_start..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
                }
            }
        }

        Ok(lines)
    }

    /// Find the best line break point within max_width.
    fn find_line_break(
        &self,
        text: &str,
        font_chain: &FontFamilyChain,
        weight: FontWeight,
        style: FontStyle,
        stretch: FontStretch,
        size: f32,
        max_width: f32,
        breaker: &LineBreaker,
    ) -> Result<usize, TextError> {
        // Get all break opportunities
        let break_offsets = breaker.break_offsets(text);

        // Find the last break that fits
        let mut best_break = 0;

        for &offset in &break_offsets {
            if offset == 0 {
                continue;
            }

            let prefix = &text[..offset];
            let shaped = self.shape(prefix, font_chain, weight, style, stretch, size)?;

            if shaped.metrics.width <= max_width {
                best_break = offset;
            } else {
                break;
            }
        }

        Ok(best_break)
    }
}

/// A wrapped line of text.
#[derive(Debug, Clone)]
pub struct WrappedLine {
    /// Shaped runs for this line.
    pub runs: Vec<ShapedRun>,
    /// Total width of this line.
    pub width: f32,
    /// Start byte offset in the original text.
    pub start_offset: usize,
    /// End byte offset in the original text.
    pub end_offset: usize,
}

impl WrappedLine {
    /// Get the height of this line (max height of all runs).
    pub fn height(&self) -> f32 {
        self.runs
            .iter()
            .map(|r| r.metrics.height)
            .fold(0.0f32, f32::max)
    }

    /// Get the ascent of this line (max ascent of all runs).
    pub fn ascent(&self) -> f32 {
        self.runs
            .iter()
            .map(|r| r.metrics.ascent)
            .fold(0.0f32, f32::max)
    }

    /// Get the descent of this line (max descent of all runs).
    pub fn descent(&self) -> f32 {
        self.runs
            .iter()
            .map(|r| r.metrics.descent)
            .fold(0.0f32, f32::max)
    }

    /// Check if this line is empty.
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty() || self.runs.iter().all(|r| r.glyphs.is_empty())
    }

    /// Get the text content of this line.
    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
}

impl Default for TextShaper {
    fn default() -> Self {
        Self::new()
    }
}

/// @font-face rule representation.
#[derive(Debug, Clone)]
pub struct FontFaceRule {
    /// Font family name to register.
    pub family: String,
    /// Font source URL.
    pub src: String,
    /// Font weight (defaults to normal).
    pub weight: FontWeight,
    /// Font style (defaults to normal).
    pub style: FontStyle,
    /// Font stretch (defaults to normal).
    pub stretch: FontStretch,
    /// Unicode range to support.
    pub unicode_range: Option<String>,
    /// Font display strategy.
    pub display: FontDisplay,
}

/// Font display strategies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontDisplay {
    /// Block period: 3s, swap period: infinite.
    #[default]
    Auto,
    /// Block period: short, swap period: infinite.
    Block,
    /// Block period: none, swap period: infinite.
    Swap,
    /// Block period: very short, swap period: short.
    Fallback,
    /// Block period: very short, swap period: none.
    Optional,
}

/// Font loader for @font-face rules.
pub struct FontLoader {
    /// Loaded font faces.
    #[allow(dead_code)]
    loaded: RwLock<HashMap<String, LoadedFont>>,
    /// Pending font loads.
    #[allow(dead_code)]
    pending: RwLock<Vec<FontFaceRule>>,
}

#[allow(dead_code)]
struct LoadedFont {
    family: String,
    data: Vec<u8>,
}

impl FontLoader {
    /// Create a new font loader.
    pub fn new() -> Self {
        Self {
            loaded: RwLock::new(HashMap::new()),
            pending: RwLock::new(Vec::new()),
        }
    }

    /// Queue a @font-face rule for loading.
    pub fn queue_font_face(&self, rule: FontFaceRule) {
        let mut pending = self.pending.write().unwrap();
        pending.push(rule);
    }

    /// Load all pending fonts (call from network thread).
    #[allow(unused)]
    pub async fn load_pending(&self) -> Vec<Result<String, TextError>> {
        let rules = {
            let mut pending = self.pending.write().unwrap();
            std::mem::take(&mut *pending)
        };

        let mut results = Vec::with_capacity(rules.len());
        for rule in rules {
            results.push(self.load_font(rule).await);
        }
        results
    }

    /// Load a single font.
    async fn load_font(&self, rule: FontFaceRule) -> Result<String, TextError> {
        // In a full implementation, this would:
        // 1. Fetch the font file from rule.src
        // 2. Parse the font data
        // 3. Register with DirectWrite
        // For now, we just track the rule

        let family = rule.family.clone();
        let mut loaded = self.loaded.write().unwrap();
        loaded.insert(
            family.clone(),
            LoadedFont {
                family: rule.family,
                data: Vec::new(),
            },
        );

        Ok(family)
    }

    /// Check if a font family is loaded (or loading).
    pub fn is_loaded(&self, family: &str) -> bool {
        let loaded = self.loaded.read().unwrap();
        loaded.contains_key(family)
    }
}

impl Default for FontLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_font_family_chain() {
        let chain = FontFamilyChain::new("Arial")
            .with_fallback("Helvetica")
            .with_fallback("sans-serif");

        let families: Vec<_> = chain.all_families().collect();
        assert_eq!(families, vec!["Arial", "Helvetica", "sans-serif"]);
    }

    #[test]
    fn test_font_family_chain_from_css() {
        let chain = FontFamilyChain::from_css_value("\"Roboto\", Arial, sans-serif");
        assert_eq!(chain.primary, "Roboto");
        assert!(chain.fallbacks.contains(&"Arial".to_string()));
    }

    #[test]
    fn test_generic_font_families() {
        let sans = FontFamilyChain::from_css_value("sans-serif");
        #[cfg(target_os = "macos")]
        assert_eq!(sans.primary, "SF Pro");
        #[cfg(not(target_os = "macos"))]
        assert_eq!(sans.primary, "Segoe UI");

        let mono = FontFamilyChain::from_css_value("monospace");
        #[cfg(target_os = "macos")]
        assert_eq!(mono.primary, "SF Mono");
        #[cfg(not(target_os = "macos"))]
        assert_eq!(mono.primary, "Cascadia Code");

        // Test system-ui and vendor-prefixed variants
        let system = FontFamilyChain::from_css_value("system-ui");
        #[cfg(target_os = "macos")]
        assert_eq!(system.primary, ".AppleSystemUIFont");
        #[cfg(not(target_os = "macos"))]
        assert_eq!(system.primary, "Segoe UI");

        let apple = FontFamilyChain::from_css_value("-apple-system");
        #[cfg(target_os = "macos")]
        assert_eq!(apple.primary, ".AppleSystemUIFont");
        #[cfg(not(target_os = "macos"))]
        assert_eq!(apple.primary, "Segoe UI");
    }

    #[test]
    fn test_text_transform() {
        assert_eq!(
            apply_text_transform("hello world", TextTransform::Uppercase),
            "HELLO WORLD"
        );
        assert_eq!(
            apply_text_transform("HELLO WORLD", TextTransform::Lowercase),
            "hello world"
        );
        assert_eq!(
            apply_text_transform("hello world", TextTransform::Capitalize),
            "Hello World"
        );
        assert_eq!(
            apply_text_transform("hello world", TextTransform::None),
            "hello world"
        );
    }

    #[test]
    fn test_collapse_whitespace() {
        assert_eq!(
            collapse_whitespace("hello   world", WhiteSpace::Normal),
            "hello world"
        );
        assert_eq!(
            collapse_whitespace("hello   world", WhiteSpace::Pre),
            "hello   world"
        );
        assert_eq!(
            collapse_whitespace("hello\n\nworld", WhiteSpace::PreLine),
            "hello\n\nworld"
        );
    }

    #[test]
    fn test_line_height() {
        let metrics = TextMetrics::with_font_size(16.0);

        let normal = LineHeight::Normal;
        assert_eq!(normal.compute(16.0, &metrics), metrics.height);

        let number = LineHeight::Number(1.5);
        assert_eq!(number.compute(16.0, &metrics), 24.0);

        let length = LineHeight::Length(20.0);
        assert_eq!(length.compute(16.0, &metrics), 20.0);
    }

    #[test]
    fn test_text_metrics() {
        let metrics = TextMetrics::with_font_size(16.0);
        assert!(metrics.ascent > 0.0);
        assert!(metrics.descent > 0.0);
        assert!(metrics.height > 0.0);
        assert!(metrics.underline_thickness > 0.0);
    }

    #[test]
    fn test_text_decoration() {
        let decoration = TextDecoration::from_style(
            TextDecorationLine::UNDERLINE,
            Some(Color::from_rgb(255, 0, 0)),
            TextDecorationStyle::Solid,
            Length::Auto,
            16.0,
        );

        assert!(decoration.has_decorations());
        assert!(decoration.lines.underline);
        assert!(!decoration.lines.line_through);
    }

    #[test]
    fn test_text_shaper_creation() {
        let shaper = TextShaper::new();
        let chain = FontFamilyChain::sans_serif();
        let result = shaper.shape(
            "Hello",
            &chain,
            FontWeight::NORMAL,
            FontStyle::Normal,
            FontStretch::Normal,
            16.0,
        );
        assert!(result.is_ok());
        let run = result.unwrap();
        assert_eq!(run.text, "Hello");
        assert!(!run.glyphs.is_empty());
    }

    #[test]
    fn test_font_loader() {
        let loader = FontLoader::new();
        assert!(!loader.is_loaded("TestFont"));

        loader.queue_font_face(FontFaceRule {
            family: "TestFont".to_string(),
            src: "url(test.woff2)".to_string(),
            weight: FontWeight::NORMAL,
            style: FontStyle::Normal,
            stretch: FontStretch::Normal,
            unicode_range: None,
            display: FontDisplay::Swap,
        });
    }

    #[test]
    fn test_empty_text_shaping() {
        let shaper = TextShaper::new();
        let chain = FontFamilyChain::sans_serif();
        let result = shaper.shape(
            "",
            &chain,
            FontWeight::NORMAL,
            FontStyle::Normal,
            FontStretch::Normal,
            16.0,
        );
        assert!(result.is_ok());
        let run = result.unwrap();
        assert!(run.glyphs.is_empty());
    }

    #[test]
    fn test_text_direction_conversions() {
        use rustkit_css::Direction as CssDirection;
        use rustkit_text::bidi::Direction as BidiDirection;

        // From CSS
        assert_eq!(TextDirection::from_css(CssDirection::Ltr), TextDirection::Ltr);
        assert_eq!(TextDirection::from_css(CssDirection::Rtl), TextDirection::Rtl);

        // From bidi
        assert_eq!(TextDirection::from_bidi(BidiDirection::Ltr), TextDirection::Ltr);
        assert_eq!(TextDirection::from_bidi(BidiDirection::Rtl), TextDirection::Rtl);

        // To bidi
        assert_eq!(TextDirection::Ltr.to_bidi(), BidiDirection::Ltr);
        assert_eq!(TextDirection::Rtl.to_bidi(), BidiDirection::Rtl);

        // Helper methods
        assert!(TextDirection::Ltr.is_ltr());
        assert!(!TextDirection::Ltr.is_rtl());
        assert!(TextDirection::Rtl.is_rtl());
        assert!(!TextDirection::Rtl.is_ltr());

        // Default
        assert_eq!(TextDirection::default(), TextDirection::Ltr);
    }

    #[test]
    fn test_shape_with_bidi_empty() {
        let shaper = TextShaper::new();
        let chain = FontFamilyChain::sans_serif();
        let result = shaper.shape_with_bidi(
            "",
            &chain,
            FontWeight::NORMAL,
            FontStyle::Normal,
            FontStretch::Normal,
            16.0,
            None,
        );
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_shape_with_bidi_ltr() {
        let shaper = TextShaper::new();
        let chain = FontFamilyChain::sans_serif();
        let result = shaper.shape_with_bidi(
            "Hello, world!",
            &chain,
            FontWeight::NORMAL,
            FontStyle::Normal,
            FontStretch::Normal,
            16.0,
            None,
        );
        assert!(result.is_ok());
        let runs = result.unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].direction, TextDirection::Ltr);
        assert_eq!(runs[0].text, "Hello, world!");
    }

    #[test]
    fn test_shape_with_bidi_rtl() {
        let shaper = TextShaper::new();
        let chain = FontFamilyChain::sans_serif();
        // Hebrew: "shalom" (שלום)
        let result = shaper.shape_with_bidi(
            "\u{05E9}\u{05DC}\u{05D5}\u{05DD}",
            &chain,
            FontWeight::NORMAL,
            FontStyle::Normal,
            FontStretch::Normal,
            16.0,
            None,
        );
        assert!(result.is_ok());
        let runs = result.unwrap();
        // Pure RTL text should produce a single RTL run
        assert!(!runs.is_empty());
    }

    #[test]
    fn test_shape_with_bidi_mixed() {
        let shaper = TextShaper::new();
        let chain = FontFamilyChain::sans_serif();
        // Mixed: "Hello שלום world"
        let result = shaper.shape_with_bidi(
            "Hello \u{05E9}\u{05DC}\u{05D5}\u{05DD} world",
            &chain,
            FontWeight::NORMAL,
            FontStyle::Normal,
            FontStretch::Normal,
            16.0,
            None,
        );
        assert!(result.is_ok());
        let runs = result.unwrap();
        // Mixed text should produce multiple runs
        assert!(runs.len() >= 2, "Expected multiple runs for mixed text, got {}", runs.len());
    }

    #[test]
    fn test_shape_with_css_direction() {
        use rustkit_css::Direction as CssDirection;

        let shaper = TextShaper::new();
        let chain = FontFamilyChain::sans_serif();
        let result = shaper.shape_with_css_direction(
            "Hello",
            &chain,
            FontWeight::NORMAL,
            FontStyle::Normal,
            FontStretch::Normal,
            16.0,
            CssDirection::Ltr,
        );
        assert!(result.is_ok());
        let runs = result.unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].direction, TextDirection::Ltr);
    }

    #[test]
    fn test_shaped_run_direction_field() {
        let shaper = TextShaper::new();
        let chain = FontFamilyChain::sans_serif();
        let result = shaper.shape(
            "Test",
            &chain,
            FontWeight::NORMAL,
            FontStyle::Normal,
            FontStretch::Normal,
            16.0,
        );
        assert!(result.is_ok());
        let run = result.unwrap();
        // Default shape() should produce LTR direction
        assert_eq!(run.direction, TextDirection::Ltr);
    }

    #[test]
    fn test_wrap_text_empty() {
        let shaper = TextShaper::new();
        let chain = FontFamilyChain::sans_serif();
        let result = shaper.wrap_text(
            "",
            &chain,
            FontWeight::NORMAL,
            FontStyle::Normal,
            FontStretch::Normal,
            16.0,
            200.0,
            CssWordBreak::Normal,
        );
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_wrap_text_single_line() {
        let shaper = TextShaper::new();
        let chain = FontFamilyChain::sans_serif();
        let result = shaper.wrap_text(
            "Hello",
            &chain,
            FontWeight::NORMAL,
            FontStyle::Normal,
            FontStretch::Normal,
            16.0,
            1000.0, // Very wide, should fit on one line
            CssWordBreak::Normal,
        );
        assert!(result.is_ok());
        let lines = result.unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text(), "Hello");
    }

    #[test]
    fn test_wrap_text_multiple_lines() {
        let shaper = TextShaper::new();
        let chain = FontFamilyChain::sans_serif();
        let result = shaper.wrap_text(
            "Hello world this is a test",
            &chain,
            FontWeight::NORMAL,
            FontStyle::Normal,
            FontStretch::Normal,
            16.0,
            80.0, // Narrow width to force wrapping
            CssWordBreak::Normal,
        );
        assert!(result.is_ok());
        let lines = result.unwrap();
        // Should have multiple lines due to narrow width
        assert!(lines.len() > 1, "Expected multiple lines, got {}", lines.len());
    }

    #[test]
    fn test_wrap_text_with_newlines() {
        let shaper = TextShaper::new();
        let chain = FontFamilyChain::sans_serif();
        let result = shaper.wrap_text(
            "Line1\nLine2",
            &chain,
            FontWeight::NORMAL,
            FontStyle::Normal,
            FontStretch::Normal,
            16.0,
            1000.0,
            CssWordBreak::Normal,
        );
        assert!(result.is_ok());
        let lines = result.unwrap();
        // Should have at least 2 lines due to newline
        assert!(lines.len() >= 2, "Expected at least 2 lines for text with newline");
    }

    #[test]
    fn test_wrap_text_break_all() {
        let shaper = TextShaper::new();
        let chain = FontFamilyChain::sans_serif();
        // With break-all, should be able to break mid-word
        let result = shaper.wrap_text(
            "Supercalifragilisticexpialidocious",
            &chain,
            FontWeight::NORMAL,
            FontStyle::Normal,
            FontStretch::Normal,
            16.0,
            50.0, // Very narrow
            CssWordBreak::BreakAll,
        );
        assert!(result.is_ok());
        let lines = result.unwrap();
        // Should break the long word
        assert!(lines.len() > 1, "Expected word to be broken with break-all");
    }

    #[test]
    fn test_wrapped_line_properties() {
        let shaper = TextShaper::new();
        let chain = FontFamilyChain::sans_serif();
        let result = shaper.wrap_text(
            "Test",
            &chain,
            FontWeight::NORMAL,
            FontStyle::Normal,
            FontStretch::Normal,
            16.0,
            1000.0,
            CssWordBreak::Normal,
        );
        assert!(result.is_ok());
        let lines = result.unwrap();
        assert_eq!(lines.len(), 1);

        let line = &lines[0];
        assert!(line.width > 0.0);
        assert!(line.height() > 0.0);
        assert!(line.ascent() > 0.0);
        assert!(!line.is_empty());
        assert_eq!(line.start_offset, 0);
        assert_eq!(line.end_offset, 4);
    }
}
