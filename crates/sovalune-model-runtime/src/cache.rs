//! Модуль кеширования запросов и ответов.
//!
//! LRU-кеш для ответов модели и эмбеддингов.
//! Уменьшает количество запросов к API и ускоряет повторные обращения.
//!
//! # Стратегия
//!
//! - Кеш ответов: TTL 5 минут, max 1000 записей
//! - Кеш эмбеддингов: TTL 1 час, max 10000 записей
//! - Кеш токенов: без TTL, max 50000 записей

use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::debug;

/// Запись в кеше с TTL.
#[derive(Debug, Clone)]
struct CacheEntry<V> {
    value: V,
    created_at: Instant,
    ttl: Duration,
}

impl<V> CacheEntry<V> {
    fn new(value: V, ttl: Duration) -> Self {
        Self {
            value,
            created_at: Instant::now(),
            ttl,
        }
    }

    fn is_expired(&self) -> bool {
        self.created_at.elapsed() > self.ttl
    }
}

/// LRU-кеш с TTL.
pub struct Cache<K, V> {
    entries: RwLock<HashMap<K, CacheEntry<V>>>,
    max_size: usize,
    default_ttl: Duration,
}

impl<K, V> Cache<K, V>
where
    K: Eq + std::hash::Hash + Clone,
    V: Clone,
{
    /// Создаёт новый кеш.
    pub fn new(max_size: usize, default_ttl: Duration) -> Self {
        Self {
            entries: RwLock::new(HashMap::with_capacity(max_size)),
            max_size,
            default_ttl,
        }
    }

    /// Получает значение из кеша.
    pub async fn get(&self, key: &K) -> Option<V> {
        let entries = self.entries.read().await;
        if let Some(entry) = entries.get(key) {
            if !entry.is_expired() {
                debug!("Cache hit");
                return Some(entry.value.clone());
            }
        }
        debug!("Cache miss");
        None
    }

    /// Сохраняет значение в кеш.
    pub async fn insert(&self, key: K, value: V) {
        let mut entries = self.entries.write().await;

        // Если кеш полон, удаляем старые записи
        if entries.len() >= self.max_size {
            self.evict_expired(&mut entries);
            if entries.len() >= self.max_size {
                // Если всё ещё полон, удаляем случайную запись
                if let Some(first_key) = entries.keys().next().cloned() {
                    entries.remove(&first_key);
                }
            }
        }

        entries.insert(key, CacheEntry::new(value, self.default_ttl));
    }

    /// Сохраняет значение с кастомным TTL.
    pub async fn insert_with_ttl(&self, key: K, value: V, ttl: Duration) {
        let mut entries = self.entries.write().await;
        entries.insert(key, CacheEntry::new(value, ttl));
    }

    /// Удаляет значение из кеша.
    pub async fn remove(&self, key: &K) -> Option<V> {
        let mut entries = self.entries.write().await;
        entries.remove(key).map(|e| e.value)
    }

    /// Очищает кеш.
    pub async fn clear(&self) {
        let mut entries = self.entries.write().await;
        entries.clear();
    }

    /// Возвращает размер кеша.
    pub async fn len(&self) -> usize {
        let entries = self.entries.read().await;
        entries.len()
    }

    /// Проверяет, пуст ли кеш.
    pub async fn is_empty(&self) -> bool {
        let entries = self.entries.read().await;
        entries.is_empty()
    }

    /// Удаляет протухшие записи.
    fn evict_expired(&self, entries: &mut HashMap<K, CacheEntry<V>>) {
        entries.retain(|_, entry| !entry.is_expired());
    }
}

/// Кеш ответов модели.
pub type ResponseCache = Cache<String, String>;

/// Кеш эмбеддингов.
pub type EmbeddingCache = Cache<String, Vec<f32>>;

/// Кеш подсчёта токенов.
pub type TokenCountCache = Cache<String, usize>;

/// Фабрика кешей.
pub struct CacheFactory;

impl CacheFactory {
    /// Создаёт кеш ответов модели.
    pub fn response_cache() -> ResponseCache {
        Cache::new(1000, Duration::from_secs(300)) // 5 минут
    }

    /// Создаёт кеш эмбеддингов.
    pub fn embedding_cache() -> EmbeddingCache {
        Cache::new(10000, Duration::from_secs(3600)) // 1 час
    }

    /// Создаёт кеш подсчёта токенов.
    pub fn token_count_cache() -> TokenCountCache {
        Cache::new(50000, Duration::from_secs(u64::MAX)) // без TTL
    }
}

/// Генерирует ключ кеша для запроса к модели.
pub fn model_cache_key(
    model: &str,
    messages: &[serde_json::Value],
    temperature: f32,
    max_tokens: u32,
) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    model.hash(&mut hasher);
    temperature.to_bits().hash(&mut hasher);
    max_tokens.hash(&mut hasher);

    for msg in messages {
        if let Some(content) = msg["content"].as_str() {
            content.hash(&mut hasher);
        }
        if let Some(role) = msg["role"].as_str() {
            role.hash(&mut hasher);
        }
    }

    format!("model:{:016x}", hasher.finish())
}

/// Генерирует ключ кеша для эмбеддинга.
pub fn embedding_cache_key(model: &str, text: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    model.hash(&mut hasher);
    text.hash(&mut hasher);

    format!("embed:{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_basic() {
        let cache = Cache::new(10, Duration::from_secs(60));

        cache.insert("key1".to_string(), "value1".to_string()).await;
        assert_eq!(
            cache.get(&"key1".to_string()).await,
            Some("value1".to_string())
        );
        assert_eq!(cache.get(&"key2".to_string()).await, None);
    }

    #[tokio::test]
    async fn test_cache_expiry() {
        let cache = Cache::new(10, Duration::from_millis(10));

        cache.insert("key1".to_string(), "value1".to_string()).await;
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(cache.get(&"key1".to_string()).await, None);
    }

    #[tokio::test]
    async fn test_cache_eviction() {
        let cache = Cache::new(2, Duration::from_secs(60));

        cache.insert("key1".to_string(), "value1".to_string()).await;
        cache.insert("key2".to_string(), "value2".to_string()).await;
        cache.insert("key3".to_string(), "value3".to_string()).await;

        // Cache size should be at most 2
        assert!(cache.len().await <= 2);
        // At least one of the later inserts should be present
        let has_v2 = cache.get(&"key2".to_string()).await.is_some();
        let has_v3 = cache.get(&"key3".to_string()).await.is_some();
        assert!(has_v2 || has_v3);
    }

    #[test]
    fn test_cache_key_generation() {
        let key1 = model_cache_key("gpt-4", &[], 0.7, 1000);
        let key2 = model_cache_key("gpt-4", &[], 0.7, 1000);
        let key3 = model_cache_key("gpt-4", &[], 0.8, 1000);

        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
    }
}
