//! Image loading utilities
//!
//! Handles async image loading with progress tracking.

use std::sync::Arc;

use tokio::sync::watch;
use url::Url;

use crate::{LoadedImage, LoadingState};

/// A handle to a loading image
pub struct ImageLoadHandle {
    /// URL being loaded
    pub url: Url,

    /// Loading state receiver
    state_rx: watch::Receiver<LoadingState>,

    /// Result handle (set when complete)
    result: Option<Arc<LoadedImage>>,
}

impl ImageLoadHandle {
    /// Create a new load handle
    pub fn new(url: Url, state_rx: watch::Receiver<LoadingState>) -> Self {
        Self {
            url,
            state_rx,
            result: None,
        }
    }

    /// Get the current loading state
    pub fn state(&self) -> LoadingState {
        self.state_rx.borrow().clone()
    }

    /// Check if loading is complete
    pub fn is_complete(&self) -> bool {
        matches!(self.state(), LoadingState::Complete | LoadingState::Error(_))
    }

    /// Get loading progress (0.0 to 1.0)
    pub fn progress(&self) -> f64 {
        match self.state() {
            LoadingState::Pending => 0.0,
            LoadingState::Loading { bytes_loaded, bytes_total } => {
                if let Some(total) = bytes_total {
                    if total > 0 {
                        return bytes_loaded as f64 / total as f64;
                    }
                }
                0.5 // Unknown progress
            }
            LoadingState::Decoding => 0.9,
            LoadingState::Complete => 1.0,
            LoadingState::Error(_) => 0.0,
        }
    }

    /// Get the result if complete
    pub fn result(&self) -> Option<Arc<LoadedImage>> {
        self.result.clone()
    }
}

/// Srcset parsing for responsive images
#[derive(Debug, Clone)]
pub struct SrcsetEntry {
    /// Image URL
    pub url: String,

    /// Width descriptor (e.g., 800w)
    pub width: Option<u32>,

    /// Pixel density descriptor (e.g., 2x)
    pub density: Option<f64>,
}

/// Parse a srcset attribute value
pub fn parse_srcset(srcset: &str) -> Vec<SrcsetEntry> {
    let mut entries = Vec::new();

    for part in srcset.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        let mut parts = part.split_whitespace();
        let url = match parts.next() {
            Some(u) => u.to_string(),
            None => continue,
        };

        let mut entry = SrcsetEntry {
            url,
            width: None,
            density: None,
        };

        if let Some(descriptor) = parts.next() {
            if descriptor.ends_with('w') {
                if let Ok(w) = descriptor.trim_end_matches('w').parse() {
                    entry.width = Some(w);
                }
            } else if descriptor.ends_with('x') {
                if let Ok(d) = descriptor.trim_end_matches('x').parse() {
                    entry.density = Some(d);
                }
            }
        }

        entries.push(entry);
    }

    entries
}

/// Select the best srcset entry for a given viewport width and device pixel ratio
pub fn select_srcset_entry(
    entries: &[SrcsetEntry],
    viewport_width: u32,
    device_pixel_ratio: f64,
) -> Option<&SrcsetEntry> {
    if entries.is_empty() {
        return None;
    }

    // Calculate target width
    let target_width = (viewport_width as f64 * device_pixel_ratio) as u32;

    // First, try width descriptors
    let width_entries: Vec<_> = entries.iter().filter(|e| e.width.is_some()).collect();
    if !width_entries.is_empty() {
        // Find the smallest image that's >= target width
        let mut best = width_entries[0];
        for entry in &width_entries {
            let w = entry.width.unwrap();
            let best_w = best.width.unwrap();

            // Update best if this entry is a better fit
            let should_update = if w >= target_width && (best_w < target_width || w < best_w) {
                true
            } else {
                w < target_width && w > best_w
            };

            if should_update {
                best = entry;
            }
        }
        return Some(best);
    }

    // Then, try density descriptors
    let density_entries: Vec<_> = entries.iter().filter(|e| e.density.is_some()).collect();
    if !density_entries.is_empty() {
        // Find the closest match to device pixel ratio
        let mut best = density_entries[0];
        for entry in &density_entries {
            let d = entry.density.unwrap();
            let best_d = best.density.unwrap();

            if (d - device_pixel_ratio).abs() < (best_d - device_pixel_ratio).abs() {
                best = entry;
            }
        }
        return Some(best);
    }

    // Fallback to first entry
    entries.first()
}

/// Parse sizes attribute for responsive images
#[derive(Debug, Clone)]
pub struct SizesEntry {
    /// Media query (e.g., "(max-width: 600px)")
    pub media_query: Option<String>,

    /// Size value (e.g., "100vw", "50vw", "500px")
    pub size: String,
}

/// Parse a sizes attribute value
pub fn parse_sizes(sizes: &str) -> Vec<SizesEntry> {
    let mut entries = Vec::new();

    for part in sizes.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        // Check if there's a media query
        if let Some(paren_start) = part.find('(') {
            if let Some(paren_end) = part.find(')') {
                let media_query = &part[paren_start..=paren_end];
                let size = part[paren_end + 1..].trim();

                entries.push(SizesEntry {
                    media_query: Some(media_query.to_string()),
                    size: size.to_string(),
                });
                continue;
            }
        }

        // No media query - this is the default size
        entries.push(SizesEntry {
            media_query: None,
            size: part.to_string(),
        });
    }

    entries
}

/// Calculate the effective size from sizes entries
pub fn calculate_effective_size(
    entries: &[SizesEntry],
    viewport_width: u32,
    _viewport_height: u32,
) -> u32 {
    for entry in entries {
        // Skip entries with media queries for now (simplified)
        // A full implementation would evaluate the media queries
        if entry.media_query.is_none() {
            return parse_size_value(&entry.size, viewport_width);
        }
    }

    // Default to viewport width
    viewport_width
}

fn parse_size_value(size: &str, viewport_width: u32) -> u32 {
    let size = size.trim();

    if size.ends_with("vw") {
        if let Ok(percent) = size.trim_end_matches("vw").parse::<f64>() {
            return (viewport_width as f64 * percent / 100.0) as u32;
        }
    } else if size.ends_with("px") {
        if let Ok(px) = size.trim_end_matches("px").parse::<u32>() {
            return px;
        }
    } else if size.ends_with("em") || size.ends_with("rem") {
        // Assume 16px base font size
        let size = size.trim_end_matches("em").trim_end_matches("r");
        if let Ok(em) = size.parse::<f64>() {
            return (em * 16.0) as u32;
        }
    }

    viewport_width
}

/// Create a placeholder image while loading
pub fn create_placeholder(width: u32, height: u32, color: [u8; 4]) -> rustkit_codecs::RgbaImage {
    let mut img = rustkit_codecs::RgbaImage::new(width, height);
    for pixel in img.pixels_mut() {
        pixel.copy_from_slice(&color);
    }
    img
}

/// Detect broken image (e.g., invalid data)
pub fn is_valid_image_data(bytes: &[u8]) -> bool {
    rustkit_codecs::detect_format(bytes).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_srcset() {
        let srcset = "image-480.jpg 480w, image-800.jpg 800w, image-1200.jpg 1200w";
        let entries = parse_srcset(srcset);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].url, "image-480.jpg");
        assert_eq!(entries[0].width, Some(480));
        assert_eq!(entries[1].width, Some(800));
        assert_eq!(entries[2].width, Some(1200));
    }

    #[test]
    fn test_parse_srcset_density() {
        let srcset = "image.jpg 1x, image@2x.jpg 2x, image@3x.jpg 3x";
        let entries = parse_srcset(srcset);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].density, Some(1.0));
        assert_eq!(entries[1].density, Some(2.0));
        assert_eq!(entries[2].density, Some(3.0));
    }

    #[test]
    fn test_select_srcset_entry() {
        let srcset = "small.jpg 400w, medium.jpg 800w, large.jpg 1200w";
        let entries = parse_srcset(srcset);

        // Viewport 500px, 1x DPR -> should pick 800w
        let selected = select_srcset_entry(&entries, 500, 1.0).unwrap();
        assert_eq!(selected.width, Some(800));

        // Viewport 500px, 2x DPR -> should pick 1200w
        let selected = select_srcset_entry(&entries, 500, 2.0).unwrap();
        assert_eq!(selected.width, Some(1200));
    }

    #[test]
    fn test_parse_sizes() {
        let sizes = "(max-width: 600px) 100vw, (max-width: 1200px) 50vw, 800px";
        let entries = parse_sizes(sizes);
        assert_eq!(entries.len(), 3);
        assert!(entries[0].media_query.is_some());
        assert_eq!(entries[2].size, "800px");
    }

    #[test]
    fn test_parse_size_value() {
        assert_eq!(parse_size_value("100vw", 1000), 1000);
        assert_eq!(parse_size_value("50vw", 1000), 500);
        assert_eq!(parse_size_value("500px", 1000), 500);
        assert_eq!(parse_size_value("2em", 1000), 32);
    }
}

