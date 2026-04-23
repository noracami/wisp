//! Webhook-Interaction Bridge POC — Stage 1.
//!
//! Empirically validates whether manually-created Discord webhooks can deliver
//! button-click interactions to this app's Interactions Endpoint, combined with
//! user-installed slash commands. See
//! `docs/superpowers/specs/2026-04-23-webhook-interaction-bridge-poc-stage1-design.md`.

use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use twilight_model::channel::message::MessageFlags;
use twilight_model::http::interaction::{
    InteractionResponse, InteractionResponseData, InteractionResponseType,
};

/// In-memory registry: one webhook URL per user (the user who ran `/tpp-setup`).
pub struct PocState {
    pub webhooks: RwLock<HashMap<String, String>>,
}

impl PocState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            webhooks: RwLock::new(HashMap::new()),
        })
    }
}

impl Default for PocState {
    fn default() -> Self {
        Self {
            webhooks: RwLock::new(HashMap::new()),
        }
    }
}

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

fn extract_option_value<'a>(interaction: &'a Value, name: &str) -> Option<&'a str> {
    interaction["data"]["options"]
        .as_array()?
        .iter()
        .find(|o| o["name"].as_str() == Some(name))
        .and_then(|o| o["value"].as_str())
}

/// `/tpp-setup url:<url>` — register a webhook URL against the invoker.
pub async fn handle_setup(state: &PocState, interaction: &Value) -> InteractionResponse {
    tracing::info!(
        event = "tpp_poc.setup",
        payload = %serde_json::to_string(interaction).unwrap_or_default(),
    );

    let Some(user_id) = extract_user_id(interaction) else {
        return ephemeral("⚠️ Error: missing user id in interaction payload");
    };

    let Some(url) = extract_option_value(interaction, "url") else {
        return ephemeral("⚠️ Error: missing `url` option");
    };

    state.webhooks.write().await.insert(user_id.clone(), url.to_string());
    tracing::info!(event = "tpp_poc.setup.stored", user_id = %user_id);

    ephemeral(format!("✅ Registered webhook for <@{user_id}>"))
}
