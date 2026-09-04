//! Integration tests for sovalune-model-runtime.
//!
//! Tests the full inference pipeline: cache, retry, tools, and engine.

#[cfg(test)]
mod tests {
    use sovalune_model_runtime::cache::{model_cache_key, Cache, CacheFactory};
    use sovalune_model_runtime::retry::{should_retry, RetryConfig, RetryState};
    use sovalune_model_runtime::tools::{ToolCall, ToolCallParser, ToolRegistry};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_cache_hit_miss() {
        let cache: Cache<String, String> = Cache::new(10, std::time::Duration::from_secs(60));

        assert_eq!(cache.get(&"key1".to_string()).await, None);

        cache.insert("key1".to_string(), "value1".to_string()).await;
        assert_eq!(
            cache.get(&"key1".to_string()).await,
            Some("value1".to_string())
        );
    }

    #[tokio::test]
    async fn test_cache_expiry() {
        let cache: Cache<String, String> = Cache::new(10, std::time::Duration::from_millis(5));

        cache.insert("k".to_string(), "v".to_string()).await;
        assert_eq!(cache.get(&"k".to_string()).await, Some("v".to_string()));

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert_eq!(cache.get(&"k".to_string()).await, None);
    }

    #[tokio::test]
    async fn test_cache_eviction() {
        let cache: Cache<String, String> = Cache::new(2, std::time::Duration::from_secs(60));

        cache.insert("a".to_string(), "1".to_string()).await;
        cache.insert("b".to_string(), "2".to_string()).await;
        cache.insert("c".to_string(), "3".to_string()).await;

        // Cache size should be at most 2
        assert!(cache.len().await <= 2);
        // At least one of the later inserts should be present
        let has_b = cache.get(&"b".to_string()).await.is_some();
        let has_c = cache.get(&"c".to_string()).await.is_some();
        assert!(has_b || has_c);
    }

    #[test]
    fn test_cache_key_deterministic() {
        let msgs = vec![serde_json::json!({"role": "user", "content": "hello"})];
        let k1 = model_cache_key("gpt-4", &msgs, 0.7, 1000);
        let k2 = model_cache_key("gpt-4", &msgs, 0.7, 1000);
        let k3 = model_cache_key("gpt-4", &msgs, 0.8, 1000);

        assert_eq!(k1, k2);
        assert_ne!(k1, k3);
    }

    #[test]
    fn test_retry_config_defaults() {
        let c = RetryConfig::default();
        assert_eq!(c.max_retries, 3);
        assert_eq!(c.base_delay_ms, 1000);

        let rl = RetryConfig::for_rate_limit();
        assert_eq!(rl.max_retries, 5);

        let t = RetryConfig::for_timeout();
        assert_eq!(t.max_retries, 2);
    }

    #[test]
    fn test_retry_delay_increases() {
        let c = RetryConfig::default();
        let d0 = c.delay_for_attempt(0);
        let d1 = c.delay_for_attempt(1);
        let d2 = c.delay_for_attempt(2);

        assert!(d1 > d0);
        assert!(d2 > d1);
    }

    #[test]
    fn test_retry_state() {
        let mut s = RetryState::new(RetryConfig::default());
        assert_eq!(s.attempt(), 0);
        assert!(s.has_retries_left());

        let _d = s.increment();
        assert_eq!(s.attempt(), 1);

        s.reset();
        assert_eq!(s.attempt(), 0);
    }

    #[test]
    fn test_should_retry() {
        assert!(should_retry("rate limit exceeded"));
        assert!(should_retry("429 Too Many Requests"));
        assert!(should_retry("connection timeout"));
        assert!(should_retry("500 Internal Server Error"));
        assert!(should_retry("502 Bad Gateway"));
        assert!(should_retry("503 Service Unavailable"));
        assert!(!should_retry("400 Bad Request"));
        assert!(!should_retry("404 Not Found"));
    }

    #[test]
    fn test_tool_registry() {
        let registry = ToolRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_parse_openai_tool_calls() {
        let calls = ToolCallParser::parse_openai_tool_calls(&[serde_json::json!({
            "id": "call_123",
            "function": {
                "name": "memory_search",
                "arguments": "{\"query\": \"hello\"}"
            }
        })]);

        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_123");
        assert_eq!(calls[0].name, "memory_search");
        assert_eq!(calls[0].arguments["query"], "hello");
    }

    #[test]
    fn test_parse_text_tool_calls() {
        let text = r#"Here is a tool call:
```json
{"name": "code_execute", "arguments": {"code": "print(1)", "language": "python"}}
```
That should work."#;

        let calls = ToolCallParser::parse_text_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "code_execute");
        assert_eq!(calls[0].arguments["language"], "python");
    }

    #[tokio::test]
    async fn test_tool_call_manager() {
        let registry = Arc::new(ToolRegistry::new());
        let mut manager = sovalune_model_runtime::tools::ToolCallManager::new(registry);

        assert!(!manager.has_pending());
        assert_eq!(manager.pending_count(), 0);

        manager.enqueue(ToolCall {
            id: "1".to_string(),
            name: "test".to_string(),
            arguments: serde_json::json!({}),
        });

        assert!(manager.has_pending());
        assert_eq!(manager.pending_count(), 1);

        let call = manager.next_call();
        assert!(call.is_some());
        assert!(!manager.has_pending());

        manager.clear();
        assert!(!manager.has_pending());
    }

    #[test]
    fn test_cache_factory() {
        let _rc = CacheFactory::response_cache();
        let _ec = CacheFactory::embedding_cache();
        let _tc = CacheFactory::token_count_cache();
    }
}
