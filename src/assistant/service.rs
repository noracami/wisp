use std::sync::Arc;
use serde_json::Value;

use crate::db::memory::Memory;
use crate::db::users::UserService;
use crate::error::AppError;
use crate::llm::claude::{ClaudeClient, LlmResponse, Usage};
use crate::platform::{ChatMessage, ChatRequest, ChatResponse};
use crate::tools::ToolRegistry;

use crate::platform::Platform;

const MAX_TOOL_ITERATIONS: usize = 10;

fn system_prompt_for(platform: Platform) -> &'static str {
    match platform {
        Platform::Line => "\
你是 Wisp，一個生活聊天小幫手。

## 你的角色
- 幫助使用者解決食、衣、住、行、育、樂等日常生活問題
- 親切、實用、簡潔

## 語言
- 使用正體中文回答
- 用台灣習慣的用語和說法

## 回覆風格
- 簡潔扼要，不要長篇大論
- 實用導向，給出可以直接行動的建議
- 語氣親切自然，像朋友聊天一樣",

        Platform::Discord => "\
You are Wisp, a helpful AI assistant. Keep responses concise.

## 語言
- 優先使用正體中文回應
- 若使用者使用英文提問，則用英文回應",
    }
}

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
        let system_prompt = system_prompt_for(request.platform);
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
            .chat(&history, Some(system_prompt), tools_param)
            .await?;

        // Tool call loop — accumulate messages for multi-turn context
        let mut tool_exchanges: Vec<(String, String, Value, String)> = Vec::new();
        let mut used_tools: Vec<String> = Vec::new();
        let mut total_usage = Usage::default();
        let mut iterations = 0;

        // Accumulate usage from initial call
        match &response {
            LlmResponse::ToolUse { usage, .. } | LlmResponse::Text { usage, .. } => {
                total_usage.input_tokens += usage.input_tokens;
                total_usage.output_tokens += usage.output_tokens;
            }
        }

        while let LlmResponse::ToolUse { id, name, input, .. } = &response {
            iterations += 1;
            if iterations > MAX_TOOL_ITERATIONS {
                let text = "Sorry, I encountered too many tool calls. Please try again.".to_string();
                self.memory
                    .store_message(conv_id, "assistant", &text, None)
                    .await
                    .map_err(AppError::Database)?;
                return Ok(ChatResponse { text });
            }

            used_tools.push(name.clone());

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
                    Some(system_prompt),
                    tools_param,
                )
                .await?;

            // Accumulate usage from this call
            match &response {
                LlmResponse::ToolUse { usage, .. } | LlmResponse::Text { usage, .. } => {
                    total_usage.input_tokens += usage.input_tokens;
                    total_usage.output_tokens += usage.output_tokens;
                }
            }
        }

        let (text, model) = match response {
            LlmResponse::Text { text, model, .. } => (text, model),
            _ => unreachable!(),
        };

        // Build footer
        let total_tokens = total_usage.input_tokens + total_usage.output_tokens;
        let footer = if used_tools.is_empty() {
            format!("\n-# {model} · {total_tokens} tokens")
        } else {
            let tools_str = used_tools.join(", ");
            format!("\n-# {model} · {total_tokens} tokens · {tools_str}")
        };
        let text_with_footer = format!("{text}{footer}");

        // Store assistant response (without footer)
        self.memory
            .store_message(conv_id, "assistant", &text, None)
            .await
            .map_err(AppError::Database)?;

        Ok(ChatResponse { text: text_with_footer })
    }
}
