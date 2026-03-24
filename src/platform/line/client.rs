use reqwest::Client;
use serde_json::json;

use crate::error::AppError;

pub struct LineClient {
    channel_access_token: String,
    base_url: String,
    http: Client,
}

impl LineClient {
    pub fn new(channel_access_token: &str, base_url: &str) -> Self {
        Self {
            channel_access_token: channel_access_token.to_string(),
            base_url: base_url.to_string(),
            http: Client::new(),
        }
    }

    pub fn with_default_url(channel_access_token: &str) -> Self {
        Self::new(channel_access_token, "https://api.line.me")
    }

    pub async fn reply(&self, reply_token: &str, text: &str) -> Result<(), AppError> {
        self.http
            .post(format!("{}/v2/bot/message/reply", self.base_url))
            .header("Authorization", format!("Bearer {}", self.channel_access_token))
            .header("Content-Type", "application/json")
            .json(&json!({
                "replyToken": reply_token,
                "messages": [{"type": "text", "text": text}]
            }))
            .send()
            .await?
            .error_for_status()
            .map_err(AppError::Request)?;
        Ok(())
    }

    pub async fn push(&self, user_id: &str, text: &str) -> Result<(), AppError> {
        self.http
            .post(format!("{}/v2/bot/message/push", self.base_url))
            .header("Authorization", format!("Bearer {}", self.channel_access_token))
            .header("Content-Type", "application/json")
            .json(&json!({
                "to": user_id,
                "messages": [{"type": "text", "text": text}]
            }))
            .send()
            .await?
            .error_for_status()
            .map_err(AppError::Request)?;
        Ok(())
    }
}
