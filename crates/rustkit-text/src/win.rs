use crate::{FontMetrics, FontStretch, FontStyle, FontWeight, GlyphMetrics, TextBackendError};
use std::sync::OnceLock;
use windows::core::{PCWSTR, BOOL};
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

#[derive(Clone)]
pub struct FontCollection {
    collection: IDWriteFontCollection,
}

pub struct FontFamily {
    family: IDWriteFontFamily,
}

pub struct Font {
    font: IDWriteFont,
}

#[derive(Clone)]
pub struct FontFace {
    face: IDWriteFontFace,
}

struct DWriteContext {
    factory: IDWriteFactory,
}

fn ctx() -> Result<&'static DWriteContext, TextBackendError> {
    static CTX: OnceLock<Result<DWriteContext, TextBackendError>> = OnceLock::new();
    let res = CTX.get_or_init(|| init_ctx());
    res.as_ref().map_err(Clone::clone)
}

fn init_ctx() -> Result<DWriteContext, TextBackendError> {
    // Ensure COM is initialized for this process; ignore mode mismatches.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }

    // Create DirectWrite factory (windows crate provides a generic helper)
    let factory: IDWriteFactory = unsafe { DWriteCreateFactory::<IDWriteFactory>(DWRITE_FACTORY_TYPE_SHARED) }
        .map_err(|e| TextBackendError::DirectWrite(format!("{e:?}")))?;

    Ok(DWriteContext { factory })
}

impl FontCollection {
    pub fn system() -> Result<Self, TextBackendError> {
        let ctx = ctx()?;
        let mut collection: Option<IDWriteFontCollection> = None;
        unsafe {
            ctx.factory
                .GetSystemFontCollection(&mut collection, false)
                .map_err(|e| TextBackendError::DirectWrite(format!("{e:?}")))?;
        }
        Ok(Self {
            collection: collection.ok_or_else(|| {
                TextBackendError::DirectWrite("GetSystemFontCollection returned None".into())
            })?,
        })
    }

    pub fn font_family_by_name(&self, name: &str) -> Result<Option<FontFamily>, TextBackendError> {
        let name_w = to_wide_null(name);
        let mut index: u32 = 0;
        let mut exists = BOOL(0);
        unsafe {
            self.collection
                .FindFamilyName(PCWSTR(name_w.as_ptr()), &mut index, &mut exists)
                .map_err(|e| TextBackendError::DirectWrite(format!("{e:?}")))?;
        }
        if !exists.as_bool() {
            return Ok(None);
        }
        let family = unsafe { self.collection.GetFontFamily(index) }
            .map_err(|e| TextBackendError::DirectWrite(format!("{e:?}")))?;
        Ok(Some(FontFamily { family }))
    }
}

impl FontFamily {
    pub fn first_matching_font(
        &self,
        weight: FontWeight,
        stretch: FontStretch,
        style: FontStyle,
    ) -> Result<Font, TextBackendError> {
        let dw_weight = DWRITE_FONT_WEIGHT(weight.0 as i32);
        let dw_stretch = DWRITE_FONT_STRETCH(stretch.0 as i32);
        let dw_style = match style {
            FontStyle::Normal => DWRITE_FONT_STYLE_NORMAL,
            FontStyle::Italic => DWRITE_FONT_STYLE_ITALIC,
            FontStyle::Oblique => DWRITE_FONT_STYLE_OBLIQUE,
        };
        let font = unsafe { self.family.GetFirstMatchingFont(dw_weight, dw_stretch, dw_style) }
            .map_err(|e| TextBackendError::DirectWrite(format!("{e:?}")))?;
        Ok(Font { font })
    }
}

impl Font {
    pub fn create_font_face(&self) -> Result<FontFace, TextBackendError> {
        let face = unsafe { self.font.CreateFontFace() }
            .map_err(|e| TextBackendError::DirectWrite(format!("{e:?}")))?;
        Ok(FontFace { face })
    }
}

impl FontFace {
    pub fn metrics(&self) -> Result<FontMetrics, TextBackendError> {
        let mut m = DWRITE_FONT_METRICS::default();
        unsafe { self.face.GetMetrics(&mut m) };
        Ok(FontMetrics {
            design_units_per_em: m.designUnitsPerEm,
            ascent: m.ascent,
            descent: m.descent,
            line_gap: m.lineGap,
            underline_position: m.underlinePosition,
            underline_thickness: m.underlineThickness,
            strikethrough_position: m.strikethroughPosition,
            strikethrough_thickness: m.strikethroughThickness,
        })
    }

    pub fn glyph_indices(&self, codepoints: &[u32]) -> Result<Vec<u16>, TextBackendError> {
        let mut out = vec![0u16; codepoints.len()];
        unsafe {
            self.face
                .GetGlyphIndices(codepoints.as_ptr(), codepoints.len() as u32, out.as_mut_ptr())
                .map_err(|e| TextBackendError::DirectWrite(format!("{e:?}")))?;
        }
        Ok(out)
    }

    pub fn design_glyph_metrics(
        &self,
        glyph_indices: &[u16],
        is_sideways: bool,
    ) -> Result<Vec<GlyphMetrics>, TextBackendError> {
        let mut metrics = vec![DWRITE_GLYPH_METRICS::default(); glyph_indices.len()];
        unsafe {
            self.face
                .GetDesignGlyphMetrics(
                    glyph_indices.as_ptr(),
                    glyph_indices.len() as u32,
                    metrics.as_mut_ptr(),
                    is_sideways,
                )
                .map_err(|e| TextBackendError::DirectWrite(format!("{e:?}")))?;
        }
        Ok(metrics
            .into_iter()
            .map(|m| GlyphMetrics {
                advance_width: m.advanceWidth as i32,
            })
            .collect())
    }
}

fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}


