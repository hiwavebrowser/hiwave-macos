//! Retry and timeout utilities.

use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, warn};

/// Retry configuration.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of attempts (1 = no retries).
    pub max_attempts: u32,
    /// Initial delay between retries.
    pub initial_delay: Duration,
    /// Maximum delay between retries.
    pub max_delay: Duration,
    /// Backoff multiplier (e.g., 2.0 for exponential).
    pub backoff_multiplier: f64,
    /// Add jitter to delays.
    pub jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            backoff_multiplier: 2.0,
            jitter: true,
        }
    }
}

impl RetryConfig {
    /// Create a config for no retries.
    pub fn none() -> Self {
        Self {
            max_attempts: 1,
            ..Default::default()
        }
    }

    /// Create a config for aggressive retries.
    pub fn aggressive() -> Self {
        Self {
            max_attempts: 5,
            initial_delay: Duration::from_millis(50),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            jitter: true,
        }
    }

    /// Calculate delay for a given attempt (1-indexed).
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        if attempt <= 1 {
            return Duration::ZERO;
        }

        let base_delay =
            self.initial_delay.as_secs_f64() * self.backoff_multiplier.powi((attempt - 2) as i32);

        let delay = Duration::from_secs_f64(base_delay.min(self.max_delay.as_secs_f64()));

        if self.jitter {
            // Add up to 25% jitter
            let jitter = delay.as_secs_f64() * (rand_jitter() * 0.25);
            delay + Duration::from_secs_f64(jitter)
        } else {
            delay
        }
    }
}

/// Simple pseudo-random jitter (0.0 to 1.0).
fn rand_jitter() -> f64 {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    (nanos as f64) / (u32::MAX as f64)
}

/// Retry a fallible async operation with exponential backoff.
pub async fn retry_with_backoff<T, E, F, Fut>(
    config: &RetryConfig,
    mut operation: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut last_error = None;

    for attempt in 1..=config.max_attempts {
        // Wait before retry (not on first attempt)
        if attempt > 1 {
            let delay = config.delay_for_attempt(attempt);
            debug!(attempt, ?delay, "Retrying after delay");
            sleep(delay).await;
        }

        match operation().await {
            Ok(value) => {
                if attempt > 1 {
                    debug!(attempt, "Operation succeeded after retries");
                }
                return Ok(value);
            }
            Err(e) => {
                warn!(attempt, max_attempts = config.max_attempts, error = %e, "Operation failed");
                last_error = Some(e);
            }
        }
    }

    Err(last_error.expect("At least one attempt should have been made"))
}

/// Run an operation with a timeout.
pub async fn with_timeout<T, F, Fut>(
    timeout: Duration,
    operation: F,
) -> Result<T, crate::RustKitError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = T>,
{
    tokio::time::timeout(timeout, operation())
        .await
        .map_err(|_| crate::RustKitError::Timeout(timeout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
    }

    #[test]
    fn test_retry_config_none() {
        let config = RetryConfig::none();
        assert_eq!(config.max_attempts, 1);
    }

    #[test]
    fn test_delay_for_attempt() {
        let config = RetryConfig {
            initial_delay: Duration::from_millis(100),
            backoff_multiplier: 2.0,
            jitter: false,
            ..Default::default()
        };

        // First attempt has no delay
        assert_eq!(config.delay_for_attempt(1), Duration::ZERO);

        // Second attempt uses initial delay
        assert_eq!(config.delay_for_attempt(2), Duration::from_millis(100));

        // Third attempt doubles
        assert_eq!(config.delay_for_attempt(3), Duration::from_millis(200));

        // Fourth attempt doubles again
        assert_eq!(config.delay_for_attempt(4), Duration::from_millis(400));
    }

    #[test]
    fn test_delay_respects_max() {
        let config = RetryConfig {
            initial_delay: Duration::from_secs(10),
            max_delay: Duration::from_secs(15),
            backoff_multiplier: 2.0,
            jitter: false,
            ..Default::default()
        };

        // Second attempt: 10s
        assert_eq!(config.delay_for_attempt(2), Duration::from_secs(10));

        // Third attempt: would be 20s but capped at 15s
        assert_eq!(config.delay_for_attempt(3), Duration::from_secs(15));
    }

    #[tokio::test]
    async fn test_retry_success_first_attempt() {
        let config = RetryConfig::default();
        let mut attempts = 0;

        let result: Result<i32, &str> = retry_with_backoff(&config, || {
            attempts += 1;
            async { Ok(42) }
        })
        .await;

        assert_eq!(result, Ok(42));
        assert_eq!(attempts, 1);
    }

    #[tokio::test]
    async fn test_retry_success_after_failures() {
        let config = RetryConfig {
            max_attempts: 3,
            initial_delay: Duration::from_millis(1),
            jitter: false,
            ..Default::default()
        };

        let attempts = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let result: Result<i32, &str> = retry_with_backoff(&config, || {
            let attempt = attempts_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            async move {
                if attempt < 3 {
                    Err("not yet")
                } else {
                    Ok(42)
                }
            }
        })
        .await;

        assert_eq!(result, Ok(42));
        assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_timeout() {
        let result = with_timeout(Duration::from_millis(10), || async {
            sleep(Duration::from_secs(1)).await;
            42
        })
        .await;

        assert!(matches!(result, Err(crate::RustKitError::Timeout(_))));
    }
}
