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

/// True for CSS keywords naming the macOS system font. It has no
/// instantiable PostScript name — it resolves ONLY through the UI-font API.
fn is_system_family(lower: &str) -> bool {
    matches!(
        lower,
        "system-ui" | "-apple-system" | "blinkmacsystemfont" | ".applesystemuifont"
    )
}

/// Map CSS generic families to concrete macOS fonts.
fn map_generic(lower: &str, fam: &str) -> &'static str {
    match lower {
        "sans-serif" => "Helvetica",
        "serif" => "Times New Roman",
        "monospace" => "Menlo",
        _ => {
            // Not generic: caller uses the original string.
            let _ = fam;
            ""
        }
    }
}

/// Map a CSS font-weight (100..900) to `kCTFontWeightTrait` (-1.0 ..= 1.0).
///
/// This is Skia's table (`SkTypeface_mac` / `SkFontHost_mac`), which is what
/// Chrome uses to resolve `system-ui` weights on macOS. The anchors are
/// Apple's own constants: Ultralight -0.80, Thin -0.60, Light -0.40,
/// Regular 0.0, Medium 0.23, Semibold 0.30, Bold 0.40, Heavy 0.56, Black 0.62.
/// Intermediate CSS weights interpolate linearly between neighbours.
pub fn ct_weight_trait(css_weight: u16) -> f64 {
    const TABLE: [(u16, f64); 9] = [
        (100, -0.80),
        (200, -0.60),
        (300, -0.40),
        (400, 0.00),
        (500, 0.23),
        (600, 0.30),
        (700, 0.40),
        (800, 0.56),
        (900, 0.62),
    ];
    let w = css_weight.clamp(1, 1000);
    if w <= TABLE[0].0 {
        return TABLE[0].1;
    }
    for pair in TABLE.windows(2) {
        let (w0, t0) = pair[0];
        let (w1, t1) = pair[1];
        if w <= w1 {
            let f = (w - w0) as f64 / (w1 - w0) as f64;
            return t0 + (t1 - t0) * f;
        }
    }
    TABLE[8].1
}

/// The macOS system font at `size`, in the face matching `css_weight`.
///
/// The system font has no instantiable PostScript name and no by-name trait
/// variants, so it resolves only through the UI-font API. That API exposes
/// exactly TWO faces (`kCTFontSystemFontType` = Regular,
/// `kCTFontEmphasizedSystemFontType` = Bold) — which is why the old
/// `weight >= 600 ? bold : regular` gate collapsed all of 100..500 onto
/// `.SFNS-Regular` and 600..900 onto `.SFNS-Bold`. Chrome does not: it applies
/// `kCTFontWeightTrait` to the descriptor and gets the real face.
///
/// Receipts (20px, `about`'s `.tagline`, `parity-tests/probe/ct_weight_advance.py`):
/// Light 659.1px vs Regular 670.0px — an 11px error on one line of `font-weight:300`
/// text, enough to wrap a string Chrome fits.
pub fn create_system_font_with_weight(size: f64, css_weight: u16) -> CTFont {
    let base = font::new_ui_font_for_language(font::kCTFontSystemFontType, size, None);
    if css_weight == 400 {
        return base;
    }
    apply_weight_trait(&base, size, css_weight).unwrap_or_else(|| {
        // Descriptor matching failed (unusual): keep the old two-face split
        // rather than silently returning Regular for bold text.
        let ui_type = if css_weight >= 600 {
            font::kCTFontEmphasizedSystemFontType
        } else {
            font::kCTFontSystemFontType
        };
        font::new_ui_font_for_language(ui_type, size, None)
    })
}

/// Copy `base`'s descriptor with `kCTFontWeightTrait` overridden, and
/// re-instantiate at `size`. Returns `None` if Core Text can't match.
fn apply_weight_trait(base: &CTFont, size: f64, css_weight: u16) -> Option<CTFont> {
    use core_foundation::base::CFType;
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use core_text::font_descriptor;

    let trait_key = unsafe { CFString::wrap_under_get_rule(font_descriptor::kCTFontWeightTrait) };
    let traits_key =
        unsafe { CFString::wrap_under_get_rule(font_descriptor::kCTFontTraitsAttribute) };

    let weight_num = CFNumber::from(ct_weight_trait(css_weight));
    let traits: CFDictionary<CFString, CFType> =
        CFDictionary::from_CFType_pairs(&[(trait_key, weight_num.as_CFType())]);
    let attrs: CFDictionary<CFString, CFType> =
        CFDictionary::from_CFType_pairs(&[(traits_key, traits.as_CFType())]);

    let desc = base
        .copy_descriptor()
        .create_copy_with_attributes(attrs.into_untyped())
        .ok()?;
    Some(font::new_from_descriptor(&desc, size))
}

/// Create a CTFont with the specified family and size.
///
/// `family` may be a raw CSS font-family LIST ("system-ui, -apple-system,
/// sans-serif"): entries are tried in order with generic keywords mapped to
/// real fonts. Before 2026-07-10 the whole list was passed verbatim to
/// new_from_name, which failed on any multi-family value — the renderer
/// painted Helvetica for every styled page regardless of the author's fonts.
pub fn create_font(family: &str, size: f64) -> Result<CTFont, TextError> {
    for fam in family.split(',') {
        let fam = fam.trim().trim_matches('"').trim_matches('\'');
        if fam.is_empty() {
            continue;
        }
        let lower = fam.to_ascii_lowercase();
        if is_system_family(&lower) {
            return Ok(font::new_ui_font_for_language(
                font::kCTFontSystemFontType,
                size,
                None,
            ));
        }
        let mapped = map_generic(&lower, fam);
        let name = if mapped.is_empty() { fam } else { mapped };
        if let Ok(f) = font::new_from_name(name, size) {
            return Ok(f);
        }
    }
    font::new_from_name(family, size).map_err(|_| TextError::FontNotFound(family.to_string()))
}

/// Create a font with specific weight and style traits
fn create_font_with_traits(
    family: &str,
    size: f64,
    weight: u16,
    italic: bool,
) -> Result<CTFont, TextError> {
    for fam in family.split(',') {
        let fam = fam.trim().trim_matches('"').trim_matches('\'');
        if fam.is_empty() {
            continue;
        }
        let lower = fam.to_ascii_lowercase();

        // System font: resolve the real weighted face via kCTFontWeightTrait,
        // as Chrome/Skia do. "-Bold" name variants never exist for it, and the
        // UI-font API alone offers only Regular/Bold — so the previous
        // `weight >= 600` gate shaped 300 as Regular and 600 as Bold.
        if is_system_family(&lower) && !italic {
            return Ok(create_system_font_with_weight(size, weight));
        }

        let mapped = map_generic(&lower, fam);
        let base = if mapped.is_empty() { fam } else { mapped };

        let mut variants: Vec<String> = Vec::new();
        if weight >= 700 && italic {
            variants.push(format!("{}-BoldItalic", base));
        }
        if weight >= 700 {
            variants.push(format!("{}-Bold", base));
            variants.push(format!("{}Bold", base));
        }
        if italic {
            variants.push(format!("{}-Italic", base));
            variants.push(format!("{}-Oblique", base));
        }
        variants.push(base.to_string());

        for v in &variants {
            if let Ok(f) = font::new_from_name(v, size) {
                return Ok(f);
            }
        }
    }

    // Fall back to base font resolution over the same list
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
    fn test_create_font_css_family_list() {
        // Regression (2026-07-10): a raw CSS family list was passed verbatim
        // to new_from_name, failing on any multi-family value — the renderer
        // painted Helvetica for every styled page.
        let font = create_font("system-ui, -apple-system, sans-serif", 16.0);
        assert!(font.is_ok(), "CSS family list should resolve");
        let font = create_font("\"NoSuchFont-XYZ\", Arial", 16.0);
        assert!(font.is_ok(), "fallback within the list should resolve");
    }

    #[test]
    fn test_system_font_bold_face() {
        // The system font has no "-Bold" PostScript variant; weight >= 600
        // must route through the UI-font API and return the emphasized face
        // (the old path silently rasterized bold headings with the regular
        // face, ~6% narrower than Chrome).
        let regular = create_font_with_traits("system-ui", 32.0, 400, false).unwrap();
        let bold = create_font_with_traits("system-ui", 32.0, 700, false).unwrap();
        let (rn, bn) = (regular.postscript_name(), bold.postscript_name());
        assert_ne!(rn, bn, "bold system face must differ from regular");
        assert!(
            bn.to_lowercase().contains("bold"),
            "emphasized face expected, got {bn}"
        );
    }
    
    #[test]
    fn test_system_font_resolves_every_css_weight_to_its_own_face() {
        // The UI-font API exposes only Regular and Bold, so the old
        // `weight >= 600` gate collapsed 100..500 onto .SFNS-Regular and
        // 600..900 onto .SFNS-Bold. Chrome/Skia apply kCTFontWeightTrait.
        // Drives the real resolution path (create_font_with_traits), not a
        // hand-simulated table.
        let faces: Vec<(u16, String)> = [100u16, 200, 300, 400, 500, 600, 700, 800, 900]
            .iter()
            .map(|&w| {
                let f = create_font_with_traits("system-ui", 32.0, w, false).unwrap();
                (w, f.postscript_name())
            })
            .collect();

        // Every step must land on a distinct face — no two CSS weights share one.
        for pair in faces.windows(2) {
            assert_ne!(
                pair[0].1, pair[1].1,
                "weights {} and {} collapsed onto the same face {}",
                pair[0].0, pair[1].0, pair[0].1
            );
        }

        // The specific regression: 300 must be lighter than 400, and 600 must
        // NOT be the same face as 700 (the two ends of the old binary gate).
        let name = |w: u16| faces.iter().find(|(x, _)| *x == w).unwrap().1.clone();
        assert!(
            name(300).to_lowercase().contains("light"),
            "font-weight:300 must resolve to the Light face, got {}",
            name(300)
        );
        assert_ne!(
            name(600),
            name(700),
            "semibold and bold must not share a face"
        );
    }

    #[test]
    fn test_ct_weight_trait_table_matches_skia_anchors() {
        // Apple's kCTFontWeight* constants, which is what Skia's table encodes.
        assert_eq!(ct_weight_trait(100), -0.80);
        assert_eq!(ct_weight_trait(300), -0.40);
        assert_eq!(ct_weight_trait(400), 0.00);
        assert_eq!(ct_weight_trait(700), 0.40);
        assert_eq!(ct_weight_trait(900), 0.62);
        // Off-table CSS weights interpolate and stay monotonic.
        let t350 = ct_weight_trait(350);
        assert!(t350 > -0.40 && t350 < 0.0, "350 between Light and Regular");
        assert_eq!(ct_weight_trait(1000), 0.62, "clamped at the top");
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
