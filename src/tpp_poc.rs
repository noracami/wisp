//! Webhook-Interaction Bridge POC — Stage 2.
//!
//! Stage 1 empirically confirmed that manually-created channel webhooks
//! silently strip `components`; Stage 2 pivots to Discord OAuth2
//! `webhook.incoming` to obtain an application-owned webhook that supports
//! buttons. See
//! `docs/superpowers/specs/2026-04-23-webhook-interaction-bridge-poc-stage2-design.md`.

use serde_json::Value;
use twilight_model::channel::message::MessageFlags;
use twilight_model::http::interaction::{
    InteractionResponse, InteractionResponseData, InteractionResponseType,
};

use crate::db::tpp_webhooks::TppWebhookStore;
use crate::platform::discord::oauth::{self, TppOAuthConfig};

/// Build an ephemeral `ChannelMessageWithSource` response (visible only to invoker).
pub(crate) fn ephemeral(content: impl Into<String>) -> InteractionResponse {
    InteractionResponse {
        kind: InteractionResponseType::ChannelMessageWithSource,
        data: Some(InteractionResponseData {
            content: Some(content.into()),
            flags: Some(MessageFlags::EPHEMERAL),
            ..Default::default()
        }),
    }
}

/// Extract the invoker's user id from either `member.user.id` (guild) or `user.id` (DM).
fn extract_user_id(interaction: &Value) -> Option<String> {
    interaction["member"]["user"]["id"]
        .as_str()
        .or_else(|| interaction["user"]["id"].as_str())
        .map(str::to_string)
}

/// `/tpp-setup` — return an ephemeral ephemeral link that starts the Discord
/// OAuth2 `webhook.incoming` flow. No args: the user picks the target channel
/// on Discord's authorize page.
pub async fn handle_setup(cfg: &TppOAuthConfig, interaction: &Value) -> InteractionResponse {
    tracing::info!(
        event = "tpp_poc.setup",
        payload = %serde_json::to_string(interaction).unwrap_or_default(),
    );

    let Some(user_id) = extract_user_id(interaction) else {
        return ephemeral("⚠️ 無法取得 user id");
    };

    let state = oauth::generate_state(&user_id, &cfg.state_secret);
    let url = oauth::build_authorize_url(&cfg.application_id, &cfg.redirect_uri, &state);

    ephemeral(format!(
        "點此授權 Wisp 建立 webhook：{url}\n（連結 10 分鐘後失效）"
    ))
}

/// `/tpp-ping` — POST a single button message to the invoker's registered webhook.
pub async fn handle_ping(
    store: &dyn TppWebhookStore,
    interaction: &Value,
) -> InteractionResponse {
    let Some(user_id) = extract_user_id(interaction) else {
        return ephemeral("⚠️ 無法取得 user id");
    };

    let stored = match store.find_by_user(&user_id).await {
        Ok(Some(w)) => w,
        Ok(None) => return ephemeral("⚠️ 尚未授權，請先 /tpp-setup"),
        Err(e) => {
            tracing::error!(event = "tpp_poc.ping.db.error", error = %e);
            return ephemeral("❌ DB 錯誤");
        }
    };

    let url = format!(
        "https://discord.com/api/webhooks/{}/{}",
        stored.webhook_id, stored.webhook_token
    );

    // Component numeric types: 1 = ACTION_ROW, 2 = BUTTON. Button style 1 = PRIMARY.
    let body = serde_json::json!({
        "content": "POC button test — 請點下方按鈕",
        "components": [{
            "type": 1,
            "components": [{
                "type": 2,
                "style": 1,
                "label": "Click me",
                "custom_id": "tpp-poc-test"
            }]
        }]
    });

    tracing::info!(
        event = "tpp_poc.ping.send.start",
        user_id = %user_id,
        webhook_id = %stored.webhook_id,
    );

    let result = reqwest::Client::new().post(&url).json(&body).send().await;

    match result {
        Ok(response) => {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            tracing::info!(
                event = "tpp_poc.ping.send.done",
                status = status.as_u16(),
                body = %text,
            );
            if status.is_success() {
                ephemeral("✅ Sent")
            } else {
                ephemeral(format!("⚠️ Webhook returned {status}: {text}"))
            }
        }
        Err(e) => {
            tracing::warn!(event = "tpp_poc.ping.send.error", error = %e);
            ephemeral(format!("❌ Failed to POST webhook: {e}"))
        }
    }
}

/// type:3 MessageComponent handler — logs the full payload and ACKs with
/// DeferredUpdateMessage (does not change the original message). Unchanged
/// from Stage 1.
pub async fn handle_click(interaction: &Value) -> InteractionResponse {
    tracing::info!(
        event = "tpp_poc.click",
        payload = %serde_json::to_string(interaction).unwrap_or_default(),
    );

    InteractionResponse {
        kind: InteractionResponseType::DeferredUpdateMessage,
        data: None,
    }
}
