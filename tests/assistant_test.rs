use std::sync::Arc;
use serde_json::json;
use wisp::assistant::service::Assistant;
use wisp::platform::{Platform, ChatRequest};
use uuid::Uuid;

#[tokio::test]
#[ignore] // Requires database + wiremock
async fn assistant_handles_simple_chat() {
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://wisp:wisp@localhost:5432/wisp".to_string());
    let pool = wisp::db::create_pool(&db_url).await.unwrap();
    wisp::db::run_migrations(&pool).await.unwrap();

    let mock_server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/v1/messages"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "content": [{"type": "text", "text": "Hi there!"}],
            "stop_reason": "end_turn"
        })))
        .mount(&mock_server)
        .await;

    let claude = Arc::new(wisp::llm::claude::ClaudeClient::new("test-key", &mock_server.uri()));
    let memory = Arc::new(wisp::db::memory::Memory::new(pool.clone()));
    let users = Arc::new(wisp::db::users::UserService::new(pool));
    let registry = wisp::tools::ToolRegistry::new();

    let assistant = Assistant::new(claude, memory, users, Arc::new(registry));

    let user_id = Uuid::new_v4();
    let request = ChatRequest {
        user_id,
        channel_id: "test-channel".to_string(),
        platform: Platform::Discord,
        message: "Hello".to_string(),
    };

    let response = assistant.handle(request).await.unwrap();
    assert_eq!(response.text, "Hi there!");
}

#[tokio::test]
#[ignore] // Requires database + wiremock
async fn assistant_handles_tool_use_loop() {
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://wisp:wisp@localhost:5432/wisp".to_string());
    let pool = wisp::db::create_pool(&db_url).await.unwrap();
    wisp::db::run_migrations(&pool).await.unwrap();

    let mock_server = wiremock::MockServer::start().await;

    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/v1/messages"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "content": [{"type": "text", "text": "The weather is sunny in Taipei."}],
            "stop_reason": "end_turn"
        })))
        .mount(&mock_server)
        .await;

    let claude = Arc::new(wisp::llm::claude::ClaudeClient::new("test-key", &mock_server.uri()));
    let memory = Arc::new(wisp::db::memory::Memory::new(pool.clone()));
    let users = Arc::new(wisp::db::users::UserService::new(pool));
    let registry = wisp::tools::ToolRegistry::new();

    let assistant = Assistant::new(claude, memory, users, Arc::new(registry));

    let user_id = Uuid::new_v4();
    let request = ChatRequest {
        user_id,
        channel_id: "test-tool".to_string(),
        platform: Platform::Discord,
        message: "What's the weather?".to_string(),
    };

    let response = assistant.handle(request).await.unwrap();
    assert!(!response.text.is_empty());
}
