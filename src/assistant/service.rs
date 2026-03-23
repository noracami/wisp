use std::sync::Arc;
use serde_json::Value;

use crate::db::memory::Memory;
use crate::db::users::UserService;
use crate::error::AppError;
use crate::llm::claude::{ClaudeClient, LlmResponse};
use crate::platform::{ChatMessage, ChatRequest, ChatResponse};
use crate::tools::ToolRegistry;

const MAX_TOOL_ITERATIONS: usize = 10;
const SYSTEM_PROMPT: &str = "You are Wisp, a helpful AI assistant. Keep responses concise.";

pub struct Assistant {
    claude: Arc<ClaudeClient>,
    memory: Arc<Memory>,
    users: Arc<UserService>,
    tools: Arc<ToolRegistry>,
}

impl Assistant {
    pub fn new(
        claude: Arc<ClaudeClient>,
        memory: Arc<Memory>,
        users: Arc<UserService>,
        tools: Arc<ToolRegistry>,
    ) -> Self {
        Self {
            claude,
            memory,
            users,
            tools,
        }
    }

    pub async fn handle(&self, request: ChatRequest) -> Result<ChatResponse, AppError> {
        let platform_str = request.platform.as_str();

        // Get or create conversation
        let conv_id = self
            .memory
            .get_or_create_conversation(
                request.user_id,
                &request.channel_id,
                platform_str,
            )
            .await
            .map_err(AppError::Database)?;

        // Store user message
        self.memory
            .store_message(conv_id, "user", &request.message, None)
            .await
            .map_err(AppError::Database)?;

        // Load history
        let history = self
            .memory
            .load_recent_messages(conv_id, 20)
            .await
            .map_err(AppError::Database)?;

        // Build tool definitions
        let tool_defs = self.tools.tool_definitions();
        let tools_param = if tool_defs.is_empty() {
            None
        } else {
            Some(&tool_defs)
        };

        // Initial LLM call
        let mut response = self
            .claude
            .chat(&history, Some(SYSTEM_PROMPT), tools_param)
            .await?;

        // Tool call loop — accumulate messages for multi-turn context
        let mut tool_exchanges: Vec<(String, String, Value, String)> = Vec::new();
        let mut iterations = 0;

        while let LlmResponse::ToolUse { id, name, input } = &response {
            iterations += 1;
            if iterations > MAX_TOOL_ITERATIONS {
                let text = "Sorry, I encountered too many tool calls. Please try again.".to_string();
                self.memory
                    .store_message(conv_id, "assistant", &text, None)
                    .await
                    .map_err(AppError::Database)?;
                return Ok(ChatResponse { text });
            }

            // Execute tool
            let tool_result = match self.tools.execute(name, input.clone()).await {
                Ok(result) => result,
                Err(e) => format!("Tool error: {e}"),
            };

            tool_exchanges.push((id.clone(), name.clone(), input.clone(), tool_result));

            // Send full context (history + all accumulated tool exchanges) back to LLM
            response = self
                .claude
                .chat_with_tool_results(
                    &history,
                    &tool_exchanges,
                    Some(SYSTEM_PROMPT),
                    tools_param,
                )
                .await?;
        }

        let text = match response {
            LlmResponse::Text(t) => t,
            _ => unreachable!(),
        };

        // Store assistant response
        self.memory
            .store_message(conv_id, "assistant", &text, None)
            .await
            .map_err(AppError::Database)?;

        Ok(ChatResponse { text })
    }
}
