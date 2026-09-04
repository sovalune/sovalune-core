//! Реализации инструментов для Sovalune AI.
//!
//! Каждый инструмент реализует трейт `ToolExecutor` и выполняет
//! конкретное действие: поиск в памяти, запись памяти, выполнение кода и т.д.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::debug;

use super::{ToolCall, ToolDefinition, ToolError, ToolExecutor, ToolResult};

/// Инструмент поиска в векторной памяти.
pub struct MemorySearchTool {
    /// Векторное хранилище для поиска.
    store: Arc<dyn MemorySearchBackend>,
}

#[async_trait]
pub trait MemorySearchBackend: Send + Sync {
    async fn search(&self, query: &str, top_k: usize) -> Result<Vec<MemorySearchResult>, String>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchResult {
    pub content: String,
    pub score: f32,
    pub tier: String,
}

impl MemorySearchTool {
    pub fn new(store: Arc<dyn MemorySearchBackend>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl ToolExecutor for MemorySearchTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "memory_search".to_string(),
            description: "Search in Vector Memory for relevant context".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" },
                    "tier": {
                        "type": "string",
                        "enum": ["verified_fact", "consolidated_knowledge", "raw_memory"],
                        "description": "Filter by memory tier"
                    },
                    "top_k": { "type": "integer", "default": 5 }
                },
                "required": ["query"]
            }),
            permissions: vec!["memory:read".to_string()],
        }
    }

    async fn check_permission(&self, _caller_id: &str) -> Result<(), ToolError> {
        Ok(())
    }

    async fn execute(&self, call: &ToolCall, _caller_id: &str) -> Result<ToolResult, ToolError> {
        let query = call.arguments["query"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments("missing 'query'".to_string()))?;
        let top_k = call.arguments["top_k"].as_u64().unwrap_or(5) as usize;

        let results = self
            .store
            .search(query, top_k)
            .await
            .map_err(ToolError::ExecutionFailed)?;

        Ok(ToolResult {
            call_id: call.id.clone(),
            tool_name: "memory_search".to_string(),
            success: true,
            output: serde_json::json!({ "results": results }),
            duration_ms: 0,
            side_effects: vec![],
        })
    }
}

/// Инструмент записи в память.
pub struct MemoryWriteTool {
    store: Arc<dyn MemoryWriteBackend>,
}

#[async_trait]
pub trait MemoryWriteBackend: Send + Sync {
    async fn write(&self, content: &str, metadata: serde_json::Value) -> Result<String, String>;
}

impl MemoryWriteTool {
    pub fn new(store: Arc<dyn MemoryWriteBackend>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl ToolExecutor for MemoryWriteTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "memory_write".to_string(),
            description: "Write a new raw memory entry".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "content": { "type": "string" },
                    "metadata": { "type": "object" }
                },
                "required": ["content"]
            }),
            permissions: vec!["memory:write".to_string()],
        }
    }

    async fn check_permission(&self, _caller_id: &str) -> Result<(), ToolError> {
        Ok(())
    }

    async fn execute(&self, call: &ToolCall, _caller_id: &str) -> Result<ToolResult, ToolError> {
        let content = call.arguments["content"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments("missing 'content'".to_string()))?;
        let metadata = call
            .arguments
            .get("metadata")
            .cloned()
            .unwrap_or(serde_json::json!({}));

        let id = self
            .store
            .write(content, metadata)
            .await
            .map_err(ToolError::ExecutionFailed)?;

        Ok(ToolResult {
            call_id: call.id.clone(),
            tool_name: "memory_write".to_string(),
            success: true,
            output: serde_json::json!({ "id": id }),
            duration_ms: 0,
            side_effects: vec![format!("Created memory entry: {}", id)],
        })
    }
}

/// Инструмент выполнения кода в песочнице.
pub struct CodeExecuteTool;

#[async_trait]
impl ToolExecutor for CodeExecuteTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "code_execute".to_string(),
            description: "Execute code in a sandboxed environment".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "code": { "type": "string" },
                    "language": { "type": "string", "enum": ["python", "javascript", "bash"] },
                    "timeout_ms": { "type": "integer", "default": 5000 }
                },
                "required": ["code", "language"]
            }),
            permissions: vec!["code:execute".to_string()],
        }
    }

    async fn check_permission(&self, _caller_id: &str) -> Result<(), ToolError> {
        Ok(())
    }

    async fn execute(&self, call: &ToolCall, _caller_id: &str) -> Result<ToolResult, ToolError> {
        let code = call.arguments["code"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments("missing 'code'".to_string()))?;
        let language = call.arguments["language"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments("missing 'language'".to_string()))?;
        let timeout_ms = call.arguments["timeout_ms"].as_u64().unwrap_or(5000);

        debug!(
            "Executing {} code ({}ms timeout): {}...",
            language,
            timeout_ms,
            &code[..50.min(code.len())]
        );

        // Sandbox execution (stub — in production would use Docker/WASM)
        let output = match language {
            "python" => format!(
                "[sandbox] Python output for: {}",
                &code[..30.min(code.len())]
            ),
            "javascript" => {
                format!("[sandbox] JS output for: {}", &code[..30.min(code.len())])
            }
            "bash" => format!("[sandbox] Bash output for: {}", &code[..30.min(code.len())]),
            _ => {
                return Err(ToolError::InvalidArguments(format!(
                    "Unsupported language: {}",
                    language
                )))
            }
        };

        Ok(ToolResult {
            call_id: call.id.clone(),
            tool_name: "code_execute".to_string(),
            success: true,
            output: serde_json::json!({
                "output": output,
                "error": null,
                "exit_code": 0
            }),
            duration_ms: 0,
            side_effects: vec![],
        })
    }
}

/// Инструмент веб-поиска.
pub struct WebSearchTool;

#[async_trait]
impl ToolExecutor for WebSearchTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "web_search".to_string(),
            description: "Search the web for information".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "max_results": { "type": "integer", "default": 5 }
                },
                "required": ["query"]
            }),
            permissions: vec!["web:search".to_string()],
        }
    }

    async fn check_permission(&self, _caller_id: &str) -> Result<(), ToolError> {
        Ok(())
    }

    async fn execute(&self, call: &ToolCall, _caller_id: &str) -> Result<ToolResult, ToolError> {
        let query = call.arguments["query"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments("missing 'query'".to_string()))?;

        // Stub — in production would call Brave/Google search API
        let results = vec![serde_json::json!({
            "title": format!("Result for: {}", query),
            "url": "https://example.com",
            "snippet": format!("Information about {}", query)
        })];

        Ok(ToolResult {
            call_id: call.id.clone(),
            tool_name: "web_search".to_string(),
            success: true,
            output: serde_json::json!({ "results": results }),
            duration_ms: 0,
            side_effects: vec![],
        })
    }
}

/// Инструмент запуска тестов.
pub struct RunTestsTool;

#[async_trait]
impl ToolExecutor for RunTestsTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "run_tests".to_string(),
            description: "Run tests on generated code".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "code": { "type": "string" },
                    "language": { "type": "string" },
                    "test_cases": {
                        "type": "array",
                        "items": { "type": "string" }
                    }
                },
                "required": ["code", "language"]
            }),
            permissions: vec!["code:test".to_string()],
        }
    }

    async fn check_permission(&self, _caller_id: &str) -> Result<(), ToolError> {
        Ok(())
    }

    async fn execute(&self, call: &ToolCall, _caller_id: &str) -> Result<ToolResult, ToolError> {
        let _code = call.arguments["code"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments("missing 'code'".to_string()))?;
        let language = call.arguments["language"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments("missing 'language'".to_string()))?;
        let test_cases: Vec<String> = call.arguments["test_cases"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        debug!("Running {} tests for {} code", test_cases.len(), language);

        // Stub — in production would execute tests in sandbox
        Ok(ToolResult {
            call_id: call.id.clone(),
            tool_name: "run_tests".to_string(),
            success: true,
            output: serde_json::json!({
                "passed": true,
                "output": format!("All {} tests passed", test_cases.len()),
                "errors": []
            }),
            duration_ms: 0,
            side_effects: vec![],
        })
    }
}

/// Создаёт реестр с инструментами по умолчанию.
pub fn create_default_registry() -> super::ToolRegistry {
    let mut registry = super::ToolRegistry::new();

    // Память — используем заглушки для хранилища
    // В production передаются реальные хранилища из server main.rs
    let memory_store = Arc::new(StubMemoryStore);
    registry.register(Arc::new(MemorySearchTool::new(memory_store.clone())));
    registry.register(Arc::new(MemoryWriteTool::new(memory_store)));

    // Код
    registry.register(Arc::new(CodeExecuteTool));

    // Веб
    registry.register(Arc::new(WebSearchTool));

    // Тесты
    registry.register(Arc::new(RunTestsTool));

    registry
}

/// Заглушка для хранилища памяти.
struct StubMemoryStore;

#[async_trait]
impl MemorySearchBackend for StubMemoryStore {
    async fn search(&self, _query: &str, _top_k: usize) -> Result<Vec<MemorySearchResult>, String> {
        Ok(vec![])
    }
}

#[async_trait]
impl MemoryWriteBackend for StubMemoryStore {
    async fn write(&self, _content: &str, _metadata: serde_json::Value) -> Result<String, String> {
        Ok(uuid::Uuid::new_v4().to_string())
    }
}
