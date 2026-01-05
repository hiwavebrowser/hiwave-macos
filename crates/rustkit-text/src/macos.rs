//! macOS text shaping implementation using Core Text
//!
//! This module provides text shaping, font loading, and glyph rendering
//! using Apple's Core Text framework.

use core_foundation::base::TCFType;
use core_graphics::base::CGFloat;
use core_graphics::color_space::CGColorSpace;
use core_graphics::context::CGContext;
use core_graphics::geometry::{CGPoint, CGRect, CGSize};
use core_text::font::{self, CTFont};
use foreign_types_shared::ForeignType;
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
        // Convert text to UTF-16 for Core Text
        let utf16_chars: Vec<u16> = text.encode_utf16().collect();
        let char_count = utf16_chars.len();
        
        if char_count == 0 {
            return Ok(ShapedText {
                glyphs: vec![],
                positions: vec![],
                advances: vec![],
                font: self.font.clone(),
            });
        }
        
        // Allocate space for glyphs
        let mut glyphs: Vec<core_graphics::font::CGGlyph> = vec![0; char_count];
        
        // Get glyph IDs using Core Text
        unsafe {
            extern "C" {
                fn CTFontGetGlyphsForCharacters(
                    font: core_text::font::CTFontRef,
                    characters: *const u16,
                    glyphs: *mut core_graphics::font::CGGlyph,
                    count: isize,
                ) -> bool;
            }
            
            let success = CTFontGetGlyphsForCharacters(
                self.font.as_concrete_TypeRef(),
                utf16_chars.as_ptr(),
                glyphs.as_mut_ptr(),
                char_count as isize,
            );
            
            // Some glyphs may not be available, but continue silently
            let _ = success;
        }
        
        // Get advances for each glyph
        let mut glyph_advances: Vec<CGSize> = vec![CGSize::new(0.0, 0.0); char_count];
        unsafe {
            extern "C" {
                fn CTFontGetAdvancesForGlyphs(
                    font: core_text::font::CTFontRef,
                    orientation: u32, // kCTFontOrientationDefault = 0
                    glyphs: *const core_graphics::font::CGGlyph,
                    advances: *mut CGSize,
                    count: isize,
                ) -> f64;
            }
            
            let _total_advance = CTFontGetAdvancesForGlyphs(
                self.font.as_concrete_TypeRef(),
                0, // kCTFontOrientationDefault
                glyphs.as_ptr(),
                glyph_advances.as_mut_ptr(),
                char_count as isize,
            );
        }
        
        // Calculate positions from advances
        let mut positions: Vec<(f32, f32)> = Vec::with_capacity(char_count);
        let mut advances: Vec<f32> = Vec::with_capacity(char_count);
        let mut x_pos: f64 = 0.0;
        
        for (i, glyph_advance) in glyph_advances.iter().enumerate() {
            positions.push((x_pos as f32, 0.0));
            advances.push(glyph_advance.width as f32);
            x_pos += glyph_advance.width;
            
            // Handle missing glyphs (glyph ID 0)
            if glyphs[i] == 0 {
                // Use a fallback advance for missing glyphs
                let fallback_advance = self.font.pt_size() * 0.5;
                if advances.last().map(|a| *a == 0.0).unwrap_or(false) {
                    if let Some(last) = advances.last_mut() {
                        *last = fallback_advance as f32;
                    }
                }
            }
        }
        
        // Convert CGGlyph to u16
        let glyphs: Vec<u16> = glyphs.into_iter().map(|g| g as u16).collect();
        
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

/// Create a font with specific weight and style traits
fn create_font_with_traits(
    family: &str,
    size: f64,
    weight: u16,
    italic: bool,
) -> Result<CTFont, TextError> {
    // Map CSS font-weight to Core Text weight trait
    // CSS: 100-900, Core Text: -1.0 to 1.0
    // 400 = normal (0.0), 700 = bold (~0.4)
    let weight_trait = match weight {
        0..=199 => -0.8,      // Thin
        200..=299 => -0.6,    // ExtraLight
        300..=399 => -0.4,    // Light
        400..=499 => 0.0,     // Normal
        500..=599 => 0.23,    // Medium
        600..=699 => 0.3,     // SemiBold
        700..=799 => 0.4,     // Bold
        800..=899 => 0.56,    // ExtraBold
        _ => 0.62,            // Black
    };
    
    // First try to find a font with the exact traits
    // For bold, try appending "-Bold" or "Bold" to the family name
    let bold_family = if weight >= 700 {
        format!("{}-Bold", family)
    } else {
        family.to_string()
    };
    
    let italic_family = if italic {
        format!("{}-Italic", bold_family)
    } else {
        bold_family
    };
    
    // Try the specific variant first
    if let Ok(f) = font::new_from_name(&italic_family, size) {
        return Ok(f);
    }
    
    // Try bold variant
    if weight >= 700 {
        if let Ok(f) = font::new_from_name(&format!("{}-Bold", family), size) {
            return Ok(f);
        }
        if let Ok(f) = font::new_from_name(&format!("{}Bold", family), size) {
            return Ok(f);
        }
    }
    
    // Fall back to base font
    create_font(family, size)
}

/// Rasterize glyphs to bitmaps using Core Text/Core Graphics
pub struct GlyphRasterizer {
    font: CTFont,
    font_size: f32,
    font_weight: u16,
    font_italic: bool,
}

impl GlyphRasterizer {
    /// Create a new glyph rasterizer for a font
    pub fn new(family: &str, size: f64) -> Result<Self, TextError> {
        let font = create_font(family, size)?;
        Ok(Self { 
            font,
            font_size: size as f32,
            font_weight: 400,
            font_italic: false,
        })
    }
    
    /// Create with default system font
    pub fn with_size(size: f32) -> Self {
        let font = create_font("Helvetica", size as f64)
            .or_else(|_| create_font("Arial", size as f64))
            .unwrap_or_else(|_| font::new_from_name("Helvetica", size as f64).unwrap());
        Self { 
            font,
            font_size: size,
            font_weight: 400,
            font_italic: false,
        }
    }
    
    /// Create with specific weight and style
    pub fn with_style(family: &str, size: f32, weight: u16, italic: bool) -> Self {
        let font = create_font_with_traits(family, size as f64, weight, italic)
            .or_else(|_| create_font_with_traits("Helvetica", size as f64, weight, italic))
            .unwrap_or_else(|_| font::new_from_name("Helvetica", size as f64).unwrap());
        Self {
            font,
            font_size: size,
            font_weight: weight,
            font_italic: italic,
        }
    }
    
    /// Get font weight
    pub fn weight(&self) -> u16 {
        self.font_weight
    }
    
    /// Get whether font is italic
    pub fn is_italic(&self) -> bool {
        self.font_italic
    }
    
    /// Rasterize a character to an alpha bitmap using Core Graphics
    ///
    /// Returns (bitmap, width, height, advance, bearing_x, bearing_y)
    pub fn rasterize_char(&self, ch: char) -> Option<(Vec<u8>, u32, u32, f32, f32, f32)> {
        // Get glyph for character
        let chars: [u16; 1] = [ch as u16];
        let mut glyphs: [u16; 1] = [0];
        
        unsafe {
            use core_text::font::CTFontRef;
            use std::os::raw::c_void;
            
            // Get the raw CTFont reference
            let font_ref = self.font.as_concrete_TypeRef();
            
            // Get glyph ID for the character
            extern "C" {
                fn CTFontGetGlyphsForCharacters(
                    font: CTFontRef,
                    characters: *const u16,
                    glyphs: *mut u16,
                    count: isize,
                ) -> bool;
                
                fn CTFontGetAdvancesForGlyphs(
                    font: CTFontRef,
                    orientation: u32,
                    glyphs: *const u16,
                    advances: *mut CGSize,
                    count: isize,
                ) -> f64;
                
                fn CTFontGetBoundingRectsForGlyphs(
                    font: CTFontRef,
                    orientation: u32,
                    glyphs: *const u16,
                    bounding_rects: *mut CGRect,
                    count: isize,
                ) -> CGRect;
                
                fn CTFontDrawGlyphs(
                    font: CTFontRef,
                    glyphs: *const u16,
                    positions: *const CGPoint,
                    count: usize,
                    context: *mut c_void,
                );
            }
            
            let success = CTFontGetGlyphsForCharacters(
                font_ref,
                chars.as_ptr(),
                glyphs.as_mut_ptr(),
                1,
            );
            
            if !success || glyphs[0] == 0 {
                // Fallback for characters without glyphs
                return self.rasterize_fallback(ch);
            }
            
            // Get glyph advance
            let mut advance_size = CGSize::new(0.0, 0.0);
            CTFontGetAdvancesForGlyphs(
                font_ref,
                0, // kCTFontOrientationHorizontal
                glyphs.as_ptr(),
                &mut advance_size,
                1,
            );
            
            // Get glyph bounding rect
            let mut bounds = CGRect::new(&CGPoint::new(0.0, 0.0), &CGSize::new(0.0, 0.0));
            CTFontGetBoundingRectsForGlyphs(
                font_ref,
                0,
                glyphs.as_ptr(),
                &mut bounds,
                1,
            );
            
            // Calculate bitmap dimensions with padding
            let padding = 2.0;
            let width = (bounds.size.width.ceil() + padding * 2.0).max(4.0) as u32;
            let height = (bounds.size.height.ceil() + padding * 2.0).max(4.0) as u32;
            
            // Create grayscale bitmap context
            let color_space = CGColorSpace::create_device_gray();
            let mut context = CGContext::create_bitmap_context(
                None,
                width as usize,
                height as usize,
                8,  // bits per component
                width as usize,  // bytes per row
                &color_space,
                0,  // kCGImageAlphaNone for grayscale
            );
            
            // Set up drawing context
            // Fill with black (transparent in our alpha usage)
            context.set_rgb_fill_color(0.0, 0.0, 0.0, 1.0);
            context.fill_rect(CGRect::new(
                &CGPoint::new(0.0, 0.0),
                &CGSize::new(width as CGFloat, height as CGFloat),
            ));
            
            // Set text color to white (opaque)
            context.set_rgb_fill_color(1.0, 1.0, 1.0, 1.0);
            
            // Calculate position to draw glyph
            // Origin is at bottom-left, glyph origin needs adjustment
            let x = padding - bounds.origin.x;
            let y = padding - bounds.origin.y;
            
            let positions = [CGPoint::new(x, y)];
            
            // Draw the glyph
            CTFontDrawGlyphs(
                font_ref,
                glyphs.as_ptr(),
                positions.as_ptr(),
                1,
                context.as_ptr() as *mut c_void,
            );
            
            // Extract bitmap data
            let data = context.data();
            let bitmap: Vec<u8> = data.to_vec();
            
            let advance = advance_size.width as f32;
            let bearing_x = bounds.origin.x as f32;
            let bearing_y = (bounds.origin.y + bounds.size.height) as f32;
            
            Some((bitmap, width, height, advance, bearing_x, bearing_y))
        }
    }
    
    /// Fallback rasterization for characters without glyphs
    fn rasterize_fallback(&self, ch: char) -> Option<(Vec<u8>, u32, u32, f32, f32, f32)> {
        // Try fallback fonts for the character
        let fallback_fonts = [
            "Apple Color Emoji",  // For emoji
            "Apple Symbols",       // For symbols
            "Arial Unicode MS",    // Wide Unicode coverage
            "Helvetica Neue",      // Good general fallback
            "Menlo",               // For code/math symbols
        ];
        
        for font_name in &fallback_fonts {
            if let Ok(fallback_font) = font::new_from_name(font_name, self.font_size as f64) {
                // Try to get glyph with this fallback font
                let chars: [u16; 1] = [ch as u16];
                let mut glyphs: [u16; 1] = [0];
                
                unsafe {
                    use core_text::font::CTFontRef;
                    
                    extern "C" {
                        fn CTFontGetGlyphsForCharacters(
                            font: CTFontRef,
                            characters: *const u16,
                            glyphs: *mut u16,
                            count: isize,
                        ) -> bool;
                    }
                    
                    let success = CTFontGetGlyphsForCharacters(
                        fallback_font.as_concrete_TypeRef(),
                        chars.as_ptr(),
                        glyphs.as_mut_ptr(),
                        1,
                    );
                    
                    if success && glyphs[0] != 0 {
                        // Found the glyph in this fallback font - rasterize with it
                        let fallback_rasterizer = GlyphRasterizer {
                            font: fallback_font.clone(),
                            font_size: self.font_size,
                            font_weight: self.font_weight,
                            font_italic: self.font_italic,
                        };
                        if let Some(result) = fallback_rasterizer.rasterize_char_with_font(&fallback_font, ch) {
                            return Some(result);
                        }
                    }
                }
            }
        }
        
        // No fallback found - return transparent placeholder
        let (width, height) = estimate_glyph_size(ch, self.font_size);
        let width = width.max(4);
        let height = height.max(4);
        let bitmap = vec![0u8; (width * height) as usize];
        
        let advance = self.font_size * width_factor(ch);
        let bearing_y = self.font_size * 0.8;
        
        Some((bitmap, width, height, advance, 0.0, bearing_y))
    }
    
    /// Rasterize a character using a specific font (for fallback)
    fn rasterize_char_with_font(&self, font: &CTFont, ch: char) -> Option<(Vec<u8>, u32, u32, f32, f32, f32)> {
        let chars: [u16; 1] = [ch as u16];
        let mut glyphs: [u16; 1] = [0];
        
        unsafe {
            use core_text::font::CTFontRef;
            use std::os::raw::c_void;
            
            extern "C" {
                fn CTFontGetGlyphsForCharacters(
                    font: CTFontRef,
                    characters: *const u16,
                    glyphs: *mut u16,
                    count: isize,
                ) -> bool;
                
                fn CTFontGetAdvancesForGlyphs(
                    font: CTFontRef,
                    orientation: u32,
                    glyphs: *const u16,
                    advances: *mut CGSize,
                    count: isize,
                ) -> f64;
                
                fn CTFontGetBoundingRectsForGlyphs(
                    font: CTFontRef,
                    orientation: u32,
                    glyphs: *const u16,
                    bounding_rects: *mut CGRect,
                    count: isize,
                ) -> CGRect;
                
                fn CTFontDrawGlyphs(
                    font: CTFontRef,
                    glyphs: *const u16,
                    positions: *const CGPoint,
                    count: usize,
                    context: *mut c_void,
                );
            }
            
            let font_ref = font.as_concrete_TypeRef();
            
            let success = CTFontGetGlyphsForCharacters(
                font_ref,
                chars.as_ptr(),
                glyphs.as_mut_ptr(),
                1,
            );
            
            if !success || glyphs[0] == 0 {
                return None;
            }
            
            let mut advance_size = CGSize::new(0.0, 0.0);
            CTFontGetAdvancesForGlyphs(
                font_ref,
                0,
                glyphs.as_ptr(),
                &mut advance_size,
                1,
            );
            
            let mut bounds = CGRect::new(&CGPoint::new(0.0, 0.0), &CGSize::new(0.0, 0.0));
            CTFontGetBoundingRectsForGlyphs(
                font_ref,
                0,
                glyphs.as_ptr(),
                &mut bounds,
                1,
            );
            
            let padding = 2.0;
            let width = (bounds.size.width.ceil() + padding * 2.0).max(4.0) as u32;
            let height = (bounds.size.height.ceil() + padding * 2.0).max(4.0) as u32;
            
            let color_space = CGColorSpace::create_device_gray();
            let mut context = CGContext::create_bitmap_context(
                None,
                width as usize,
                height as usize,
                8,
                width as usize,
                &color_space,
                0,
            );
            
            context.set_allows_antialiasing(true);
            context.set_should_antialias(true);
            context.set_should_smooth_fonts(true);
            context.set_gray_fill_color(1.0, 1.0);
            
            let draw_x = padding - bounds.origin.x;
            let draw_y = padding - bounds.origin.y;
            
            let position = CGPoint::new(draw_x, draw_y);
            CTFontDrawGlyphs(
                font_ref,
                glyphs.as_ptr(),
                &position,
                1,
                context.as_ptr() as *mut c_void,
            );
            
            let data = context.data();
            let bitmap: Vec<u8> = std::slice::from_raw_parts(
                data.as_ptr() as *const u8,
                (width * height) as usize,
            ).to_vec();
            
            let advance = advance_size.width as f32;
            let bearing_x = bounds.origin.x as f32;
            let bearing_y = (bounds.origin.y + bounds.size.height) as f32;
            
            Some((bitmap, width, height, advance, bearing_x, bearing_y))
        }
    }
    
    /// Get glyph ID for a character
    pub fn get_glyph(&self, ch: char) -> u16 {
        ch as u16
    }
    
    /// Rasterize a glyph by character (we use char code as ID)
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
