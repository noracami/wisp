use wiremock::{MockServer, Mock, matchers, ResponseTemplate};
use wisp::platform::line::client::LineClient;

#[tokio::test]
async fn reply_message_sends_correct_request() {
    let mock_server = MockServer::start().await;

    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v2/bot/message/reply"))
        .and(matchers::header("Authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = LineClient::new("test-token", &mock_server.uri());
    client.reply("reply-token-123", "Hello from Wisp!").await.unwrap();
}

#[tokio::test]
async fn push_message_sends_correct_request() {
    let mock_server = MockServer::start().await;

    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v2/bot/message/push"))
        .and(matchers::header("Authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = LineClient::new("test-token", &mock_server.uri());
    client.push("user-id-123", "Fallback message").await.unwrap();
}
