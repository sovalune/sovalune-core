//! Сборщик контекста для промпта модели.
//!
//! ContextBuilder собирает системный промпт, секции памяти,
//! историю сообщений и пользовательский ввод в оптимальном порядке,
//! учитывая лимит токенов.

use crate::types::ContextSection;

/// Лимит токенов по умолчанию для контекстного окна.
const DEFAULT_CONTEXT_WINDOW: usize = 128_000;

/// Сборщик контекста — собирает промпт из различных источников.
///
/// # Пример
///
/// ```rust
/// use sovalune_model_runtime::context::ContextBuilder;
///
/// let context = ContextBuilder::new(4096)
///     .with_system("You are a helpful assistant.")
///     .with_memory_section("verified", "User prefers Python.")
///     .with_history_entry("user", "Hello!")
///     .with_history_entry("assistant", "Hi! How can I help?")
///     .with_user_input("Write a sorting algorithm")
///     .build();
/// ```
pub struct ContextBuilder {
    /// Максимальный размер контекстного окна в токенах.
    max_tokens: usize,
    /// Секции контекста.
    sections: Vec<ContextSection>,
    /// Текущее использование токенов.
    tokens_used: usize,
}

impl ContextBuilder {
    /// Создаёт новый сборщик с указанным лимитом токенов.
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_tokens,
            sections: Vec::new(),
            tokens_used: 0,
        }
    }

    /// Создаёт сборщик с лимитом по умолчанию (128K токенов).
    pub fn with_default_limit() -> Self {
        Self::new(DEFAULT_CONTEXT_WINDOW)
    }

    /// Добавляет системный промпт (наивысший приоритет).
    pub fn with_system(mut self, content: &str) -> Self {
        self.add_section(ContextSection {
            role: "system".to_string(),
            content: content.to_string(),
            priority: 0,
            token_estimate: self.estimate_tokens(content),
        });
        self
    }

    /// Добавляет секцию памяти (проверенная/верифицированная информация).
    pub fn with_memory_section(mut self, tier: &str, content: &str) -> Self {
        let priority = match tier {
            "verified" => 10,
            "consolidated" => 20,
            "raw" => 30,
            _ => 25,
        };

        self.add_section(ContextSection {
            role: "memory".to_string(),
            content: format!("[{}] {}", tier.to_uppercase(), content),
            priority,
            token_estimate: self.estimate_tokens(content),
        });
        self
    }

    /// Добавляет запись из истории сообщений.
    pub fn with_history_entry(mut self, role: &str, content: &str) -> Self {
        self.add_section(ContextSection {
            role: "history".to_string(),
            content: format!("{}: {}", role, content),
            priority: 40,
            token_estimate: self.estimate_tokens(content),
        });
        self
    }

    /// Добавляет пользовательский ввод (самый низкий приоритет по позиции).
    pub fn with_user_input(mut self, content: &str) -> Self {
        self.add_section(ContextSection {
            role: "user".to_string(),
            content: content.to_string(),
            priority: 50,
            token_estimate: self.estimate_tokens(content),
        });
        self
    }

    /// Добавляет произвольную секцию.
    pub fn with_section(mut self, section: ContextSection) -> Self {
        self.add_section(section);
        self
    }

    /// Собирает финальный контекст, сортируя по приоритету
    /// и обрезая低-priority секции при превышении лимита.
    pub fn build(mut self) -> Vec<ContextSection> {
        // Сортируем по приоритету (0 = highest)
        self.sections.sort_by_key(|s| s.priority);

        let mut result = Vec::new();
        let mut tokens_used = 0;

        for section in &self.sections {
            if tokens_used + section.token_estimate <= self.max_tokens {
                tokens_used += section.token_estimate;
                result.push(section.clone());
            } else {
                // Пытаемся обрезать секцию, если она не system
                if section.role != "system" && tokens_used < self.max_tokens {
                    let remaining = self.max_tokens - tokens_used;
                    let truncated = self.truncate_to_tokens(&section.content, remaining);
                    if !truncated.is_empty() {
                        result.push(ContextSection {
                            content: truncated,
                            token_estimate: remaining,
                            ..section.clone()
                        });
                    }
                }
            }
        }

        result
    }

    /// Возвращает количество использованных токенов.
    pub fn tokens_used(&self) -> usize {
        self.tokens_used
    }

    /// Возвращает оставшуюся ёмкость контекстного окна.
    pub fn remaining_capacity(&self) -> usize {
        self.max_tokens.saturating_sub(self.tokens_used)
    }

    /// Добавляет секцию с учётом лимита.
    fn add_section(&mut self, section: ContextSection) {
        self.tokens_used += section.token_estimate;
        self.sections.push(section);
    }

    /// Грубая оценка количества токенов (4 символа ≈ 1 токен).
    fn estimate_tokens(&self, text: &str) -> usize {
        text.len().div_ceil(4)
    }

    /// Обрезает текст до указанного количества токенов.
    fn truncate_to_tokens(&self, text: &str, max_tokens: usize) -> String {
        let max_chars = max_tokens * 4;
        if text.len() <= max_chars {
            text.to_string()
        } else {
            format!("{}...", &text[..max_chars.saturating_sub(3)])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_context_building() {
        let context = ContextBuilder::new(1000)
            .with_system("You are helpful.")
            .with_user_input("Hello!")
            .build();

        assert_eq!(context.len(), 2);
        assert_eq!(context[0].role, "system");
        assert_eq!(context[1].role, "user");
    }

    #[test]
    fn test_priority_ordering() {
        let context = ContextBuilder::new(10000)
            .with_user_input("Low priority")
            .with_system("High priority")
            .with_memory_section("verified", "Memory content")
            .build();

        assert_eq!(context[0].role, "system");
        assert_eq!(context[1].role, "memory");
        assert_eq!(context[2].role, "user");
    }

    #[test]
    fn test_truncation() {
        let long_text = "a".repeat(10000);
        let context = ContextBuilder::new(100) // Very small limit
            .with_system("System prompt")
            .with_user_input(&long_text)
            .build();

        // System should always fit
        assert!(context.iter().any(|s| s.role == "system"));
    }
}
