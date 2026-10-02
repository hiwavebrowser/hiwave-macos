"""Apply the rustkit-image half of the loader routing (exact-string replacements; fails loudly)."""
WT = '/Users/petecopeland/Repos/.worktrees/rs-bg-layers'


def sub(path, pairs):
    s = open(path).read()
    for old, new in pairs:
        assert s.count(old) == 1, (path, s.count(old), old[:70])
        s = s.replace(old, new)
    open(path, 'w').write(s)


sub(f'{WT}/crates/rustkit-image/Cargo.toml', [(
'''# HTTP client for loading (RustKit-owned)
rustkit-http = { path = "../rustkit-http" }

''', '')])

sub(f'{WT}/crates/rustkit-image/src/lib.rs', [
('''    #[error("Network error: {0}")]
    NetworkError(#[from] rustkit_http::HttpError),

''', ''),
('''    /// HTTP client for fetching images
    client: rustkit_http::Client,

''', ''),
('''            client: rustkit_http::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
''', ''),
('''    /// Load an image from a URL
    pub async fn load(''', '''    /// An image that needs no request: one already cached, or a `data:`
    /// URL. This crate has no HTTP client. A network image is fetched by
    /// the engine's resource loader (the shield, the Referer policy and the
    /// subresource budget all live there) and handed to
    /// [`ImageManager::insert_fetched`]; asking for one here is an error.
    pub async fn load('''),
('''    /// Fetch and decode an image
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

        // SVG from an extensionless URL (linkedin's hero is
        // `/aero-v1/sc/h/<hash>`) reaches the raster lane; route it back by
        // its type instead of failing it as "Unknown image format".
        if content_type.as_deref().is_some_and(is_svg_content_type) {
            return Err(ImageError::Svg(
                String::from_utf8_lossy(&response.body).into_owned(),
            ));
        }

        // Decode the image
        let mut loaded = self.decode_bytes(&url, &response.body)?;
        loaded.content_type = content_type;

        Ok(Arc::new(loaded))
    }
''', '''    /// Decode a `data:` URL; anything else was never fetched.
    async fn fetch_and_decode(&self, url: Url) -> ImageResult<Arc<LoadedImage>> {
        if url.scheme() == "data" {
            return self.decode_data_url(&url);
        }
        Err(Self::not_fetched(&url))
    }

    fn not_fetched(url: &Url) -> ImageError {
        ImageError::FetchError(format!(
            "{url} is not cached: network images are fetched by the engine's resource loader"
        ))
    }

    /// Decode a response body the caller fetched for `url` and cache it
    /// under that URL.
    pub fn insert_fetched(
        &self,
        url: &Url,
        content_type: Option<&str>,
        body: &[u8],
    ) -> ImageResult<Arc<LoadedImage>> {
        // SVG from an extensionless URL (linkedin's hero is
        // `/aero-v1/sc/h/<hash>`) reaches the raster lane; route it back by
        // its type instead of failing it as "Unknown image format".
        if content_type.is_some_and(is_svg_content_type) {
            return Err(ImageError::Svg(String::from_utf8_lossy(body).into_owned()));
        }

        let mut loaded = self.decode_bytes(url, body)?;
        loaded.content_type = content_type.map(str::to_string);
        let loaded = Arc::new(loaded);
        self.cache.write().unwrap().insert(url.clone(), loaded.clone());
        Ok(loaded)
    }
'''),
('''    /// Load an image synchronously (blocking).
    /// This is primarily for parity testing where we need images to be loaded
    /// before capturing a frame.
    pub fn load_blocking(''', '''    /// [`ImageManager::load`] without an executor: the cache, or a `data:`
    /// URL decoded in place.
    pub fn load_blocking('''),
('''        // For other URLs, we need to block on the async load
        // This uses a simple polling approach
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| ImageError::FetchError(format!("Runtime error: {}", e)))?;

        runtime.block_on(self.load(url))
''', '''        Err(Self::not_fetched(&url))
'''),
])
print('ok')
