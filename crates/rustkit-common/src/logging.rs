//! Logging configuration and setup.

use tracing::Level;
use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};

/// Log output format.
#[derive(Debug, Clone, Copy, Default)]
pub enum LogFormat {
    /// Human-readable format.
    #[default]
    Pretty,
    /// Compact single-line format.
    Compact,
    /// JSON format for structured logging.
    Json,
}

/// Logging configuration.
#[derive(Debug, Clone)]
pub struct LogConfig {
    /// Log level.
    pub level: Level,
    /// Output format.
    pub format: LogFormat,
    /// Include source file location.
    pub include_location: bool,
    /// Include thread names.
    pub include_thread_names: bool,
    /// Include span events (enter, exit).
    pub include_span_events: bool,
    /// Custom filter string (e.g., "rustkit=debug,wgpu=warn").
    pub filter: Option<String>,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: Level::INFO,
            format: LogFormat::Pretty,
            include_location: false,
            include_thread_names: false,
            include_span_events: false,
            filter: None,
        }
    }
}

impl LogConfig {
    /// Create a debug configuration.
    pub fn debug() -> Self {
        Self {
            level: Level::DEBUG,
            include_location: true,
            include_span_events: true,
            ..Default::default()
        }
    }

    /// Create a trace configuration.
    pub fn trace() -> Self {
        Self {
            level: Level::TRACE,
            include_location: true,
            include_thread_names: true,
            include_span_events: true,
            ..Default::default()
        }
    }

    /// Create a production configuration.
    pub fn production() -> Self {
        Self {
            level: Level::INFO,
            format: LogFormat::Json,
            ..Default::default()
        }
    }

    /// Set a custom filter.
    pub fn with_filter(mut self, filter: impl Into<String>) -> Self {
        self.filter = Some(filter.into());
        self
    }
}

/// Initialize logging with the given configuration.
pub fn init_logging(config: LogConfig) {
    // Build filter
    let filter = if let Some(ref custom_filter) = config.filter {
        EnvFilter::try_new(custom_filter)
            .unwrap_or_else(|_| EnvFilter::new(format!("{}", config.level)))
    } else {
        EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(format!("{}", config.level)))
    };

    // Span events
    let span_events = if config.include_span_events {
        FmtSpan::ENTER | FmtSpan::CLOSE
    } else {
        FmtSpan::NONE
    };

    // Initialize based on format
    match config.format {
        LogFormat::Pretty => {
            let fmt_layer = fmt::layer()
                .with_target(true)
                .with_file(config.include_location)
                .with_line_number(config.include_location)
                .with_thread_names(config.include_thread_names)
                .with_span_events(span_events);

            tracing_subscriber::registry()
                .with(filter)
                .with(fmt_layer)
                .init();
        }
        LogFormat::Compact => {
            let fmt_layer = fmt::layer()
                .compact()
                .with_target(true)
                .with_span_events(span_events);

            tracing_subscriber::registry()
                .with(filter)
                .with(fmt_layer)
                .init();
        }
        LogFormat::Json => {
            let fmt_layer = fmt::layer().json().with_span_events(span_events);

            tracing_subscriber::registry()
                .with(filter)
                .with(fmt_layer)
                .init();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_config_default() {
        let config = LogConfig::default();
        assert_eq!(config.level, Level::INFO);
        assert!(!config.include_location);
    }

    #[test]
    fn test_log_config_debug() {
        let config = LogConfig::debug();
        assert_eq!(config.level, Level::DEBUG);
        assert!(config.include_location);
    }

    #[test]
    fn test_log_config_with_filter() {
        let config = LogConfig::default().with_filter("rustkit=debug");
        assert_eq!(config.filter, Some("rustkit=debug".to_string()));
    }
}
