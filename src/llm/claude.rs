use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::ChatMessage;
use crate::error::AppError;

pub struct ClaudeClient {
    api_key: String,
    base_url: String,
    http: Client,
}

#[derive(Serialize)]
struct ClaudeRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<ClaudeMessage>,
}

#[derive(Serialize)]
struct ClaudeMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ClaudeResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: String,
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
    ) -> Result<String, AppError> {
        let request = ClaudeRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 1024,
            system: system_prompt.map(|s| s.to_string()),
            messages: messages
                .iter()
                .map(|m| ClaudeMessage {
                    role: m.role.clone(),
                    content: m.content.clone(),
                })
                .collect(),
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

        resp.content
            .into_iter()
            .next()
            .map(|b| b.text)
            .ok_or_else(|| AppError::Internal("Empty response from Claude".to_string()))
    }
}
