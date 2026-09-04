//! Модуль повторных попыток с экспоненциальной задержкой.
//!
//! Автоматически повторяет запросы при ошибках (rate limit, timeout, server errors)
//! с экспоненциальной задержкой и jitter для предотвращения thundering herd.
//!
//! # Стратегия
//!
//! - Base delay: 1 секунда
//! - Multiplier: 2x
//! - Max delay: 60 секунд
//! - Jitter: ±20%
//! - Max retries: 3

use std::time::Duration;
use tracing::{debug, warn};

/// Конфигурация повторных попыток.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Максимальное количество повторных попыток.
    pub max_retries: u32,
    /// Базовая задержка (мс).
    pub base_delay_ms: u64,
    /// Максимальная задержка (мс).
    pub max_delay_ms: u64,
    /// Множитель задержки.
    pub multiplier: f64,
    /// Jitter factor (0.0 - 1.0).
    pub jitter: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 1000,
            max_delay_ms: 60000,
            multiplier: 2.0,
            jitter: 0.2,
        }
    }
}

impl RetryConfig {
    /// Конфигурация для rate limit ошибок (более агрессивная).
    pub fn for_rate_limit() -> Self {
        Self {
            max_retries: 5,
            base_delay_ms: 2000,
            max_delay_ms: 120000,
            multiplier: 2.0,
            jitter: 0.3,
        }
    }

    /// Конфигурация для timeout ошибок.
    pub fn for_timeout() -> Self {
        Self {
            max_retries: 2,
            base_delay_ms: 500,
            max_delay_ms: 5000,
            multiplier: 2.0,
            jitter: 0.1,
        }
    }

    /// Конфигурация для server errors (5xx).
    pub fn for_server_error() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 1000,
            max_delay_ms: 30000,
            multiplier: 2.0,
            jitter: 0.2,
        }
    }

    /// Вычисляет задержку для указанной попытки.
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let base = self.base_delay_ms as f64 * self.multiplier.powi(attempt as i32);
        let capped = base.min(self.max_delay_ms as f64);

        // Добавляем jitter
        let jitter_range = capped * self.jitter;
        let jitter = (rand() * 2.0 - 1.0) * jitter_range;
        let delay = (capped + jitter).max(0.0);

        Duration::from_millis(delay as u64)
    }
}

/// Простой генератор случайных чисел (0.0 - 1.0).
fn rand() -> f64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    let s = RandomState::new();
    let mut hasher = s.build_hasher();
    hasher.write_u64(0);
    let bits = hasher.finish();
    (bits as f64) / (u64::MAX as f64)
}

/// Определяет, нужно ли повторять запрос для данной ошибки.
pub fn should_retry(error: &str) -> bool {
    let lower = error.to_lowercase();

    // Rate limit — повторяем
    if lower.contains("rate") && lower.contains("limit") {
        return true;
    }
    if lower.contains("429") {
        return true;
    }

    // Timeout — повторяем
    if lower.contains("timeout") {
        return true;
    }

    // Server errors — повторяем
    if lower.contains("500") || lower.contains("502") || lower.contains("503") || lower.contains("504") {
        return true;
    }

    // Network errors — повторяем
    if lower.contains("connection") && lower.contains("reset") {
        return true;
    }
    if lower.contains("eof") {
        return true;
    }

    // Не повторяем для клиентских ошибок (4xx кроме 429)
    false
}

/// Извлекает retry-after из ошибки (если есть).
pub fn extract_retry_after(error: &str) -> Option<Duration> {
    let lower = error.to_lowercase();

    // Ищем "retry after X seconds"
    if let Some(pos) = lower.find("retry after") {
        let rest = &error[pos + 11..];
        if let Some(seconds_str) = rest.split_whitespace().next() {
            if let Ok(seconds) = seconds_str.parse::<u64>() {
                return Some(Duration::from_secs(seconds));
            }
        }
    }

    // Ищем "retry-after: X"
    if let Some(pos) = lower.find("retry-after:") {
        let rest = &error[pos + 12..];
        if let Some(seconds_str) = rest.split_whitespace().next() {
            if let Ok(seconds) = seconds_str.parse::<u64>() {
                return Some(Duration::from_secs(seconds));
            }
        }
    }

    None
}

/// Структура для отслеживания попыток.
pub struct RetryState {
    config: RetryConfig,
    attempt: u32,
}

impl RetryState {
    /// Создаёт новое состояние с указанной конфигурацией.
    pub fn new(config: RetryConfig) -> Self {
        Self { config, attempt: 0 }
    }

    /// Создаёт состояние с конфигурацией по умолчанию.
    pub fn with_default() -> Self {
        Self::new(RetryConfig::default())
    }

    /// Возвращает номер текущей попытки (0-based).
    pub fn attempt(&self) -> u32 {
        self.attempt
    }

    /// Проверяет, есть ли ещё попытки.
    pub fn has_retries_left(&self) -> bool {
        self.attempt < self.config.max_retries
    }

    /// Вычисляет задержку для следующей попытки.
    pub fn next_delay(&self) -> Duration {
        self.config.delay_for_attempt(self.attempt)
    }

    /// Увеличивает счётчик попыток и возвращает задержку.
    pub fn increment(&mut self) -> Duration {
        let delay = self.next_delay();
        self.attempt += 1;
        delay
    }

    /// Сбрасывает счётчик попыток.
    pub fn reset(&mut self) {
        self.attempt = 0;
    }
}

/// Макрос для автоматических повторных попыток.
///
/// # Пример
/// ```rust,ignore
/// let result = retry_with_backoff!(|| async {
///     client.post(&url).send().await
/// }, RetryConfig::default()).await?;
/// ```
#[macro_export]
macro_rules! retry_with_backoff {
    ($operation:expr, $config:expr) => {{
        use $crate::retry::{RetryState, should_retry, extract_retry_after};

        let mut state = RetryState::new($config);
        let mut last_error = None;

        loop {
            match $operation().await {
                Ok(result) => break Ok(result),
                Err(e) => {
                    let error_str = format!("{}", e);
                    last_error = Some(e);

                    if !state.has_retries_left() || !should_retry(&error_str) {
                        break Err(last_error.unwrap());
                    }

                    // Проверяем retry-after заголовок
                    let delay = if let Some(retry_after) = extract_retry_after(&error_str) {
                        retry_after
                    } else {
                        state.increment()
                    };

                    tracing::warn!(
                        "Request failed (attempt {}), retrying in {:?}: {}",
                        state.attempt(),
                        delay,
                        &error_str[..100.min(error_str.len())]
                    );

                    tokio::time::sleep(delay).await;
                }
            }
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.base_delay_ms, 1000);
    }

    #[test]
    fn test_delay_increases() {
        let config = RetryConfig::default();
        let d0 = config.delay_for_attempt(0);
        let d1 = config.delay_for_attempt(1);
        let d2 = config.delay_for_attempt(2);

        assert!(d1 > d0);
        assert!(d2 > d1);
    }

    #[test]
    fn test_should_retry() {
        assert!(should_retry("rate limit exceeded"));
        assert!(should_retry("429 Too Many Requests"));
        assert!(should_retry("connection timeout"));
        assert!(should_retry("500 Internal Server Error"));
        assert!(!should_retry("400 Bad Request"));
        assert!(!should_retry("404 Not Found"));
    }

    #[test]
    fn test_extract_retry_after() {
        assert_eq!(
            extract_retry_after("rate limit, retry after 30 seconds"),
            Some(Duration::from_secs(30))
        );
        assert_eq!(
            extract_retry_after("Too many requests. retry-after: 60"),
            Some(Duration::from_secs(60))
        );
        assert_eq!(extract_retry_after("some other error"), None);
    }
}
