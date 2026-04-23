use serde_json::json;
use twilight_model::channel::message::MessageFlags;
use twilight_model::http::interaction::InteractionResponseType;
use wisp::tpp_poc::{handle_ping, handle_setup, PocState};

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
