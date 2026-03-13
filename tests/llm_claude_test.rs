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
    let messages = vec![wisp::llm::ChatMessage {
        role: "user".to_string(),
        content: "Hi".to_string(),
    }];

    let response = client.chat(&messages, None).await.unwrap();
    assert_eq!(response, "Hello! How can I help?");
}
