use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use tokio::sync::RwLock;
use twilight_model::channel::message::MessageFlags;
use twilight_model::http::interaction::InteractionResponseType;
use wisp::db::tpp_webhooks::{StoredWebhook, TppWebhookStore};
use wisp::platform::discord::oauth::TppOAuthConfig;
use wisp::tpp_poc::{handle_click, handle_ping, handle_setup};

fn fake_oauth_config() -> TppOAuthConfig {
    TppOAuthConfig {
        application_id: "12345".into(),
        client_secret: "cs".into(),
        redirect_uri: "https://wisp.example.com/discord/oauth/callback".into(),
        state_secret: "ss".into(),
    }
}

#[derive(Default)]
struct FakeStore {
    inner: RwLock<HashMap<String, StoredWebhook>>,
}

#[async_trait]
impl TppWebhookStore for FakeStore {
    async fn upsert(
        &self,
        user_id: &str,
        webhook_id: &str,
        webhook_token: &str,
        channel_id: &str,
        guild_id: Option<&str>,
        channel_name: Option<&str>,
    ) -> sqlx::Result<()> {
        self.inner.write().await.insert(
            user_id.to_string(),
            StoredWebhook {
                user_id: user_id.into(),
                webhook_id: webhook_id.into(),
                webhook_token: webhook_token.into(),
                channel_id: channel_id.into(),
                guild_id: guild_id.map(str::to_string),
                channel_name: channel_name.map(str::to_string),
            },
        );
        Ok(())
    }

    async fn find_by_user(&self, user_id: &str) -> sqlx::Result<Option<StoredWebhook>> {
        Ok(self.inner.read().await.get(user_id).cloned())
    }
}

#[tokio::test]
async fn setup_returns_authorize_url_from_guild_member() {
    let cfg = fake_oauth_config();
    let interaction = json!({
        "type": 2,
        "data": { "name": "tpp-setup" },
        "member": { "user": { "id": "329579602429214721" } }
    });

    let response = handle_setup(&cfg, &interaction).await;

    assert_eq!(response.kind, InteractionResponseType::ChannelMessageWithSource);
    let data = response.data.expect("response has data");
    assert_eq!(data.flags, Some(MessageFlags::EPHEMERAL));
    let content = data.content.expect("has content");
    assert!(content.contains("https://discord.com/api/oauth2/authorize"));
    assert!(content.contains("client_id=12345"));
    assert!(content.contains("scope=webhook.incoming"));
    assert!(content.contains("state="));
}

#[tokio::test]
async fn setup_returns_authorize_url_from_dm_user() {
    let cfg = fake_oauth_config();
    let interaction = json!({
        "type": 2,
        "data": { "name": "tpp-setup" },
        "user": { "id": "u-dm" }
    });

    let response = handle_setup(&cfg, &interaction).await;
    let content = response.data.unwrap().content.unwrap();
    assert!(content.contains("https://discord.com/api/oauth2/authorize"));
}

#[tokio::test]
async fn setup_missing_user_returns_error() {
    let cfg = fake_oauth_config();
    let interaction = json!({
        "type": 2,
        "data": { "name": "tpp-setup" }
    });

    let response = handle_setup(&cfg, &interaction).await;
    let content = response.data.unwrap().content.unwrap();
    assert!(content.contains("無法取得 user id"));
}

#[tokio::test]
async fn ping_without_authorization_returns_error() {
    let store = FakeStore::default();
    let interaction = json!({
        "type": 2,
        "data": { "name": "tpp-ping" },
        "user": { "id": "user-no-webhook" }
    });

    let response = handle_ping(&store, &interaction).await;

    assert_eq!(response.kind, InteractionResponseType::ChannelMessageWithSource);
    let data = response.data.expect("has data");
    assert_eq!(data.flags, Some(MessageFlags::EPHEMERAL));
    assert!(data.content.as_deref().unwrap_or("").contains("尚未授權"));
}

#[tokio::test]
async fn ping_missing_user_id_returns_error() {
    let store = FakeStore::default();
    let interaction = json!({
        "type": 2,
        "data": { "name": "tpp-ping" }
    });

    let response = handle_ping(&store, &interaction).await;
    let content = response.data.unwrap().content.unwrap();
    assert!(content.contains("無法取得 user id"));
}

#[tokio::test]
async fn handle_click_returns_deferred_update_message() {
    let interaction = json!({
        "type": 3,
        "data": { "custom_id": "tpp-poc-test" },
        "member": { "user": { "id": "u" } },
        "message": { "webhook_id": "1234567890" }
    });

    let response = handle_click(&interaction).await;

    assert_eq!(response.kind, InteractionResponseType::DeferredUpdateMessage);
    assert!(response.data.is_none());
}
