use async_trait::async_trait;
use chrono::{FixedOffset, Utc};
use serde_json::{json, Value};

use crate::error::AppError;
use super::Tool;

pub struct TimeTool;

impl TimeTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for TimeTool {
    fn name(&self) -> &str {
        "get_current_time"
    }

    fn description(&self) -> &str {
        "取得目前的日期與時間（UTC+8 台灣時間）"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn execute(&self, _input: Value) -> Result<String, AppError> {
        let offset = FixedOffset::east_opt(8 * 3600).unwrap();
        let now = Utc::now().with_timezone(&offset);
        let weekday = match now.format("%A").to_string().as_str() {
            "Monday" => "週一",
            "Tuesday" => "週二",
            "Wednesday" => "週三",
            "Thursday" => "週四",
            "Friday" => "週五",
            "Saturday" => "週六",
            "Sunday" => "週日",
            _ => "",
        };
        Ok(format!(
            "{}（{}）{} UTC+8",
            now.format("%Y-%m-%d"),
            weekday,
            now.format("%H:%M:%S")
        ))
    }
}
