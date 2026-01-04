//! # Text Rendering Module
//!
//! Comprehensive text rendering support using DirectWrite on Windows.
//! Provides font fallback, text shaping, text decoration, and line height calculation.
//!
//! ## Features
//!
//! - **Font Fallback Chain**: Automatic fallback for missing glyphs
//! - **Complex Script Support**: Full Unicode shaping via DirectWrite
//! - **Text Decoration**: Underline, strikethrough, overline
//! - **Line Height**: Proper line-height calculation with various units
//! - **Font Variants**: Bold, italic, weights, stretches
//! - **Metrics**: Accurate glyph and line metrics

use rustkit_css::{
    Color, FontStretch, FontStyle, FontWeight, Length, TextDecorationLine, TextDecorationStyle,
    TextTransform, WhiteSpace,
};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;

#[cfg(windows)]
use rustkit_text::{FontCollection as RkFontCollection, FontStretch as RkFontStretch, FontStyle as RkFontStyle, FontWeight as RkFontWeight};

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
    pub fn serif() -> Self {
        Self::new("Times New Roman")
            .with_fallback("Georgia")
            .with_fallback("Noto Serif")
            .with_fallback("Noto Serif CJK SC")
            .with_fallback("SimSun")
            .with_fallback("serif")
    }

    /// Create default font chain for monospace.
    pub fn monospace() -> Self {
        Self::new("Cascadia Code")
            .with_fallback("Consolas")
            .with_fallback("Courier New")
            .with_fallback("Noto Sans Mono")
            .with_fallback("monospace")
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
            "cursive" => Self::new("Comic Sans MS")
                .with_fallback("Brush Script MT")
                .with_fallback("cursive"),
            "fantasy" => Self::new("Impact")
                .with_fallback("Papyrus")
                .with_fallback("fantasy"),
            "system-ui" => Self::new("Segoe UI").with_fallback("system-ui"),
            _ => {
                let mut chain = Self::new(primary);
                for fallback in families.iter().skip(1) {
                    chain.fallbacks.push(fallback.to_string());
                }
                // Always add system fallbacks
                chain.fallbacks.push("Segoe UI".to_string());
                chain.fallbacks.push("Arial".to_string());
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
    pub fn with_font_size(font_size: f32) -> Self {
        let ascent = font_size * 0.8;
        let descent = font_size * 0.2;
        let leading = font_size * 0.15;

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
    fonts: RwLock<HashMap<FontKey, ()>>,
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

    #[cfg(not(windows))]
    pub fn get_metrics(
        &self,
        _family: &str,
        _weight: FontWeight,
        _style: FontStyle,
        _stretch: FontStretch,
        size: f32,
    ) -> Result<TextMetrics, TextError> {
        // Fallback metrics for non-Windows platforms
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
        })
    }

    #[cfg(not(windows))]
    pub fn shape(
        &self,
        text: &str,
        font_chain: &FontFamilyChain,
        weight: FontWeight,
        style: FontStyle,
        stretch: FontStretch,
        size: f32,
    ) -> Result<ShapedRun, TextError> {
        // Simplified shaping for non-Windows platforms
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
        assert_eq!(sans.primary, "Segoe UI");

        let mono = FontFamilyChain::from_css_value("monospace");
        assert_eq!(mono.primary, "Cascadia Code");
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
}
