use serde_json::{json, Value};
use wisp::tools::{ToolRegistry, Tool};
use wisp::error::AppError;
use async_trait::async_trait;

struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
    fn name(&self) -> &str { "echo" }
    fn description(&self) -> &str { "Echoes input back" }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "text": { "type": "string" }
            },
            "required": ["text"]
        })
    }
    async fn execute(&self, input: Value) -> Result<String, AppError> {
        Ok(input["text"].as_str().unwrap_or("").to_string())
    }
}

#[test]
fn registry_tool_definitions() {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(EchoTool));

    let defs = registry.tool_definitions();
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0]["name"], "echo");
    assert_eq!(defs[0]["description"], "Echoes input back");
}

#[tokio::test]
async fn registry_execute_known_tool() {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(EchoTool));

    let result = registry.execute("echo", json!({"text": "hello"})).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "hello");
}

#[tokio::test]
async fn registry_execute_unknown_tool() {
    let registry = ToolRegistry::new();
    let result = registry.execute("nonexistent", json!({})).await;
    assert!(result.is_err());
}
