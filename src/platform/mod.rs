pub mod discord;
pub mod line;

use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Discord,
    Line,
}

impl Platform {
    pub fn as_str(&self) -> &'static str {
        match self {
            Platform::Discord => "discord",
            Platform::Line => "line",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "discord" => Some(Platform::Discord),
            "line" => Some(Platform::Line),
            _ => None,
        }
    }
}

/// Shared message type used across LLM, memory, and assistant layers.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Unified request from Platform Layer to Assistant
pub struct ChatRequest {
    pub user_id: Uuid,
    pub channel_id: String,
    pub platform: Platform,
    pub message: String,
}

/// Assistant response back to Platform Layer
pub struct ChatResponse {
    pub text: String,
}
