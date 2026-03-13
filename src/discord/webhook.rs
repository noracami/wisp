use reqwest::Client;
use serde_json::json;

use crate::error::AppError;

pub struct WebhookClient {
    url: String,
    http: Client,
}

impl WebhookClient {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            http: Client::new(),
        }
    }

    pub async fn send_message(&self, content: &str) -> Result<(), AppError> {
        self.http
            .post(&self.url)
            .json(&json!({ "content": content }))
            .send()
            .await?
            .error_for_status()
            .map_err(AppError::Request)?;
        Ok(())
    }

    pub async fn send_embed(
        &self,
        title: &str,
        description: &str,
        color: u32,
    ) -> Result<(), AppError> {
        self.http
            .post(&self.url)
            .json(&json!({
                "embeds": [{
                    "title": title,
                    "description": description,
                    "color": color
                }]
            }))
            .send()
            .await?
            .error_for_status()
            .map_err(AppError::Request)?;
        Ok(())
    }
}
