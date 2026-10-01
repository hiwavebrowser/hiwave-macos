//! OS-level window screenshot capture.
//!
//! Uses Win32 APIs to capture window contents as PNG files.

#[cfg(windows)]
use windows::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
        GetDIBits, GetWindowDC, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER,
        BI_RGB, DIB_RGB_COLORS, SRCCOPY,
    },
    UI::WindowsAndMessaging::GetClientRect,
};

use std::path::Path;

/// Error type for screenshot operations.
#[derive(Debug, thiserror::Error)]
pub enum ScreenshotError {
    #[error("Win32 API error: {0}")]
    Win32(String),
    
    #[error("PNG encoding failed: {0}")]
    PngEncoding(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Not implemented on this platform")]
    NotImplemented,
}

/// Capture a window's contents to a PNG file.
#[cfg(windows)]
pub fn capture_hwnd_to_png(
    hwnd: HWND,
    output_path: impl AsRef<Path>,
) -> Result<(u32, u32), ScreenshotError> {
    use std::fs::File;
    use std::io::BufWriter;
    
    unsafe {
        // Get window dimensions
        let mut rect = RECT::default();
        GetClientRect(hwnd, &mut rect)
            .map_err(|e| ScreenshotError::Win32(format!("GetClientRect failed: {e}")))?;
        
        let width = (rect.right - rect.left) as u32;
        let height = (rect.bottom - rect.top) as u32;
        
        if width == 0 || height == 0 {
            return Err(ScreenshotError::Win32("Window has zero size".into()));
        }
        
        // Get window DC
        let hdc_window = GetWindowDC(hwnd);
        if hdc_window.is_invalid() {
            return Err(ScreenshotError::Win32("GetWindowDC failed".into()));
        }
        
        // Create compatible DC and bitmap
        let hdc_mem = CreateCompatibleDC(hdc_window);
        if hdc_mem.is_invalid() {
            ReleaseDC(hwnd, hdc_window);
            return Err(ScreenshotError::Win32("CreateCompatibleDC failed".into()));
        }
        
        let hbm = CreateCompatibleBitmap(hdc_window, width as i32, height as i32);
        if hbm.is_invalid() {
            let _ = DeleteDC(hdc_mem);
            ReleaseDC(hwnd, hdc_window);
            return Err(ScreenshotError::Win32("CreateCompatibleBitmap failed".into()));
        }
        
        let old_bm = SelectObject(hdc_mem, hbm);
        
        // BitBlt from window to memory DC
        let result = BitBlt(
            hdc_mem,
            0,
            0,
            width as i32,
            height as i32,
            hdc_window,
            0,
            0,
            SRCCOPY,
        );
        
        if result.is_err() {
            SelectObject(hdc_mem, old_bm);
            let _ = DeleteObject(hbm);
            let _ = DeleteDC(hdc_mem);
            ReleaseDC(hwnd, hdc_window);
            return Err(ScreenshotError::Win32("BitBlt failed".into()));
        }
        
        // Get bitmap bits
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32), // Negative for top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [Default::default()],
        };
        
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        
        let lines = GetDIBits(
            hdc_mem,
            hbm,
            0,
            height,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );
        
        // Cleanup
        SelectObject(hdc_mem, old_bm);
        let _ = DeleteObject(hbm);
        let _ = DeleteDC(hdc_mem);
        ReleaseDC(hwnd, hdc_window);
        
        if lines == 0 {
            return Err(ScreenshotError::Win32("GetDIBits failed".into()));
        }
        
        // Convert BGRA to RGBA
        for chunk in pixels.chunks_exact_mut(4) {
            chunk.swap(0, 2); // Swap B and R
        }
        
        // Save PNG
        let file = File::create(&output_path)?;
        let writer = BufWriter::new(file);
        
        let mut encoder = png::Encoder::new(writer, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        
        let mut png_writer = encoder
            .write_header()
            .map_err(|e| ScreenshotError::PngEncoding(e.to_string()))?;
        
        png_writer
            .write_image_data(&pixels)
            .map_err(|e| ScreenshotError::PngEncoding(e.to_string()))?;
        
        Ok((width, height))
    }
}

/// Capture a window's contents (non-Windows stub).
#[cfg(not(windows))]
pub fn capture_hwnd_to_png(
    _hwnd: isize,
    _output_path: impl AsRef<Path>,
) -> Result<(u32, u32), ScreenshotError> {
    Err(ScreenshotError::NotImplemented)
}

/// Capture raw pixels from an HWND (for testing/comparison).
#[cfg(windows)]
pub fn capture_hwnd_pixels(hwnd: HWND) -> Result<(u32, u32, Vec<u8>), ScreenshotError> {
    unsafe {
        // Get window dimensions
        let mut rect = RECT::default();
        GetClientRect(hwnd, &mut rect)
            .map_err(|e| ScreenshotError::Win32(format!("GetClientRect failed: {e}")))?;
        
        let width = (rect.right - rect.left) as u32;
        let height = (rect.bottom - rect.top) as u32;
        
        if width == 0 || height == 0 {
            return Err(ScreenshotError::Win32("Window has zero size".into()));
        }
        
        // Get window DC
        let hdc_window = GetWindowDC(hwnd);
        if hdc_window.is_invalid() {
            return Err(ScreenshotError::Win32("GetWindowDC failed".into()));
        }
        
        // Create compatible DC and bitmap
        let hdc_mem = CreateCompatibleDC(hdc_window);
        if hdc_mem.is_invalid() {
            ReleaseDC(hwnd, hdc_window);
            return Err(ScreenshotError::Win32("CreateCompatibleDC failed".into()));
        }
        
        let hbm = CreateCompatibleBitmap(hdc_window, width as i32, height as i32);
        if hbm.is_invalid() {
            let _ = DeleteDC(hdc_mem);
            ReleaseDC(hwnd, hdc_window);
            return Err(ScreenshotError::Win32("CreateCompatibleBitmap failed".into()));
        }
        
        let old_bm = SelectObject(hdc_mem, hbm);
        
        // BitBlt from window to memory DC
        let result = BitBlt(
            hdc_mem,
            0,
            0,
            width as i32,
            height as i32,
            hdc_window,
            0,
            0,
            SRCCOPY,
        );
        
        if result.is_err() {
            SelectObject(hdc_mem, old_bm);
            let _ = DeleteObject(hbm);
            let _ = DeleteDC(hdc_mem);
            ReleaseDC(hwnd, hdc_window);
            return Err(ScreenshotError::Win32("BitBlt failed".into()));
        }
        
        // Get bitmap bits
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32), // Negative for top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [Default::default()],
        };
        
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        
        let lines = GetDIBits(
            hdc_mem,
            hbm,
            0,
            height,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );
        
        // Cleanup
        SelectObject(hdc_mem, old_bm);
        let _ = DeleteObject(hbm);
        let _ = DeleteDC(hdc_mem);
        ReleaseDC(hwnd, hdc_window);
        
        if lines == 0 {
            return Err(ScreenshotError::Win32("GetDIBits failed".into()));
        }
        
        // Convert BGRA to RGBA
        for chunk in pixels.chunks_exact_mut(4) {
            chunk.swap(0, 2); // Swap B and R
        }
        
        Ok((width, height, pixels))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    #[cfg(windows)]
    fn test_screenshot_error_display() {
        let err = ScreenshotError::Win32("test error".into());
        assert!(err.to_string().contains("test error"));
    }
}

