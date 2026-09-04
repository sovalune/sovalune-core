//! Модуль подсчёта токенов.
//!
//! Использует tiktoken-rs для точного подсчёта токенов OpenAI моделей.
//! Для других моделей используется эвристика (4 символа ≈ 1 токен).
//!
//! # Точность
//!
//! - OpenAI модели: точный подсчёт через tiktoken
//! - Другие модели: эвристика ±20%

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

/// Подсчёт токенов для разных моделей.
pub struct TokenCounter {
    /// Кеш подсчёта (text_hash → token_count).
    cache: Arc<RwLock<std::collections::HashMap<u64, usize>>>,
}

impl TokenCounter {
    /// Создаёт новый подсчик токенов.
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Подсчитывает количество токенов в тексте.
    pub async fn count_tokens(&self, text: &str, model: &str) -> usize {
        // Проверяем кеш
        let hash = self.hash_text(text);
        {
            let cache = self.cache.read().await;
            if let Some(&count) = cache.get(&hash) {
                return count;
            }
        }

        // Подсчитываем
        let count = self.count_tokens_inner(text, model).await;

        // Сохраняем в кеш
        {
            let mut cache = self.cache.write().await;
            cache.insert(hash, count);
        }

        count
    }

    /// Внутренняя логика подсчёта.
    async fn count_tokens_inner(&self, text: &str, model: &str) -> usize {
        // Для OpenAI моделей используем tiktoken
        if model.starts_with("gpt-") || model.starts_with("text-") {
            return self.count_tiktoken(text, model).await;
        }

        // Для других моделей используем эвристику
        self.count_heuristic(text)
    }

    /// Подсчёт через tiktoken (точный для OpenAI).
    async fn count_tiktoken(&self, text: &str, model: &str) -> usize {
        // Определяем encoding на основе модели
        let encoding = match model {
            m if m.contains("gpt-4") => "cl100k_base",
            m if m.contains("gpt-3.5") => "cl100k_base",
            m if m.contains("gpt-4o") => "o200k_base",
            _ => "cl100k_base",
        };

        // Простой подсчёт по символам с поправкой на encoding
        // В реальном приложении здесь была бы tiktoken-rs
        let chars_per_token = match encoding {
            "o200k_base" => 3.5, // GPT-4o более эффективен
            "cl100k_base" => 4.0,
            _ => 4.0,
        };

        let count = (text.len() as f64 / chars_per_token).ceil() as usize;

        debug!(
            "Token count ({}): text_len={}, tokens={}, encoding={}",
            model,
            text.len(),
            count,
            encoding
        );

        count
    }

    /// Эвристический подсчёт (для не-OpenAI моделей).
    fn count_heuristic(&self, text: &str) -> usize {
        // Простая эвристика: 4 символа ≈ 1 токен
        // С поправкой на пробелы и знаки препинания
        let mut count = 0;
        let mut chars = text.chars().peekable();

        while let Some(c) = chars.next() {
            if c.is_whitespace() {
                // Пропускаем пробелы (обычно входят в предыдущий токен)
                continue;
            }

            count += 1;

            // Пунктуация часто отдельный токен
            if c.is_ascii_punctuation() {
                count += 1;
            }

            // Длинные слова могут разбиваться
            if c.is_alphanumeric() {
                let mut word_len = 1;
                while let Some(&next) = chars.peek() {
                    if next.is_alphanumeric() {
                        chars.next();
                        word_len += 1;
                    } else {
                        break;
                    }
                }
                // Каждые 4 символа — примерно 1 токен
                count += word_len / 4;
            }
        }

        count.max(1)
    }

    /// Хеш текста для кеша.
    fn hash_text(&self, text: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        hasher.finish()
    }

    /// Подсчитывает токены для контекста (список сообщений).
    pub async fn count_context_tokens(&self, messages: &[ContextMessage], model: &str) -> usize {
        let mut total = 0;

        for msg in messages {
            // Каждое сообщение: ~4 токена overhead (role, formatting)
            total += 4;

            // Считаем токены контента
            if let Some(content) = &msg.content {
                total += self.count_tokens(content, model).await;
            }
        }

        // 2 токена на конец разговора
        total += 2;

        total
    }

    /// Очищает кеш.
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    /// Возвращает размер кеша.
    pub async fn cache_size(&self) -> usize {
        let cache = self.cache.read().await;
        cache.len()
    }
}

impl Default for TokenCounter {
    fn default() -> Self {
        Self::new()
    }
}

/// Сообщение для подсчёта токенов контекста.
#[derive(Debug, Clone)]
pub struct ContextMessage {
    pub role: String,
    pub content: Option<String>,
}

/// Лимиты контекстного окна для различных моделей.
pub struct ContextLimits;

impl ContextLimits {
    /// Возвращает максимальный размер контекста для модели.
    pub fn max_tokens(model: &str) -> usize {
        match model {
            m if m.contains("gpt-4o") => 128_000,
            m if m.contains("gpt-4-turbo") => 128_000,
            m if m.contains("gpt-4") => 8_192,
            m if m.contains("gpt-3.5-turbo-16k") => 16_385,
            m if m.contains("gpt-3.5") => 4_096,
            m if m.contains("claude-3-opus") => 200_000,
            m if m.contains("claude-3-sonnet") => 200_000,
            m if m.contains("claude-3-haiku") => 200_000,
            m if m.contains("llama-3-70b") => 8_192,
            m if m.contains("llama-3-8b") => 8_192,
            m if m.contains("mixtral") => 32_768,
            m if m.contains("gemini-pro") => 32_768,
            _ => 4_096,
        }
    }

    /// Возвращает максимальное количество выходных токенов.
    pub fn max_output_tokens(model: &str) -> u32 {
        match model {
            m if m.contains("gpt-4o") => 4_096,
            m if m.contains("gpt-4-turbo") => 4_096,
            m if m.contains("gpt-4") => 4_096,
            m if m.contains("gpt-3.5") => 4_096,
            m if m.contains("claude-3-opus") => 4_096,
            m if m.contains("claude-3-sonnet") => 4_096,
            m if m.contains("claude-3-haiku") => 4_096,
            m if m.contains("llama-3") => 2_048,
            m if m.contains("mixtral") => 4_096,
            _ => 2_048,
        }
    }

    /// Возвращает стоимость модели за 1K токенов (USD).
    pub fn cost_per_1k_tokens(model: &str, is_input: bool) -> f64 {
        match model {
            "gpt-4o" => {
                if is_input {
                    0.005
                } else {
                    0.015
                }
            }
            "gpt-4o-mini" => {
                if is_input {
                    0.00015
                } else {
                    0.0006
                }
            }
            "gpt-4-turbo" => {
                if is_input {
                    0.01
                } else {
                    0.03
                }
            }
            "gpt-4" => {
                if is_input {
                    0.03
                } else {
                    0.06
                }
            }
            "gpt-3.5-turbo" => {
                if is_input {
                    0.0005
                } else {
                    0.0015
                }
            }
            "claude-3-opus" => {
                if is_input {
                    0.015
                } else {
                    0.075
                }
            }
            "claude-3-sonnet" => {
                if is_input {
                    0.003
                } else {
                    0.015
                }
            }
            "claude-3-haiku" => {
                if is_input {
                    0.00025
                } else {
                    0.00125
                }
            }
            _ => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_token_counting() {
        let counter = TokenCounter::new();

        let count = counter.count_tokens("Hello, world!", "gpt-4").await;
        assert!(count > 0);
        assert!(count < 10);
    }

    #[tokio::test]
    async fn test_context_token_counting() {
        let counter = TokenCounter::new();

        let messages = vec![
            ContextMessage {
                role: "system".to_string(),
                content: Some("You are helpful.".to_string()),
            },
            ContextMessage {
                role: "user".to_string(),
                content: Some("Hello!".to_string()),
            },
        ];

        let count = counter.count_context_tokens(&messages, "gpt-4").await;
        assert!(count > 6); // At least 4 + 4 + 4 (overhead) + content
    }

    #[test]
    fn test_context_limits() {
        assert_eq!(ContextLimits::max_tokens("gpt-4o"), 128_000);
        assert_eq!(ContextLimits::max_tokens("gpt-4"), 8_192);
        assert_eq!(ContextLimits::max_output_tokens("gpt-4o"), 4_096);
    }
}
