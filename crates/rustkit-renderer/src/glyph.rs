//! Glyph cache for text rendering.
//!
//! Caches rasterized glyphs in a GPU texture atlas.

use crate::RendererError;
use hashbrown::HashMap;
#[cfg(windows)]
use rustkit_text::{FontCollection as RkFontCollection, FontStretch as RkFontStretch, FontStyle as RkFontStyle, FontWeight as RkFontWeight};

/// Key for identifying a specific glyph.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct GlyphKey {
    pub codepoint: char,
    pub font_family: String,
    pub font_size: u32, // Fixed-point (size * 10)
    pub font_weight: u16,
    pub font_style: u8, // 0 = normal, 1 = italic
}

/// Cached glyph entry.
#[derive(Debug, Clone)]
pub struct GlyphEntry {
    /// Texture coordinates in atlas [u0, v0, u1, v1].
    pub tex_coords: [f32; 4],
    /// Offset from cursor position.
    pub offset: [f32; 2],
    /// Horizontal advance.
    pub advance: f32,
}

/// Glyph atlas for caching rasterized glyphs.
pub struct GlyphCache {
    atlas: wgpu::Texture,
    _atlas_view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    atlas_size: u32,
    entries: HashMap<GlyphKey, GlyphEntry>,
    next_x: u32,
    next_y: u32,
    row_height: u32,
}

impl GlyphCache {
    /// Default atlas size (2048x2048).
    pub const DEFAULT_ATLAS_SIZE: u32 = 2048;

    /// Create a new glyph cache.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bind_group_layout: wgpu::BindGroupLayout,
    ) -> Result<Self, RendererError> {
        let atlas_size = Self::DEFAULT_ATLAS_SIZE;

        // Create atlas texture
        let atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Glyph Atlas"),
            size: wgpu::Extent3d {
                width: atlas_size,
                height: atlas_size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        // Initialize with transparent
        let empty_data = vec![0u8; (atlas_size * atlas_size) as usize];
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &atlas,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &empty_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(atlas_size),
                rows_per_image: Some(atlas_size),
            },
            wgpu::Extent3d {
                width: atlas_size,
                height: atlas_size,
                depth_or_array_layers: 1,
            },
        );

        let atlas_view = atlas.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
            label: Some("glyph_atlas_bind_group"),
        });

        Ok(Self {
            atlas,
            _atlas_view: atlas_view,
            bind_group,
            atlas_size,
            entries: HashMap::new(),
            next_x: 1, // Start at 1 to avoid edge artifacts
            next_y: 1,
            row_height: 0,
        })
    }

    /// Get the atlas size.
    pub fn atlas_size(&self) -> u32 {
        self.atlas_size
    }

    /// Get the bind group for the atlas texture.
    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    /// Get or rasterize a glyph.
    pub fn get_or_rasterize(
        &mut self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: &GlyphKey,
    ) -> Option<GlyphEntry> {
        if let Some(entry) = self.entries.get(key) {
            return Some(entry.clone());
        }

        // Rasterize using fallback (simple rectangle placeholder)
        self.rasterize_glyph_fallback(queue, key)
    }

    /// Rasterize a glyph using platform-specific text rendering.
    fn rasterize_glyph_fallback(
        &mut self,
        queue: &wgpu::Queue,
        key: &GlyphKey,
    ) -> Option<GlyphEntry> {
        let font_size = key.font_size as f32 / 10.0;
        
        // Use platform-specific glyph rasterization
        #[cfg(target_os = "macos")]
        let raster_result = {
            let rasterizer = rustkit_text::macos::GlyphRasterizer::with_size(font_size);
            rasterizer.rasterize_char(key.codepoint)
        };
        
        #[cfg(windows)]
        let raster_result = {
            // Windows fallback - use simple placeholder
            let (glyph_width, glyph_height) = estimate_glyph_size(key.codepoint, font_size);
            let glyph_width = glyph_width.max(1).min(256);
            let glyph_height = glyph_height.max(1).min(256);
            
            let mut bitmap = vec![0u8; (glyph_width * glyph_height) as usize];
            if key.codepoint.is_ascii_graphic() || key.codepoint.is_alphabetic() {
                for y in 0..glyph_height {
                    for x in 0..glyph_width {
                        let idx = (y * glyph_width + x) as usize;
                        let border = x == 0 || x == glyph_width - 1 || y == 0 || y == glyph_height - 1;
                        bitmap[idx] = if border { 255 } else { 200 };
                    }
                }
            }
            Some((bitmap, glyph_width, glyph_height, glyph_width as f32, 0.0f32, font_size * 0.8))
        };
        
        #[cfg(not(any(target_os = "macos", windows)))]
        let raster_result: Option<(Vec<u8>, u32, u32, f32, f32, f32)> = {
            // Fallback for other platforms
            let (glyph_width, glyph_height) = estimate_glyph_size(key.codepoint, font_size);
            let glyph_width = glyph_width.max(1).min(256);
            let glyph_height = glyph_height.max(1).min(256);
            
            let mut bitmap = vec![0u8; (glyph_width * glyph_height) as usize];
            if key.codepoint.is_ascii_graphic() || key.codepoint.is_alphabetic() {
                for y in 0..glyph_height {
                    for x in 0..glyph_width {
                        let idx = (y * glyph_width + x) as usize;
                        let border = x == 0 || x == glyph_width - 1 || y == 0 || y == glyph_height - 1;
                        bitmap[idx] = if border { 255 } else { 200 };
                    }
                }
            }
            Some((bitmap, glyph_width, glyph_height, glyph_width as f32, 0.0f32, font_size * 0.8))
        };
        
        let (bitmap, glyph_width, glyph_height, advance, _bearing_x, bearing_y) = raster_result?;
        
        let glyph_width = glyph_width.max(1).min(256);
        let glyph_height = glyph_height.max(1).min(256);

        // Allocate space in the atlas
        let (atlas_x, atlas_y) = self.allocate_space(glyph_width + 2, glyph_height + 2)?;

        // Upload to atlas
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.atlas,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: atlas_x + 1,
                    y: atlas_y + 1,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &bitmap,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(glyph_width),
                rows_per_image: Some(glyph_height),
            },
            wgpu::Extent3d {
                width: glyph_width,
                height: glyph_height,
                depth_or_array_layers: 1,
            },
        );

        let u0 = (atlas_x + 1) as f32 / self.atlas_size as f32;
        let v0 = (atlas_y + 1) as f32 / self.atlas_size as f32;
        let u1 = (atlas_x + 1 + glyph_width) as f32 / self.atlas_size as f32;
        let v1 = (atlas_y + 1 + glyph_height) as f32 / self.atlas_size as f32;

        let entry = GlyphEntry {
            tex_coords: [u0, v0, u1, v1],
            offset: [0.0, -bearing_y], // Baseline offset
            advance,
        };

        self.entries.insert(key.clone(), entry.clone());
        Some(entry)
    }

    /// Allocate space in the atlas.
    fn allocate_space(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        // Check if we need a new row
        if self.next_x + width > self.atlas_size {
            self.next_x = 1;
            self.next_y += self.row_height + 1;
            self.row_height = 0;
        }

        // Check if we've run out of space
        if self.next_y + height > self.atlas_size {
            tracing::warn!("Glyph atlas full, clearing cache");
            self.entries.clear();
            self.next_x = 1;
            self.next_y = 1;
            self.row_height = 0;
        }

        let x = self.next_x;
        let y = self.next_y;

        self.next_x += width + 1;
        self.row_height = self.row_height.max(height);

        Some((x, y))
    }

    /// Clear the cache.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.next_x = 1;
        self.next_y = 1;
        self.row_height = 0;
    }
}

/// Estimate glyph size based on character and font size.
fn estimate_glyph_size(ch: char, font_size: f32) -> (u32, u32) {
    let height = font_size.ceil() as u32;
    
    // Estimate width based on character type
    let width_factor = match ch {
        ' ' => 0.3,
        'i' | 'l' | '!' | '|' | '\'' => 0.3,
        'm' | 'w' | 'M' | 'W' => 0.9,
        _ if ch.is_ascii() => 0.6,
        _ => 0.8, // CJK and other wide characters
    };
    
    let width = (font_size * width_factor).ceil() as u32;
    (width.max(1), height.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glyph_key_hash() {
        let key1 = GlyphKey {
            codepoint: 'A',
            font_family: "Arial".to_string(),
            font_size: 160,
            font_weight: 400,
            font_style: 0,
        };

        let key2 = GlyphKey {
            codepoint: 'A',
            font_family: "Arial".to_string(),
            font_size: 160,
            font_weight: 400,
            font_style: 0,
        };

        assert_eq!(key1, key2);
    }

    #[test]
    fn test_glyph_key_different() {
        let key1 = GlyphKey {
            codepoint: 'A',
            font_family: "Arial".to_string(),
            font_size: 160,
            font_weight: 400,
            font_style: 0,
        };

        let key2 = GlyphKey {
            codepoint: 'B',
            font_family: "Arial".to_string(),
            font_size: 160,
            font_weight: 400,
            font_style: 0,
        };

        assert_ne!(key1, key2);
    }

    #[test]
    fn test_estimate_glyph_size() {
        let (w, h) = estimate_glyph_size('A', 16.0);
        assert!(w > 0);
        assert!(h > 0);
        
        let (narrow_w, _) = estimate_glyph_size('i', 16.0);
        let (wide_w, _) = estimate_glyph_size('M', 16.0);
        assert!(narrow_w < wide_w);
    }
}
