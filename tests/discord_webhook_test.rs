use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path, body_json};
use serde_json::json;

#[tokio::test]
async fn send_webhook_message_posts_correct_payload() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_json(json!({ "content": "Hello from Wisp!" })))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = wisp::platform::discord::webhook::WebhookClient::new(&mock_server.uri());
    client.send_message("Hello from Wisp!").await.unwrap();
}

#[tokio::test]
async fn send_webhook_embed_posts_correct_payload() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(body_json(json!({
            "embeds": [{
                "title": "Weather Report",
                "description": "Today: Sunny, 25°C",
                "color": 43775
            }]
        })))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = wisp::platform::discord::webhook::WebhookClient::new(&mock_server.uri());
    client
        .send_embed("Weather Report", "Today: Sunny, 25°C", 0x00AAFF)
        .await
        .unwrap();
}
