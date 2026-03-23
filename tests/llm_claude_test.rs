use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn claude_chat_sends_correct_request_and_parses_response() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("x-api-key", "test-key"))
        .and(header("anthropic-version", "2023-06-01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "msg_123",
            "type": "message",
            "role": "assistant",
            "content": [{ "type": "text", "text": "Hello! How can I help?" }],
            "model": "claude-sonnet-4-20250514",
            "stop_reason": "end_turn",
            "usage": { "input_tokens": 10, "output_tokens": 20 }
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = wisp::llm::claude::ClaudeClient::new("test-key", &mock_server.uri());
    let messages = vec![wisp::platform::ChatMessage {
        role: "user".to_string(),
        content: "Hi".to_string(),
    }];

    let response = client.chat(&messages, None, None).await.unwrap();
    match response {
        wisp::llm::claude::LlmResponse::Text(t) => assert_eq!(t, "Hello! How can I help?"),
        _ => panic!("Expected Text response"),
    }
}

#[tokio::test]
async fn chat_with_tools_returns_tool_use() {
    let mock_server = wiremock::MockServer::start().await;

    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/v1/messages"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [
                {
                    "type": "tool_use",
                    "id": "toolu_123",
                    "name": "get_weather",
                    "input": {"location": "臺北市"}
                }
            ],
            "stop_reason": "tool_use"
        })))
        .mount(&mock_server)
        .await;

    let client = wisp::llm::claude::ClaudeClient::new("test-key", &mock_server.uri());
    let tools = vec![serde_json::json!({
        "name": "get_weather",
        "description": "Get weather forecast",
        "input_schema": {
            "type": "object",
            "properties": {
                "location": {"type": "string"}
            }
        }
    })];
    let messages = vec![wisp::platform::ChatMessage {
        role: "user".to_string(),
        content: "What's the weather in Taipei?".to_string(),
    }];

    let response = client.chat(&messages, None, Some(&tools)).await.unwrap();

    match response {
        wisp::llm::claude::LlmResponse::Text(t) => panic!("Expected ToolUse, got Text: {t}"),
        wisp::llm::claude::LlmResponse::ToolUse { id, name, input } => {
            assert_eq!(name, "get_weather");
            assert_eq!(id, "toolu_123");
            assert_eq!(input["location"], "臺北市");
        }
    }
}

#[tokio::test]
async fn chat_with_tools_returns_text() {
    let mock_server = wiremock::MockServer::start().await;

    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/v1/messages"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{"type": "text", "text": "The weather is sunny."}],
            "stop_reason": "end_turn"
        })))
        .mount(&mock_server)
        .await;

    let client = wisp::llm::claude::ClaudeClient::new("test-key", &mock_server.uri());
    let messages = vec![wisp::platform::ChatMessage {
        role: "user".to_string(),
        content: "hello".to_string(),
    }];

    let response = client.chat(&messages, None, Some(&vec![])).await.unwrap();

    match response {
        wisp::llm::claude::LlmResponse::Text(t) => assert_eq!(t, "The weather is sunny."),
        wisp::llm::claude::LlmResponse::ToolUse { .. } => panic!("Expected Text, got ToolUse"),
    }
}
