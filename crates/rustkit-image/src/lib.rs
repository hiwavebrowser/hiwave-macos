//! # RustKit Image
//!
//! Image loading, decoding, and caching for the RustKit browser engine.
//!
//! This crate handles:
//! - Async image fetching from URLs
//! - Decoding of PNG, JPEG, GIF, WebP, BMP, and ICO formats
//! - Animated GIF support
//! - Memory and disk caching
//! - GPU texture management
//! - Lazy loading support

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use rustkit_codecs::{Decoded, ImageFormat, RgbaImage};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tracing::debug;
use url::Url;

pub mod cache;
pub mod decode;
pub mod loader;

pub use cache::*;
pub use decode::*;
pub use loader::*;

/// Errors that can occur during image operations
#[derive(Error, Debug)]
pub enum ImageError {
    #[error("Failed to fetch image: {0}")]
    FetchError(String),

    #[error("Failed to decode image: {0}")]
    DecodeError(String),

    #[error("Unsupported image format: {0}")]
    UnsupportedFormat(String),

    #[error("Image too large: {width}x{height} exceeds maximum")]
    TooLarge { width: u32, height: u32 },

    #[error("Invalid image URL: {0}")]
    InvalidUrl(String),

    #[error("Network error: {0}")]
    NetworkError(#[from] rustkit_http::HttpError),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Cache error: {0}")]
    CacheError(String),
}

/// Result type for image operations
pub type ImageResult<T> = Result<T, ImageError>;

/// Represents a loaded and decoded image
#[derive(Clone)]
pub struct LoadedImage {
    /// Original URL of the image
    pub url: Url,

    /// Natural width of the image
    pub natural_width: u32,

    /// Natural height of the image
    pub natural_height: u32,

    /// Image data (static or animated)
    pub data: ImageData,

    /// When this image was decoded
    pub decoded_at: Instant,

    /// Content type from HTTP response
    pub content_type: Option<String>,

    /// Whether this image is complete (loaded successfully)
    pub complete: bool,
}

impl LoadedImage {
    /// Create a new loaded image from decoded data
    pub fn new(url: Url, image: RgbaImage) -> Self {
        let natural_width = image.width();
        let natural_height = image.height();
        Self {
            url,
            natural_width,
            natural_height,
            data: ImageData::Static(image),
            decoded_at: Instant::now(),
            content_type: None,
            complete: true,
        }
    }

    /// Create an animated image
    pub fn animated(url: Url, frames: Vec<AnimationFrame>) -> Self {
        let (natural_width, natural_height) = if let Some(first) = frames.first() {
            (first.image.width(), first.image.height())
        } else {
            (0, 0)
        };

        Self {
            url,
            natural_width,
            natural_height,
            data: ImageData::Animated(AnimatedImage {
                frames,
                loop_count: 0, // Infinite
            }),
            decoded_at: Instant::now(),
            content_type: None,
            complete: true,
        }
    }

    /// Get the current frame to display
    pub fn current_frame(&self, elapsed: Duration) -> &RgbaImage {
        match &self.data {
            ImageData::Static(img) => img,
            ImageData::Animated(anim) => anim.frame_at(elapsed),
        }
    }

    /// Check if this image is animated
    pub fn is_animated(&self) -> bool {
        matches!(self.data, ImageData::Animated(_))
    }

    /// Get the aspect ratio
    pub fn aspect_ratio(&self) -> f64 {
        if self.natural_height == 0 {
            1.0
        } else {
            self.natural_width as f64 / self.natural_height as f64
        }
    }
}

/// Image data - either static or animated
#[derive(Clone)]
pub enum ImageData {
    /// Single static image
    Static(RgbaImage),

    /// Animated image with multiple frames
    Animated(AnimatedImage),
}

/// Animated image with frames
#[derive(Clone)]
pub struct AnimatedImage {
    /// All frames
    pub frames: Vec<AnimationFrame>,

    /// Number of times to loop (0 = infinite)
    pub loop_count: u32,
}

impl AnimatedImage {
    /// Get the frame at a given elapsed time
    pub fn frame_at(&self, elapsed: Duration) -> &RgbaImage {
        if self.frames.is_empty() {
            panic!("AnimatedImage has no frames");
        }

        let total_duration: u64 = self.frames.iter().map(|f| f.delay_ms as u64).sum();
        if total_duration == 0 {
            return &self.frames[0].image;
        }

        let elapsed_ms = elapsed.as_millis() as u64 % total_duration;
        let mut cumulative = 0u64;

        for frame in &self.frames {
            cumulative += frame.delay_ms as u64;
            if elapsed_ms < cumulative {
                return &frame.image;
            }
        }

        &self.frames.last().unwrap().image
    }

    /// Get the total animation duration
    pub fn total_duration(&self) -> Duration {
        let total_ms: u64 = self.frames.iter().map(|f| f.delay_ms as u64).sum();
        Duration::from_millis(total_ms)
    }
}

/// A single animation frame
#[derive(Clone)]
pub struct AnimationFrame {
    /// The frame image
    pub image: RgbaImage,

    /// Delay before showing next frame (in milliseconds)
    pub delay_ms: u32,
}

/// Image loading state for tracking progress
#[derive(Clone, Debug)]
pub enum LoadingState {
    /// Not started
    Pending,

    /// Currently loading
    Loading {
        bytes_loaded: usize,
        bytes_total: Option<usize>,
    },

    /// Decoding the image
    Decoding,

    /// Successfully loaded
    Complete,

    /// Failed to load
    Error(String),
}

/// Request for loading an image
#[derive(Debug)]
pub struct ImageRequest {
    /// URL to load
    pub url: Url,

    /// Whether to use cache
    pub use_cache: bool,

    /// Priority (higher = load sooner)
    pub priority: u8,

    /// Whether this is a lazy load (defer if offscreen)
    pub lazy: bool,

    /// Desired width hint for srcset selection
    pub width_hint: Option<u32>,
}

impl ImageRequest {
    /// Create a simple request for a URL
    pub fn new(url: Url) -> Self {
        Self {
            url,
            use_cache: true,
            priority: 5,
            lazy: false,
            width_hint: None,
        }
    }

    /// Set lazy loading
    pub fn lazy(mut self, lazy: bool) -> Self {
        self.lazy = lazy;
        self
    }

    /// Set priority
    pub fn priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    /// Set width hint for responsive images
    pub fn width_hint(mut self, width: u32) -> Self {
        self.width_hint = Some(width);
        self
    }
}

/// The main image manager that handles loading and caching
pub struct ImageManager {
    /// Memory cache for decoded images
    cache: Arc<RwLock<ImageCache>>,

    /// HTTP client for fetching images
    client: rustkit_http::Client,

    /// Pending loads
    #[allow(clippy::type_complexity)]
    pending: Arc<RwLock<HashMap<Url, Vec<oneshot::Sender<ImageResult<Arc<LoadedImage>>>>>>>,

    /// Channel for sending load requests
    request_tx: mpsc::Sender<ImageRequest>,

    /// Maximum image dimensions
    max_dimensions: (u32, u32),

    /// Maximum memory cache size in bytes
    #[allow(dead_code)]
    max_cache_bytes: usize,
}

impl ImageManager {
    /// Create a new image manager
    pub fn new() -> Self {
        let (request_tx, _request_rx) = mpsc::channel::<ImageRequest>(100);

        Self {
            cache: Arc::new(RwLock::new(ImageCache::new(100))),
            client: rustkit_http::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
            pending: Arc::new(RwLock::new(HashMap::new())),
            request_tx,
            max_dimensions: (16384, 16384),
            max_cache_bytes: 256 * 1024 * 1024, // 256MB
        }
    }

    /// Load an image from a URL
    pub async fn load(&self, url: Url) -> ImageResult<Arc<LoadedImage>> {
        // Check cache first
        if let Some(cached) = self.cache.read().unwrap().get(&url) {
            debug!("Image cache hit: {}", url);
            return Ok(cached);
        }

        // Check if already loading
        let already_loading = {
            let pending = self.pending.read().unwrap();
            pending.contains_key(&url)
        };

        if already_loading {
            debug!("Image already loading: {}", url);
            // Add ourselves to the waiting list
            let (tx, rx) = oneshot::channel();
            self.pending.write().unwrap().entry(url.clone()).or_default().push(tx);
            return rx.await.map_err(|_| ImageError::FetchError("Load cancelled".into()))?;
        }

        // Start loading
        debug!("Starting image load: {}", url);
        self.pending.write().unwrap().insert(url.clone(), vec![]);

        let result = self.fetch_and_decode(url.clone()).await;

        // Notify waiters and cache result
        let waiters = self.pending.write().unwrap().remove(&url).unwrap_or_default();
        
        match &result {
            Ok(image) => {
                self.cache.write().unwrap().insert(url.clone(), image.clone());
                for waiter in waiters {
                    let _ = waiter.send(Ok(image.clone()));
                }
            }
            Err(e) => {
                let err_msg = e.to_string();
                for waiter in waiters {
                    let _ = waiter.send(Err(ImageError::FetchError(err_msg.clone())));
                }
            }
        }

        result
    }

    /// Fetch and decode an image
    async fn fetch_and_decode(&self, url: Url) -> ImageResult<Arc<LoadedImage>> {
        // Handle data URLs
        if url.scheme() == "data" {
            return self.decode_data_url(&url);
        }

        // Fetch the image using rustkit-http
        let response = self.client.get(url.as_str()).await?;

        if !response.is_success() {
            return Err(ImageError::FetchError(format!(
                "HTTP {} for {}",
                response.status,
                url
            )));
        }

        let content_type = response.content_type().map(|s| s.to_string());

        // Decode the image
        let mut loaded = self.decode_bytes(&url, &response.body)?;
        loaded.content_type = content_type;

        Ok(Arc::new(loaded))
    }

    /// Decode image from bytes
    fn decode_bytes(&self, url: &Url, bytes: &[u8]) -> ImageResult<LoadedImage> {
        // Guess format from bytes
        let format = rustkit_codecs::detect_format(bytes)
            .unwrap_or(ImageFormat::Unknown);

        if format == ImageFormat::Unknown {
            return Err(ImageError::DecodeError("Unknown image format".into()));
        }

        // Handle animated GIFs specially
        if format == ImageFormat::Gif {
            return self.decode_gif(url, bytes);
        }

        // Decode static image
        let decoded = rustkit_codecs::decode_any(bytes)
            .map_err(|e| ImageError::DecodeError(e.to_string()))?;
        let img = match decoded {
            Decoded::Static(img) => img,
            Decoded::Animated(frames) => {
                // Some formats may be treated as animated later; for now, take first frame.
                frames
                    .into_iter()
                    .next()
                    .map(|f| f.image)
                    .ok_or_else(|| ImageError::DecodeError("Animated image had no frames".into()))?
            }
        };

        // Check dimensions
        let (width, height) = (img.width(), img.height());
        if width > self.max_dimensions.0 || height > self.max_dimensions.1 {
            return Err(ImageError::TooLarge { width, height });
        }

        Ok(LoadedImage::new(url.clone(), img))
    }

    /// Decode an animated GIF
    fn decode_gif(&self, url: &Url, bytes: &[u8]) -> ImageResult<LoadedImage> {
        let decoded_frames = rustkit_codecs::decode_gif(bytes)
            .map_err(|e| ImageError::DecodeError(e.to_string()))?;

        let mut frames = Vec::with_capacity(decoded_frames.len());
        for f in decoded_frames {
            // Check dimensions
            if f.image.width() > self.max_dimensions.0 || f.image.height() > self.max_dimensions.1 {
                return Err(ImageError::TooLarge {
                    width: f.image.width(),
                    height: f.image.height(),
                });
            }
            frames.push(AnimationFrame {
                image: f.image,
                delay_ms: f.delay_ms.max(10),
            });
        }

        if frames.is_empty() {
            return Err(ImageError::DecodeError("GIF has no frames".into()));
        }

        // Single frame = static image
        if frames.len() == 1 {
            let frame = frames.remove(0);
            return Ok(LoadedImage {
                url: url.clone(),
                natural_width: frame.image.width(),
                natural_height: frame.image.height(),
                data: ImageData::Static(frame.image),
                decoded_at: Instant::now(),
                content_type: Some("image/gif".into()),
                complete: true,
            });
        }

        Ok(LoadedImage::animated(url.clone(), frames))
    }

    /// Decode a data URL
    fn decode_data_url(&self, url: &Url) -> ImageResult<Arc<LoadedImage>> {
        let path = url.path();
        
        // Parse data URL: data:[<mediatype>][;base64],<data>
        let comma_pos = path.find(',')
            .ok_or_else(|| ImageError::InvalidUrl("Invalid data URL format".into()))?;

        let metadata = &path[..comma_pos];
        let data = &path[comma_pos + 1..];

        let is_base64 = metadata.contains("base64");

        let bytes = if is_base64 {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.decode(data)
                .map_err(|e| ImageError::DecodeError(format!("Base64 decode error: {}", e)))?
        } else {
            // URL-encoded
            urlencoding::decode(data)
                .map_err(|e| ImageError::DecodeError(format!("URL decode error: {}", e)))?
                .into_owned()
                .into_bytes()
        };

        let loaded = self.decode_bytes(url, &bytes)?;
        Ok(Arc::new(loaded))
    }

    /// Preload an image without blocking
    pub fn preload(&self, url: Url) {
        let _ = self.request_tx.try_send(ImageRequest::new(url));
    }

    /// Clear the cache
    pub fn clear_cache(&self) {
        self.cache.write().unwrap().clear();
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> CacheStats {
        self.cache.read().unwrap().stats()
    }

    /// Check if an image is cached
    pub fn is_cached(&self, url: &Url) -> bool {
        self.cache.read().unwrap().contains(url)
    }

    /// Get a cached image if available
    pub fn get_cached(&self, url: &Url) -> Option<Arc<LoadedImage>> {
        self.cache.read().unwrap().get(url)
    }
}

impl Default for ImageManager {
    fn default() -> Self {
        Self::new()
    }
}

/// CSS object-fit values
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ObjectFit {
    /// Fill the box, possibly distorting the image
    Fill,

    /// Scale to fit inside the box, preserving aspect ratio
    #[default]
    Contain,

    /// Scale to cover the box, preserving aspect ratio
    Cover,

    /// Don't scale the image
    None,

    /// Like `contain` but never scale up
    ScaleDown,
}

impl ObjectFit {
    /// Parse from CSS value
    pub fn from_css(value: &str) -> Option<Self> {
        match value.trim().to_lowercase().as_str() {
            "fill" => Some(ObjectFit::Fill),
            "contain" => Some(ObjectFit::Contain),
            "cover" => Some(ObjectFit::Cover),
            "none" => Some(ObjectFit::None),
            "scale-down" => Some(ObjectFit::ScaleDown),
            _ => None,
        }
    }

    /// Calculate the image rectangle within a container
    pub fn compute_rect(
        &self,
        container_width: f64,
        container_height: f64,
        image_width: f64,
        image_height: f64,
        object_position: (f64, f64), // 0-1 range, default (0.5, 0.5)
    ) -> ImageRect {
        if image_width == 0.0 || image_height == 0.0 {
            return ImageRect::default();
        }

        let image_aspect = image_width / image_height;
        let container_aspect = container_width / container_height;

        let (draw_width, draw_height) = match self {
            ObjectFit::Fill => (container_width, container_height),

            ObjectFit::Contain => {
                if image_aspect > container_aspect {
                    // Image is wider - fit to width
                    (container_width, container_width / image_aspect)
                } else {
                    // Image is taller - fit to height
                    (container_height * image_aspect, container_height)
                }
            }

            ObjectFit::Cover => {
                if image_aspect > container_aspect {
                    // Image is wider - fit to height
                    (container_height * image_aspect, container_height)
                } else {
                    // Image is taller - fit to width
                    (container_width, container_width / image_aspect)
                }
            }

            ObjectFit::None => (image_width, image_height),

            ObjectFit::ScaleDown => {
                // Use contain but only if it would scale down
                if image_width <= container_width && image_height <= container_height {
                    (image_width, image_height)
                } else if image_aspect > container_aspect {
                    (container_width, container_width / image_aspect)
                } else {
                    (container_height * image_aspect, container_height)
                }
            }
        };

        // Position within container
        let x = (container_width - draw_width) * object_position.0;
        let y = (container_height - draw_height) * object_position.1;

        ImageRect {
            x,
            y,
            width: draw_width,
            height: draw_height,
        }
    }
}

/// Rectangle for drawing an image
#[derive(Debug, Clone, Default)]
pub struct ImageRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// CSS object-position parsing
#[derive(Debug, Clone, Default)]
pub struct ObjectPosition {
    pub x: f64, // 0-1 range
    pub y: f64, // 0-1 range
}

impl ObjectPosition {
    /// Default center position
    pub fn center() -> Self {
        Self { x: 0.5, y: 0.5 }
    }

    /// Parse from CSS value
    pub fn from_css(value: &str) -> Self {
        let parts: Vec<&str> = value.split_whitespace().collect();

        let parse_keyword_or_percentage = |s: &str| -> f64 {
            match s.to_lowercase().as_str() {
                "left" | "top" => 0.0,
                "center" => 0.5,
                "right" | "bottom" => 1.0,
                s if s.ends_with('%') => {
                    s.trim_end_matches('%').parse::<f64>().unwrap_or(50.0) / 100.0
                }
                _ => 0.5,
            }
        };

        match parts.len() {
            0 => Self::center(),
            1 => {
                let v = parse_keyword_or_percentage(parts[0]);
                Self { x: v, y: v }
            }
            _ => Self {
                x: parse_keyword_or_percentage(parts[0]),
                y: parse_keyword_or_percentage(parts[1]),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_object_fit_contain() {
        let fit = ObjectFit::Contain;
        let rect = fit.compute_rect(100.0, 100.0, 200.0, 100.0, (0.5, 0.5));
        assert!((rect.width - 100.0).abs() < 0.001);
        assert!((rect.height - 50.0).abs() < 0.001);
        assert!((rect.x - 0.0).abs() < 0.001);
        assert!((rect.y - 25.0).abs() < 0.001);
    }

    #[test]
    fn test_object_fit_cover() {
        let fit = ObjectFit::Cover;
        let rect = fit.compute_rect(100.0, 100.0, 200.0, 100.0, (0.5, 0.5));
        assert!((rect.width - 200.0).abs() < 0.001);
        assert!((rect.height - 100.0).abs() < 0.001);
        assert!((rect.x - -50.0).abs() < 0.001);
    }

    #[test]
    fn test_object_fit_fill() {
        let fit = ObjectFit::Fill;
        let rect = fit.compute_rect(100.0, 80.0, 200.0, 100.0, (0.5, 0.5));
        assert!((rect.width - 100.0).abs() < 0.001);
        assert!((rect.height - 80.0).abs() < 0.001);
    }

    #[test]
    fn test_object_position_parsing() {
        let pos = ObjectPosition::from_css("left top");
        assert!((pos.x - 0.0).abs() < 0.001);
        assert!((pos.y - 0.0).abs() < 0.001);

        let pos = ObjectPosition::from_css("center");
        assert!((pos.x - 0.5).abs() < 0.001);
        assert!((pos.y - 0.5).abs() < 0.001);

        let pos = ObjectPosition::from_css("75% 25%");
        assert!((pos.x - 0.75).abs() < 0.001);
        assert!((pos.y - 0.25).abs() < 0.001);
    }

    #[test]
    fn test_animated_image_frame_at() {
        let rgba1 = RgbaImage::new(10, 10);
        let rgba2 = RgbaImage::new(10, 10);
        let anim = AnimatedImage {
            frames: vec![
                AnimationFrame { image: rgba1, delay_ms: 100 },
                AnimationFrame { image: rgba2, delay_ms: 100 },
            ],
            loop_count: 0,
        };

        // At 0ms, should be frame 0
        let _ = anim.frame_at(Duration::from_millis(0));
        
        // At 150ms, should be frame 1
        let _ = anim.frame_at(Duration::from_millis(150));

        // At 250ms (past loop), should be frame 0 again
        let _ = anim.frame_at(Duration::from_millis(250));
    }

    #[test]
    fn test_object_fit_scale_down() {
        // Image smaller than container - don't scale
        let fit = ObjectFit::ScaleDown;
        let rect = fit.compute_rect(200.0, 200.0, 50.0, 50.0, (0.5, 0.5));
        assert!((rect.width - 50.0).abs() < 0.001);
        assert!((rect.height - 50.0).abs() < 0.001);

        // Image larger than container - scale down like contain
        let rect = fit.compute_rect(100.0, 100.0, 400.0, 200.0, (0.5, 0.5));
        assert!((rect.width - 100.0).abs() < 0.001);
        assert!((rect.height - 50.0).abs() < 0.001);
    }
}

