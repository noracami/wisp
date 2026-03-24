use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::AppError;
use super::Tool;

pub struct SearchTool {
    api_key: String,
    engine_id: String,
    http: Client,
}

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    items: Vec<SearchItem>,
}

#[derive(Deserialize)]
struct SearchItem {
    title: String,
    link: String,
    snippet: String,
}

impl SearchTool {
    pub fn new(api_key: &str, engine_id: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            engine_id: engine_id.to_string(),
            http: Client::new(),
        }
    }
}

#[async_trait]
impl Tool for SearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "搜尋網路上的資訊，適合查詢最新消息、餐廳推薦、生活資訊等"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "搜尋關鍵字"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, input: Value) -> Result<String, AppError> {
        let query = input["query"]
            .as_str()
            .ok_or_else(|| AppError::Internal("Missing query parameter".to_string()))?;

        let resp: SearchResponse = self
            .http
            .get("https://www.googleapis.com/customsearch/v1")
            .query(&[
                ("key", self.api_key.as_str()),
                ("cx", self.engine_id.as_str()),
                ("q", query),
                ("num", "5"),
            ])
            .send()
            .await?
            .error_for_status()
            .map_err(AppError::Request)?
            .json()
            .await?;

        if resp.items.is_empty() {
            return Ok("沒有找到相關結果".to_string());
        }

        let results: Vec<String> = resp
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                format!("{}. {}\n   {}\n   {}", i + 1, item.title, item.snippet, item.link)
            })
            .collect();

        Ok(results.join("\n\n"))
    }
}
