//! macOS text shaping implementation using Core Text
//!
//! This module provides text shaping, font loading, and glyph rendering
//! using Apple's Core Text framework.
//!
//! NOTE: This implementation uses Core Text for font metrics and basic shaping.
//! Full glyph rasterization will be enhanced in future iterations.

use core_text::font::{self, CTFont};
use thiserror::Error;

/// Errors that can occur in text shaping
#[derive(Error, Debug)]
pub enum TextError {
    #[error("Font not found: {0}")]
    FontNotFound(String),

    #[error("Text shaping failed: {0}")]
    ShapingFailed(String),

    #[error("Core Text error: {0}")]
    CoreTextError(String),
}

/// Text shaper using Core Text
pub struct TextShaper {
    font: CTFont,
}

impl TextShaper {
    /// Create a new text shaper with the specified font
    pub fn new(font_name: &str, size: f64) -> Result<Self, TextError> {
        let font = create_font(font_name, size)?;
        Ok(Self { font })
    }

    /// Create a text shaper with the default system font
    pub fn with_system_font(size: f64) -> Self {
        // Try to create system font, fall back to Helvetica
        let font = create_font("SF Pro", size)
            .or_else(|_| create_font(".AppleSystemUIFont", size))
            .or_else(|_| create_font("Helvetica", size))
            .unwrap_or_else(|_| {
                // Ultimate fallback
                font::new_from_name("Helvetica", size)
                    .expect("Failed to create any font")
            });
        Self { font }
    }

    /// Shape text and return glyph information
    pub fn shape(&self, text: &str) -> Result<ShapedText, TextError> {
        // Simple character-to-position mapping
        // For proper shaping, Core Text's CTLine/CTFrame API should be used
        let chars: Vec<char> = text.chars().collect();
        let mut glyphs: Vec<u16> = Vec::with_capacity(chars.len());
        let mut positions: Vec<(f32, f32)> = Vec::with_capacity(chars.len());
        let mut advances: Vec<f32> = Vec::with_capacity(chars.len());
        
        let mut x_pos: f32 = 0.0;
        let font_size = self.font.pt_size() as f32;
        
        for ch in &chars {
            // Use character code as glyph ID (simplified)
            let glyph = *ch as u16;
            
            // Estimate advance based on character type
            let advance = font_size * width_factor(*ch);
            
            glyphs.push(glyph);
            positions.push((x_pos, 0.0));
            advances.push(advance);
            
            x_pos += advance;
        }
        
        Ok(ShapedText {
            glyphs,
            positions,
            advances,
            font: self.font.clone(),
        })
    }

    /// Get font metrics
    pub fn get_metrics(&self) -> FontMetrics {
        FontMetrics {
            ascent: self.font.ascent() as f32,
            descent: self.font.descent() as f32,
            leading: self.font.leading() as f32,
            cap_height: self.font.cap_height() as f32,
            x_height: self.font.x_height() as f32,
        }
    }
    
    /// Get the underlying CTFont
    pub fn font(&self) -> &CTFont {
        &self.font
    }
}

/// Shaped text result
pub struct ShapedText {
    pub glyphs: Vec<u16>,
    pub positions: Vec<(f32, f32)>,
    pub advances: Vec<f32>,
    pub font: CTFont,
}

/// Font metrics
#[derive(Debug, Clone, Copy)]
pub struct FontMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub leading: f32,
    pub cap_height: f32,
    pub x_height: f32,
}

/// Create a CTFont with the specified family and size
pub fn create_font(family: &str, size: f64) -> Result<CTFont, TextError> {
    font::new_from_name(family, size)
        .map_err(|_| TextError::FontNotFound(family.to_string()))
}

/// Rasterize glyphs to bitmaps
/// 
/// This implementation creates simple filled rectangles as glyph placeholders.
/// It avoids the unsafe `std::mem::zeroed()` pattern and will work reliably.
/// Future iterations will add proper Core Text/Core Graphics glyph rasterization.
pub struct GlyphRasterizer {
    font_size: f32,
}

impl GlyphRasterizer {
    /// Create a new glyph rasterizer for a font
    pub fn new(family: &str, size: f64) -> Result<Self, TextError> {
        // Verify the font exists
        let _ = create_font(family, size)?;
        Ok(Self { font_size: size as f32 })
    }
    
    /// Create from a font size (no font object needed for placeholder rendering)
    pub fn with_size(size: f32) -> Self {
        Self { font_size: size }
    }
    
    /// Rasterize a character to an alpha bitmap
    ///
    /// Returns (bitmap, width, height, advance, bearing_x, bearing_y)
    pub fn rasterize_char(&self, ch: char) -> Option<(Vec<u8>, u32, u32, f32, f32, f32)> {
        // Estimate glyph dimensions
        let (width, height) = estimate_glyph_size(ch, self.font_size);
        let width = width.max(4);
        let height = height.max(4);
        
        // Create bitmap
        let mut bitmap = vec![0u8; (width * height) as usize];
        
        // For visible characters, create a simple filled shape
        // Whitespace remains transparent
        if ch.is_ascii_graphic() || ch.is_alphabetic() || ch.is_numeric() {
            // Create a filled rectangle with slight padding
            let pad = 1u32.min(width / 4).min(height / 4);
            for y in pad..(height - pad) {
                for x in pad..(width - pad) {
                    let idx = (y * width + x) as usize;
                    // Create a gradient for visual interest
                    let intensity = 200u8 + ((x * 55) / width) as u8;
                    bitmap[idx] = intensity.min(255);
                }
            }
        }
        
        let advance = self.font_size * width_factor(ch);
        let bearing_y = self.font_size * 0.8;
        
        Some((bitmap, width, height, advance, 0.0, bearing_y))
    }
    
    /// Get glyph ID for a character (simplified - just returns char code)
    pub fn get_glyph(&self, ch: char) -> u16 {
        ch as u16
    }
    
    /// Rasterize a glyph by ID
    pub fn rasterize(&self, glyph: u16) -> Option<(Vec<u8>, u32, u32, f32, f32, f32)> {
        if let Some(ch) = char::from_u32(glyph as u32) {
            self.rasterize_char(ch)
        } else {
            None
        }
    }
}

/// Estimate glyph size based on character and font size
fn estimate_glyph_size(ch: char, font_size: f32) -> (u32, u32) {
    let height = (font_size * 1.2).ceil() as u32;
    let width = (font_size * width_factor(ch)).ceil() as u32;
    (width.max(1), height.max(1))
}

/// Get approximate width factor for a character
fn width_factor(ch: char) -> f32 {
    match ch {
        ' ' => 0.3,
        'i' | 'l' | '!' | '|' | '\'' | '.' | ',' | ':' | ';' => 0.3,
        'f' | 'j' | 't' | 'r' => 0.4,
        'm' | 'w' | 'M' | 'W' | '@' | '%' => 0.9,
        _ if ch.is_ascii_uppercase() => 0.7,
        _ if ch.is_ascii() => 0.55,
        _ => 0.9, // CJK and other wide characters
    }
}

/// Get available system fonts
pub fn get_available_fonts() -> Vec<String> {
    // Return a common list of fonts available on macOS
    vec![
        "Helvetica".to_string(),
        "Helvetica Neue".to_string(),
        "Arial".to_string(),
        "Times New Roman".to_string(),
        "Courier New".to_string(),
        "Georgia".to_string(),
        "Verdana".to_string(),
        "SF Pro".to_string(),
        "SF Mono".to_string(),
        "Menlo".to_string(),
        "Monaco".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_create_font() {
        let font = create_font("Helvetica", 16.0);
        assert!(font.is_ok(), "Should create Helvetica font");
    }
    
    #[test]
    fn test_text_shaper() {
        let shaper = TextShaper::with_system_font(16.0);
        let result = shaper.shape("Hello");
        assert!(result.is_ok());
        let shaped = result.unwrap();
        assert_eq!(shaped.glyphs.len(), 5);
    }
    
    #[test]
    fn test_font_metrics() {
        let shaper = TextShaper::with_system_font(16.0);
        let metrics = shaper.get_metrics();
        assert!(metrics.ascent > 0.0);
    }
    
    #[test]
    fn test_glyph_rasterizer() {
        let rasterizer = GlyphRasterizer::with_size(16.0);
        
        let result = rasterizer.rasterize_char('A');
        assert!(result.is_some(), "Should rasterize character");
        
        let (bitmap, width, height, advance, _, _) = result.unwrap();
        assert!(width > 0);
        assert!(height > 0);
        assert!(advance > 0.0);
        assert!(!bitmap.is_empty());
        
        // Check that the bitmap has non-zero values (not all transparent)
        let has_content = bitmap.iter().any(|&b| b > 0);
        assert!(has_content, "Bitmap should have visible content");
    }
    
    #[test]
    fn test_whitespace_transparent() {
        let rasterizer = GlyphRasterizer::with_size(16.0);
        
        let result = rasterizer.rasterize_char(' ');
        assert!(result.is_some());
        
        let (bitmap, _, _, _, _, _) = result.unwrap();
        // Whitespace should be transparent (all zeros)
        let all_transparent = bitmap.iter().all(|&b| b == 0);
        assert!(all_transparent, "Whitespace should be transparent");
    }
    
    #[test]
    fn test_width_factors() {
        // Narrow characters should have smaller width factor
        assert!(width_factor('i') < width_factor('m'));
        assert!(width_factor('.') < width_factor('W'));
    }
}
