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

#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug)]
pub enum LlmResponse {
    Text {
        text: String,
        model: String,
        usage: Usage,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
        model: String,
        usage: Usage,
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
    model: String,
    #[serde(default)]
    usage: ClaudeUsage,
}

#[derive(Deserialize, Default)]
struct ClaudeUsage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
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
        let claude_messages: Vec<ClaudeMessage> = messages
            .iter()
            .map(|m| ClaudeMessage {
                role: m.role.clone(),
                content: ClaudeContent::Text(m.content.clone()),
            })
            .collect();

        self.send_request(claude_messages, system_prompt, tools).await
    }

    /// Send accumulated tool call results back to Claude.
    /// Appends all tool_use/tool_result pairs after the conversation history.
    pub async fn chat_with_tool_results(
        &self,
        history: &[ChatMessage],
        tool_exchanges: &[(String, String, Value, String)], // (id, name, input, result)
        system_prompt: Option<&str>,
        tools: Option<&Vec<Value>>,
    ) -> Result<LlmResponse, AppError> {
        let mut claude_messages: Vec<ClaudeMessage> = history
            .iter()
            .map(|m| ClaudeMessage {
                role: m.role.clone(),
                content: ClaudeContent::Text(m.content.clone()),
            })
            .collect();

        // Append each tool exchange pair
        for (id, name, input, result) in tool_exchanges {
            claude_messages.push(ClaudeMessage {
                role: "assistant".to_string(),
                content: ClaudeContent::Blocks(vec![serde_json::json!({
                    "type": "tool_use",
                    "id": id,
                    "name": name,
                    "input": input,
                })]),
            });
            claude_messages.push(ClaudeMessage {
                role: "user".to_string(),
                content: ClaudeContent::Blocks(vec![serde_json::json!({
                    "type": "tool_result",
                    "tool_use_id": id,
                    "content": result,
                })]),
            });
        }

        self.send_request(claude_messages, system_prompt, tools).await
    }

    async fn send_request(
        &self,
        messages: Vec<ClaudeMessage>,
        system_prompt: Option<&str>,
        tools: Option<&Vec<Value>>,
    ) -> Result<LlmResponse, AppError> {
        let request = ClaudeRequest {
            model: "claude-haiku-4-5-20241022".to_string(),
            max_tokens: 1024,
            system: system_prompt.map(|s| s.to_string()),
            messages,
            tools: tools.cloned(),
        };

        let http_resp = self
            .http
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await?;

        let status = http_resp.status();
        if !status.is_success() {
            let body = http_resp.text().await.unwrap_or_default();
            tracing::error!("Claude API error ({}): {}", status, body);
            return Err(AppError::Internal(format!("Claude API error ({}): {}", status, body)));
        }

        let resp: ClaudeResponse = http_resp.json().await?;

        let usage = Usage {
            input_tokens: resp.usage.input_tokens,
            output_tokens: resp.usage.output_tokens,
        };
        let model = resp.model;

        // Check for tool_use first
        for block in &resp.content {
            if let ContentBlock::ToolUse { id, name, input } = block {
                return Ok(LlmResponse::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                    model,
                    usage,
                });
            }
        }

        // Fall back to text
        for block in resp.content {
            if let ContentBlock::Text { text } = block {
                return Ok(LlmResponse::Text { text, model, usage });
            }
        }

        Err(AppError::Internal("Empty response from Claude".to_string()))
    }
}
