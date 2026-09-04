use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SdkError {
    #[error("Tool not found: {0}")]
    ToolNotFound(String),
    #[error("Invalid arguments: {0}")]
    InvalidArguments(String),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub arguments: serde_json::Value,
    pub result: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub id: String,
    pub name: String,
    pub output: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub struct ToolRegistry {
    tools: HashMap<String, ToolDefinition>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: ToolDefinition) {
        self.tools.insert(tool.name.clone(), tool);
    }

    pub fn get(&self, name: &str) -> Option<&ToolDefinition> {
        self.tools.get(name)
    }

    pub fn list(&self) -> Vec<&ToolDefinition> {
        self.tools.values().collect()
    }

    pub fn from_json(json: &str) -> Result<Self, SdkError> {
        let schema: serde_json::Value =
            serde_json::from_str(json).map_err(|e| SdkError::InvalidArguments(e.to_string()))?;

        let mut registry = Self::new();

        if let Some(tools) = schema.get("tools").and_then(|t| t.as_object()) {
            for (name, def) in tools {
                let tool = ToolDefinition {
                    name: name.clone(),
                    description: def["description"].as_str().unwrap_or("").to_string(),
                    arguments: def["arguments"].clone(),
                    result: def["result"].clone(),
                };
                registry.register(tool);
            }
        }

        Ok(registry)
    }
}

pub fn default_tool_schema() -> &'static str {
    include_str!("../schemas/tools.json")
}
