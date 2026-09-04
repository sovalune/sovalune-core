//! Модуль вызова инструментов (tool calling / function calling).
//!
//! Реализует протокол вызова инструментов для моделей, поддерживающих function calling.
//! Модель может запросить вызов инструмента, а система выполнит его и вернёт результат.
//!
//! # Протокол
//!
//! 1. Модель генерирует `tool_call` с именем и аргументами
//! 2. Система выполняет инструмент
//! 3. Результат добавляется в контекст как `tool_result`
//! 4. Модель продолжает генерацию с учётом результата

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, warn};

/// Ошибка выполнения инструмента.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Tool not found: {0}")]
    NotFound(String),

    #[error("Invalid arguments: {0}")]
    InvalidArguments(String),

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Timeout after {0}ms")]
    Timeout(u64),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Описание инструмента для модели.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Уникальное имя инструмента.
    pub name: String,
    /// Описание для модели.
    pub description: String,
    /// JSON Schema параметров.
    pub parameters: serde_json::Value,
    /// Необходимые разрешения.
    #[serde(default)]
    pub permissions: Vec<String>,
}

/// Вызов инструмента от модели.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// ID вызова.
    pub id: String,
    /// Имя инструмента.
    pub name: String,
    /// Аргументы (JSON).
    pub arguments: serde_json::Value,
}

/// Результат выполнения инструмента.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// ID вызова (ссылка на ToolCall.id).
    pub call_id: String,
    /// Имя инструмента.
    pub tool_name: String,
    /// Успешно ли выполнен.
    pub success: bool,
    /// Результат (текст или JSON).
    pub output: serde_json::Value,
    /// Время выполнения (мс).
    pub duration_ms: u64,
    /// Побочные эффекты (изменения файлов, etc).
    #[serde(default)]
    pub side_effects: Vec<String>,
}

/// Трейт для выполнения инструментов.
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Возвращает определение инструмента.
    fn definition(&self) -> ToolDefinition;

    /// Проверяет, имеет ли вызывающий право на выполнение.
    async fn check_permission(&self, caller_id: &str) -> Result<(), ToolError>;

    /// Выполняет инструмент.
    async fn execute(
        &self,
        call: &ToolCall,
        caller_id: &str,
    ) -> Result<ToolResult, ToolError>;
}

/// Реестр инструментов — управляет доступными инструментами.
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn ToolExecutor>>,
}

impl ToolRegistry {
    /// Создаёт пустой реестр.
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Регистрирует инструмент.
    pub fn register(&mut self, executor: Arc<dyn ToolExecutor>) {
        let def = executor.definition();
        debug!("Registered tool: {}", def.name);
        self.tools.insert(def.name.clone(), executor);
    }

    /// Возвращает определение инструмента по имени.
    pub fn get_definition(&self, name: &str) -> Option<ToolDefinition> {
        self.tools.get(name).map(|e| e.definition())
    }

    /// Возвращает все определения инструментов.
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|e| e.definition()).collect()
    }

    /// Возвращает JSON Schema всех инструментов для OpenAI function calling.
    pub fn openai_tools_schema(&self) -> Vec<serde_json::Value> {
        self.tools
            .values()
            .map(|e| {
                let def = e.definition();
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": def.name,
                        "description": def.description,
                        "parameters": def.parameters
                    }
                })
            })
            .collect()
    }

    /// Выполняет вызов инструмента.
    pub async fn execute(
        &self,
        call: &ToolCall,
        caller_id: &str,
    ) -> Result<ToolResult, ToolError> {
        let executor = self
            .tools
            .get(&call.name)
            .ok_or_else(|| ToolError::NotFound(call.name.clone()))?;

        // Проверяем разрешения
        executor.check_permission(caller_id).await?;

        // Выполняем
        let start = std::time::Instant::now();
        let result = executor.execute(call, caller_id).await?;
        let duration_ms = start.elapsed().as_millis() as u64;

        debug!(
            "Tool executed: {} duration={}ms success={}",
            call.name, duration_ms, result.success
        );

        Ok(ToolResult {
            duration_ms,
            ..result
        })
    }

    /// Возвращает количество зарегистрированных инструментов.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Проверяет, пуст ли реестр.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Менеджер вызовов инструментов — координирует вызовы между моделью и реестром.
pub struct ToolCallManager {
    registry: Arc<ToolRegistry>,
    /// История вызовов для текущей сессии.
    pending_calls: Vec<ToolCall>,
}

impl ToolCallManager {
    /// Создаёт новый менеджер.
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self {
            registry,
            pending_calls: Vec::new(),
        }
    }

    /// Добавляет вызов от модели в очередь.
    pub fn enqueue(&mut self, call: ToolCall) {
        debug!("Enqueued tool call: {} (id={})", call.name, call.id);
        self.pending_calls.push(call);
    }

    /// Добавляет несколько вызовов.
    pub fn enqueue_batch(&mut self, calls: Vec<ToolCall>) {
        for call in calls {
            self.enqueue(call);
        }
    }

    /// Берёт следующий вызов из очереди.
    pub fn next_call(&mut self) -> Option<ToolCall> {
        self.pending_calls.pop()
    }

    /// Проверяет, есть ли ожидающие вызовы.
    pub fn has_pending(&self) -> bool {
        !self.pending_calls.is_empty()
    }

    /// Количество ожидающих вызовов.
    pub fn pending_count(&self) -> usize {
        self.pending_calls.len()
    }

    /// Выполняет все ожидающие вызовы.
    pub async fn execute_all(
        &self,
        caller_id: &str,
    ) -> Vec<ToolResult> {
        let mut results = Vec::new();
        let mut calls = self.pending_calls.clone();

        for call in &calls {
            match self.registry.execute(call, caller_id).await {
                Ok(result) => results.push(result),
                Err(e) => {
                    warn!("Tool call failed: {} - {}", call.name, e);
                    results.push(ToolResult {
                        call_id: call.id.clone(),
                        tool_name: call.name.clone(),
                        success: false,
                        output: serde_json::json!({
                            "error": e.to_string()
                        }),
                        duration_ms: 0,
                        side_effects: vec![],
                    });
                }
            }
        }

        results
    }

    /// Очищает очередь вызовов.
    pub fn clear(&mut self) {
        self.pending_calls.clear();
    }
}

/// Парсер вызовов инструментов из ответа модели.
pub struct ToolCallParser;

impl ToolCallParser {
    /// Парсит tool calls из ответа OpenAI API.
    pub fn parse_openai_tool_calls(
        tool_calls: &[serde_json::Value],
    ) -> Vec<ToolCall> {
        tool_calls
            .iter()
            .filter_map(|tc| {
                let id = tc["id"].as_str()?.to_string();
                let name = tc["function"]["name"].as_str()?.to_string();
                let arguments_str = tc["function"]["arguments"].as_str()?;
                let arguments: serde_json::Value =
                    serde_json::from_str(arguments_str).ok()?;

                Some(ToolCall {
                    id,
                    name,
                    arguments,
                })
            })
            .collect()
    }

    /// Парсит tool calls из текста (для моделей без native function calling).
    ///
    /// Ищет паттерн: ```json\n{"name": "...", "arguments": {...}}\n```
    pub fn parse_text_tool_calls(text: &str) -> Vec<ToolCall> {
        let mut calls = Vec::new();

        // Ищем JSON блоки
        let mut in_code_block = false;
        let mut current_block = String::new();

        for line in text.lines() {
            if line.trim().starts_with("```json") || line.trim().starts_with("```") {
                if in_code_block {
                    // Конец блока — парсим
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&current_block) {
                        if let Some(name) = value["name"].as_str() {
                            let id = format!("call_{}", uuid::Uuid::new_v4().to_string().replace("-", "")[..16].to_string());
                            calls.push(ToolCall {
                                id,
                                name: name.to_string(),
                                arguments: value.get("arguments").cloned().unwrap_or(serde_json::json!({})),
                            });
                        }
                    }
                    current_block.clear();
                    in_code_block = false;
                } else {
                    in_code_block = true;
                }
            } else if in_code_block {
                current_block.push_str(line);
                current_block.push('\n');
            }
        }

        calls
    }
}
