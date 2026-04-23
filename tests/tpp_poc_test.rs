use serde_json::json;
use twilight_model::channel::message::MessageFlags;
use twilight_model::http::interaction::InteractionResponseType;
use wisp::tpp_poc::{handle_click, handle_ping, handle_setup, PocState};
use wiremock::matchers::{body_partial_json, method};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn poc_state_starts_empty() {
    let state = PocState::new();
    let webhooks = state.webhooks.read().await;
    assert!(webhooks.is_empty());
}

#[tokio::test]
async fn poc_state_insert_and_read() {
    let state = PocState::new();
    state
        .webhooks
        .write()
        .await
        .insert("user-1".to_string(), "https://webhook.example/x".to_string());

    let webhooks = state.webhooks.read().await;
    assert_eq!(
        webhooks.get("user-1"),
        Some(&"https://webhook.example/x".to_string())
    );
}

#[tokio::test]
async fn handle_setup_stores_url_from_guild_member() {
    let state = PocState::new();
    let interaction = json!({
        "type": 2,
        "data": {
            "name": "tpp-setup",
            "options": [{"name": "url", "type": 3, "value": "https://webhook.example/abc"}]
        },
        "member": {"user": {"id": "user-42"}}
    });

    let response = handle_setup(&state, &interaction).await;

    assert_eq!(response.kind, InteractionResponseType::ChannelMessageWithSource);
    let data = response.data.expect("response has data");
    assert_eq!(data.flags, Some(MessageFlags::EPHEMERAL));
    assert!(data.content.as_deref().unwrap_or("").contains("Registered"));

    let webhooks = state.webhooks.read().await;
    assert_eq!(
        webhooks.get("user-42"),
        Some(&"https://webhook.example/abc".to_string())
    );
}

#[tokio::test]
async fn handle_setup_stores_url_from_dm_user() {
    let state = PocState::new();
    let interaction = json!({
        "type": 2,
        "data": {
            "name": "tpp-setup",
            "options": [{"name": "url", "type": 3, "value": "https://webhook.example/dm"}]
        },
        "user": {"id": "user-99"}
    });

    let response = handle_setup(&state, &interaction).await;

    assert_eq!(response.kind, InteractionResponseType::ChannelMessageWithSource);
    let webhooks = state.webhooks.read().await;
    assert_eq!(
        webhooks.get("user-99"),
        Some(&"https://webhook.example/dm".to_string())
    );
}

#[tokio::test]
async fn handle_setup_missing_user_returns_error() {
    let state = PocState::new();
    let interaction = json!({
        "type": 2,
        "data": {
            "name": "tpp-setup",
            "options": [{"name": "url", "type": 3, "value": "https://x"}]
        }
    });

    let response = handle_setup(&state, &interaction).await;
    let data = response.data.expect("has data");
    assert!(data.content.as_deref().unwrap_or("").to_lowercase().contains("error"));
    assert!(state.webhooks.read().await.is_empty());
}

#[tokio::test]
async fn handle_ping_without_setup_returns_error() {
    let state = PocState::new();
    let interaction = json!({
        "type": 2,
        "data": {"name": "tpp-ping"},
        "user": {"id": "user-no-webhook"}
    });

    let response = handle_ping(&state, &interaction).await;

    assert_eq!(response.kind, InteractionResponseType::ChannelMessageWithSource);
    let data = response.data.expect("has data");
    assert_eq!(data.flags, Some(MessageFlags::EPHEMERAL));
    assert!(
        data.content
            .as_deref()
            .unwrap_or("")
            .contains("尚未登記")
    );
}

#[tokio::test]
async fn handle_ping_posts_button_message_to_webhook() {
    let mock = MockServer::start().await;

    Mock::given(method("POST"))
        .and(body_partial_json(json!({
            "components": [{"type": 1, "components": [{"type": 2, "custom_id": "tpp-poc-test"}]}]
        })))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&mock)
        .await;

    let webhook_url = format!("{}/webhook/abc/token", mock.uri());

    let state = PocState::new();
    state
        .webhooks
        .write()
        .await
        .insert("user-42".to_string(), webhook_url);

    let interaction = json!({
        "type": 2,
        "data": {"name": "tpp-ping"},
        "member": {"user": {"id": "user-42"}}
    });

    let response = handle_ping(&state, &interaction).await;

    assert_eq!(response.kind, InteractionResponseType::ChannelMessageWithSource);
    let data = response.data.expect("has data");
    assert_eq!(data.flags, Some(MessageFlags::EPHEMERAL));
    assert!(data.content.as_deref().unwrap_or("").contains("Sent"));
}

#[tokio::test]
async fn handle_ping_reports_webhook_error_status() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(400).set_body_string("bad"))
        .expect(1)
        .mount(&mock)
        .await;

    let webhook_url = format!("{}/webhook/bad/token", mock.uri());

    let state = PocState::new();
    state.webhooks.write().await.insert("u".to_string(), webhook_url);

    let interaction = json!({
        "type": 2,
        "data": {"name": "tpp-ping"},
        "user": {"id": "u"}
    });

    let response = handle_ping(&state, &interaction).await;
    let content = response.data.unwrap().content.unwrap();
    assert!(content.contains("400"), "content was: {content}");
}

#[tokio::test]
async fn handle_click_returns_deferred_update_message() {
    let interaction = json!({
        "type": 3,
        "data": {"custom_id": "tpp-poc-test"},
        "member": {"user": {"id": "u"}},
        "message": {"webhook_id": "1234567890"}
    });

    let response = handle_click(&interaction).await;

    assert_eq!(response.kind, InteractionResponseType::DeferredUpdateMessage);
    assert!(response.data.is_none());
}
