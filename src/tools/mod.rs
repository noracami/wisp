pub mod search;
pub mod weather;

use std::collections::HashMap;
use async_trait::async_trait;
use serde_json::Value;

use crate::error::AppError;

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> Value;
    async fn execute(&self, input: Value) -> Result<String, AppError>;
}

pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn tool_definitions(&self) -> Vec<Value> {
        self.tools
            .values()
            .map(|t| {
                serde_json::json!({
                    "name": t.name(),
                    "description": t.description(),
                    "input_schema": t.parameters(),
                })
            })
            .collect()
    }

    pub async fn execute(&self, name: &str, input: Value) -> Result<String, AppError> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| AppError::Internal(format!("Unknown tool: {name}")))?;
        tool.execute(input).await
    }
}
