use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::platform::ChatMessage;
use crate::error::AppError;

pub struct ClaudeClient {
    api_key: String,
    base_url: String,
    http: Client,
}

#[derive(Debug)]
pub enum LlmResponse {
    Text(String),
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
}

#[derive(Serialize)]
struct ClaudeRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<ClaudeMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<Value>>,
}

#[derive(Serialize)]
struct ClaudeMessage {
    role: String,
    content: ClaudeContent,
}

#[derive(Serialize)]
#[serde(untagged)]
enum ClaudeContent {
    Text(String),
    Blocks(Vec<Value>),
}

#[derive(Deserialize)]
struct ClaudeResponse {
    content: Vec<ContentBlock>,
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
}

impl ClaudeClient {
    pub fn new(api_key: &str, base_url: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            base_url: base_url.to_string(),
            http: Client::new(),
        }
    }

    pub fn with_default_url(api_key: &str) -> Self {
        Self::new(api_key, "https://api.anthropic.com")
    }

    pub async fn chat(
        &self,
        messages: &[ChatMessage],
        system_prompt: Option<&str>,
        tools: Option<&Vec<Value>>,
    ) -> Result<LlmResponse, AppError> {
        let request = ClaudeRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 1024,
            system: system_prompt.map(|s| s.to_string()),
            messages: messages
                .iter()
                .map(|m| ClaudeMessage {
                    role: m.role.clone(),
                    content: ClaudeContent::Text(m.content.clone()),
                })
                .collect(),
            tools: tools.cloned(),
        };

        let resp: ClaudeResponse = self
            .http
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await?
            .error_for_status()
            .map_err(AppError::Request)?
            .json()
            .await?;

        // Check for tool_use first
        for block in &resp.content {
            if let ContentBlock::ToolUse { id, name, input } = block {
                return Ok(LlmResponse::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                });
            }
        }

        // Fall back to text
        for block in resp.content {
            if let ContentBlock::Text { text } = block {
                return Ok(LlmResponse::Text(text));
            }
        }

        Err(AppError::Internal("Empty response from Claude".to_string()))
    }

    /// Send tool result back to Claude for continued processing.
    pub async fn chat_with_tool_result(
        &self,
        messages: &[ChatMessage],
        tool_use_id: &str,
        tool_use_name: &str,
        tool_use_input: &Value,
        tool_result: &str,
        system_prompt: Option<&str>,
        tools: Option<&Vec<Value>>,
    ) -> Result<LlmResponse, AppError> {
        let mut claude_messages: Vec<ClaudeMessage> = messages
            .iter()
            .map(|m| ClaudeMessage {
                role: m.role.clone(),
                content: ClaudeContent::Text(m.content.clone()),
            })
            .collect();

        // Add assistant's tool_use message
        claude_messages.push(ClaudeMessage {
            role: "assistant".to_string(),
            content: ClaudeContent::Blocks(vec![serde_json::json!({
                "type": "tool_use",
                "id": tool_use_id,
                "name": tool_use_name,
                "input": tool_use_input,
            })]),
        });

        // Add user's tool_result message
        claude_messages.push(ClaudeMessage {
            role: "user".to_string(),
            content: ClaudeContent::Blocks(vec![serde_json::json!({
                "type": "tool_result",
                "tool_use_id": tool_use_id,
                "content": tool_result,
            })]),
        });

        let request = ClaudeRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 1024,
            system: system_prompt.map(|s| s.to_string()),
            messages: claude_messages,
            tools: tools.cloned(),
        };

        let resp: ClaudeResponse = self
            .http
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await?
            .error_for_status()
            .map_err(AppError::Request)?
            .json()
            .await?;

        for block in &resp.content {
            if let ContentBlock::ToolUse { id, name, input } = block {
                return Ok(LlmResponse::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                });
            }
        }

        for block in resp.content {
            if let ContentBlock::Text { text } = block {
                return Ok(LlmResponse::Text(text));
            }
        }

        Err(AppError::Internal("Empty response from Claude".to_string()))
    }
}
