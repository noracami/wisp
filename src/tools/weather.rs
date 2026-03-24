use async_trait::async_trait;
use serde_json::{json, Value};

use crate::error::AppError;
use crate::weather::cwa::CwaClient;
use super::Tool;

pub struct WeatherTool {
    cwa_client: CwaClient,
}

impl WeatherTool {
    pub fn new(cwa_client: CwaClient) -> Self {
        Self { cwa_client }
    }
}

#[async_trait]
impl Tool for WeatherTool {
    fn name(&self) -> &str {
        "get_weather"
    }

    fn description(&self) -> &str {
        "取得台灣指定地區的天氣預報"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "location": {
                    "type": "string",
                    "description": "地區名稱，例如：臺北市"
                }
            },
            "required": ["location"]
        })
    }

    async fn execute(&self, input: Value) -> Result<String, AppError> {
        let location = input["location"].as_str().unwrap_or("臺北市");
        let forecast = self
            .cwa_client
            .fetch_forecast(location)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(forecast.to_embed_description())
    }
}
