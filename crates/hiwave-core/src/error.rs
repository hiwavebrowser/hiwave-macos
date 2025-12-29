//! Error types for HiWave

use thiserror::Error;

/// Result type alias for HiWave operations
pub type HiWaveResult<T> = Result<T, HiWaveError>;

/// Main error type for HiWave
#[derive(Error, Debug)]
pub enum HiWaveError {
    #[error("Parse error: {0}")]
    Parse(String),

    #[error("DOM error: {0}")]
    Dom(String),

    #[error("Layout error: {0}")]
    Layout(String),

    #[error("JavaScript error: {0}")]
    JavaScript(String),

    #[error("Render error: {0}")]
    Render(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Ad blocking error: {0}")]
    AdBlock(String),

    #[error("Vault error: {0}")]
    Vault(String),

    #[error("Analytics error: {0}")]
    Analytics(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("URL error: {0}")]
    Url(#[from] url::ParseError),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl HiWaveError {
    /// Create a new parse error
    pub fn parse(msg: impl Into<String>) -> Self {
        Self::Parse(msg.into())
    }

    /// Create a new DOM error
    pub fn dom(msg: impl Into<String>) -> Self {
        Self::Dom(msg.into())
    }

    /// Create a new layout error
    pub fn layout(msg: impl Into<String>) -> Self {
        Self::Layout(msg.into())
    }

    /// Create a new JavaScript error
    pub fn js(msg: impl Into<String>) -> Self {
        Self::JavaScript(msg.into())
    }

    /// Create a new render error
    pub fn render(msg: impl Into<String>) -> Self {
        Self::Render(msg.into())
    }

    /// Create a new network error
    pub fn network(msg: impl Into<String>) -> Self {
        Self::Network(msg.into())
    }

    /// Create a new analytics error
    pub fn analytics(msg: impl Into<String>) -> Self {
        Self::Analytics(msg.into())
    }
}
