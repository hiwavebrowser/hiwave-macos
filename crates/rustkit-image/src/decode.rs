//! Image decoding utilities
//!
//! Handles detection and decoding of various image formats.

use rustkit_codecs::ImageFormat;

/// Detect image format from bytes
pub fn detect_format(bytes: &[u8]) -> Option<ImageFormat> {
    rustkit_codecs::detect_format(bytes)
}

/// Get MIME type for an image format
pub fn format_to_mime(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "image/png",
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::Gif => "image/gif",
        ImageFormat::WebP => "image/webp",
        ImageFormat::Bmp => "image/bmp",
        ImageFormat::Ico => "image/x-icon",
        _ => "application/octet-stream",
    }
}

/// Get file extension for an image format
pub fn format_to_extension(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Gif => "gif",
        ImageFormat::WebP => "webp",
        ImageFormat::Bmp => "bmp",
        ImageFormat::Ico => "ico",
        _ => "bin",
    }
}

/// Parse MIME type to image format
pub fn mime_to_format(mime: &str) -> Option<ImageFormat> {
    match mime.to_lowercase().as_str() {
        "image/png" => Some(ImageFormat::Png),
        "image/jpeg" | "image/jpg" => Some(ImageFormat::Jpeg),
        "image/gif" => Some(ImageFormat::Gif),
        "image/webp" => Some(ImageFormat::WebP),
        "image/bmp" => Some(ImageFormat::Bmp),
        "image/x-icon" | "image/vnd.microsoft.icon" => Some(ImageFormat::Ico),
        _ => None,
    }
}

/// Check if a format supports animation
pub fn supports_animation(format: ImageFormat) -> bool {
    matches!(format, ImageFormat::Gif | ImageFormat::WebP)
}

/// Check if a format supports transparency
pub fn supports_transparency(format: ImageFormat) -> bool {
    matches!(
        format,
        ImageFormat::Png | ImageFormat::Gif | ImageFormat::WebP | ImageFormat::Ico
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mime_to_format() {
        assert_eq!(mime_to_format("image/png"), Some(ImageFormat::Png));
        assert_eq!(mime_to_format("image/jpeg"), Some(ImageFormat::Jpeg));
        assert_eq!(mime_to_format("image/gif"), Some(ImageFormat::Gif));
        assert_eq!(mime_to_format("text/html"), None);
    }

    #[test]
    fn test_format_to_mime() {
        assert_eq!(format_to_mime(ImageFormat::Png), "image/png");
        assert_eq!(format_to_mime(ImageFormat::Jpeg), "image/jpeg");
    }

    #[test]
    fn test_supports_animation() {
        assert!(supports_animation(ImageFormat::Gif));
        assert!(!supports_animation(ImageFormat::Png));
        assert!(!supports_animation(ImageFormat::Jpeg));
    }

    #[test]
    fn test_supports_transparency() {
        assert!(supports_transparency(ImageFormat::Png));
        assert!(supports_transparency(ImageFormat::Gif));
        assert!(!supports_transparency(ImageFormat::Jpeg));
    }
}

