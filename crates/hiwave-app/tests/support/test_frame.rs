//! Test frame capture and pixel verification utilities.

use std::path::Path;

/// RGB color value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RGB {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RGB {
    /// Create a new RGB color.
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Check if this color is near another color within tolerance.
    pub fn near(self, other: RGB, tolerance: u8) -> bool {
        let dr = (self.r as i32 - other.r as i32).abs();
        let dg = (self.g as i32 - other.g as i32).abs();
        let db = (self.b as i32 - other.b as i32).abs();

        dr <= tolerance as i32 && dg <= tolerance as i32 && db <= tolerance as i32
    }
}

/// Captured frame for testing.
pub struct TestFrame {
    pub width: u32,
    pub height: u32,
    pixels: Vec<RGB>,
}

impl TestFrame {
    /// Load frame from PPM file.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let content = std::fs::read(path)
            .map_err(|e| format!("Failed to read frame at {}: {}", path.display(), e))?;

        // Parse PPM format: P6\nWIDTH HEIGHT\n255\n<binary RGB data>
        let mut lines = content.split(|&b| b == b'\n');

        // First line: P6
        let magic = lines.next().ok_or("Empty PPM file")?;
        if magic != b"P6" {
            return Err(format!("Invalid PPM magic: expected P6, got {:?}", magic));
        }

        // Skip comments and find dimensions
        let mut dims_line = None;
        for line in lines.by_ref() {
            if line.is_empty() {
                continue;
            }
            if line.starts_with(b"#") {
                continue;
            }
            dims_line = Some(line);
            break;
        }

        let dims = dims_line.ok_or("No dimensions found in PPM")?;
        let dims_str = std::str::from_utf8(dims)
            .map_err(|_| "Invalid UTF-8 in PPM dimensions")?;

        let parts: Vec<&str> = dims_str.split_whitespace().collect();
        if parts.len() < 2 {
            return Err(format!("Invalid dimensions: {:?}", dims_str));
        }

        let width: u32 = parts[0].parse()
            .map_err(|_| format!("Invalid width: {}", parts[0]))?;
        let height: u32 = parts[1].parse()
            .map_err(|_| format!("Invalid height: {}", parts[1]))?;

        // Max value line
        let max_val_line = lines.next().ok_or("No max value in PPM")?;
        let _max_val = std::str::from_utf8(max_val_line)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
            .ok_or("Invalid max value")?;

        // Calculate where pixel data starts
        let header_size = magic.len() + 1 // P6\n
            + dims.len() + 1  // width height\n
            + max_val_line.len() + 1; // 255\n

        // Read pixel data
        let pixel_data = &content[header_size..];
        let mut pixels = Vec::new();

        for chunk in pixel_data.chunks(3) {
            if chunk.len() == 3 {
                pixels.push(RGB {
                    r: chunk[0],
                    g: chunk[1],
                    b: chunk[2],
                });
            }
        }

        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// Sample a pixel at (x, y).
    pub fn sample_pixel(&self, x: u32, y: u32) -> RGB {
        let idx = (y * self.width + x) as usize;
        if idx < self.pixels.len() {
            self.pixels[idx]
        } else {
            RGB::new(0, 0, 0)
        }
    }

    /// Check if frame is blank (all pixels same color).
    pub fn is_blank(&self) -> bool {
        if self.pixels.is_empty() {
            return true;
        }

        let first = self.pixels[0];
        self.pixels.iter().all(|p| *p == first)
    }

    /// Compare this frame with another.
    pub fn compare(&self, other: &TestFrame, tolerance: u8) -> FrameDiff {
        if self.width != other.width || self.height != other.height {
            return FrameDiff {
                size_mismatch: true,
                diff_pixels: 0,
                total_pixels: 0,
                diff_percent: 0.0,
            };
        }

        let total = self.pixels.len();
        let mut diff_count = 0;

        for (a, b) in self.pixels.iter().zip(other.pixels.iter()) {
            if !a.near(*b, tolerance) {
                diff_count += 1;
            }
        }

        FrameDiff {
            size_mismatch: false,
            diff_pixels: diff_count,
            total_pixels: total,
            diff_percent: (diff_count as f64 / total as f64) * 100.0,
        }
    }
}

/// Frame comparison result.
#[derive(Debug)]
pub struct FrameDiff {
    pub size_mismatch: bool,
    pub diff_pixels: usize,
    pub total_pixels: usize,
    pub diff_percent: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgb_near() {
        let color1 = RGB::new(100, 150, 200);
        let color2 = RGB::new(102, 148, 201);

        assert!(color1.near(color2, 5));
        assert!(!color1.near(color2, 1));
    }

    #[test]
    fn test_frame_is_blank() {
        let frame = TestFrame {
            width: 10,
            height: 10,
            pixels: vec![RGB::new(255, 0, 0); 100],
        };

        assert!(frame.is_blank());
    }

    #[test]
    fn test_frame_not_blank() {
        let mut pixels = vec![RGB::new(255, 0, 0); 100];
        pixels[50] = RGB::new(0, 255, 0); // Different color

        let frame = TestFrame {
            width: 10,
            height: 10,
            pixels,
        };

        assert!(!frame.is_blank());
    }
}
