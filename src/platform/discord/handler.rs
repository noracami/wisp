use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use twilight_model::application::interaction::InteractionType;
use twilight_model::channel::message::MessageFlags;
use twilight_model::http::interaction::{
    InteractionResponse, InteractionResponseData, InteractionResponseType,
};

use super::oauth::{self, TppOAuthConfig};
use super::verify::verify_signature;
use crate::assistant::service::Assistant;
use crate::db::allowed_channels::AllowedChannels;
use crate::db::tpp_webhooks::TppWebhookStore;
use crate::db::users::UserService;
use crate::platform::{ChatRequest, Platform};

#[derive(Clone)]
pub struct DiscordState {
    pub public_key_hex: String,
    pub application_id: String,
    pub bot_token: String,
    pub assistant: Arc<Assistant>,
    pub users: Arc<UserService>,
    pub allowed_channels: Arc<AllowedChannels>,
    pub tpp_webhooks: Arc<dyn TppWebhookStore>,
    pub oauth_config: TppOAuthConfig,
}

pub fn routes(state: Arc<DiscordState>) -> Router {
    Router::new()
        .route("/interactions", post(handle_interaction))
        .route("/oauth/callback", get(oauth_callback))
        .with_state(state)
}

async fn handle_interaction(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // Verify signature
    let signature = headers
        .get("X-Signature-Ed25519")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let timestamp = headers
        .get("X-Signature-Timestamp")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();

    if verify_signature(&state.public_key_hex, signature, timestamp, &body).is_err() {
        return (StatusCode::UNAUTHORIZED, "Invalid signature").into_response();
    }

    let interaction: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid JSON").into_response(),
    };

    let kind = interaction["type"]
        .as_u64()
        .and_then(|n| InteractionType::try_from(n as u8).ok());

    match kind {
        // PING
        Some(InteractionType::Ping) => Json(InteractionResponse {
            kind: InteractionResponseType::Pong,
            data: None,
        })
        .into_response(),

        // APPLICATION_COMMAND
        Some(InteractionType::ApplicationCommand) => {
            let command_name = interaction["data"]["name"].as_str().unwrap_or("");

            // POC commands: handled synchronously, return their own InteractionResponse.
            match command_name {
                "tpp-setup" => {
                    return Json(
                        crate::tpp_poc::handle_setup(&state.oauth_config, &interaction).await,
                    )
                    .into_response();
                }
                "tpp-ping" => {
                    return Json(
                        crate::tpp_poc::handle_ping(
                            state.tpp_webhooks.as_ref(),
                            &interaction,
                        )
                        .await,
                    )
                    .into_response();
                }
                _ => {}
            }

            // Fall through to existing Assistant flow.
            let guild_id = interaction["guild_id"].as_str();
            let channel_id = interaction["channel_id"].as_str().unwrap_or("");

            // Determine visibility: DM/Group DM = public, guild = check allowlist
            let ephemeral = if let Some(gid) = guild_id {
                !state.allowed_channels.is_public(gid, channel_id).await
            } else {
                false
            };

            let state = state.clone();
            let interaction = interaction.clone();
            tokio::spawn(async move {
                if let Err(e) = process_command(&state, &interaction).await {
                    tracing::error!("Failed to process command: {e}");
                    let _ = send_error_followup(&state, &interaction).await;
                }
            });

            let data = if ephemeral {
                Some(InteractionResponseData {
                    flags: Some(MessageFlags::EPHEMERAL),
                    ..Default::default()
                })
            } else {
                None
            };

            Json(InteractionResponse {
                kind: InteractionResponseType::DeferredChannelMessageWithSource,
                data,
            })
            .into_response()
        }

        Some(InteractionType::MessageComponent) => {
            Json(crate::tpp_poc::handle_click(&interaction).await).into_response()
        }

        _ => (StatusCode::BAD_REQUEST, "Unknown interaction type").into_response(),
    }
}

async fn process_command(
    state: &DiscordState,
    interaction: &Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let platform_user_id = interaction["member"]["user"]["id"]
        .as_str()
        .or_else(|| interaction["user"]["id"].as_str())
        .unwrap_or("unknown");
    let channel_id = interaction["channel_id"].as_str().unwrap_or("unknown");

    let user_message = interaction["data"]["options"]
        .as_array()
        .and_then(|opts| opts.first())
        .and_then(|opt| opt["value"].as_str())
        .unwrap_or("");

    let interaction_token = interaction["token"].as_str().unwrap_or("");

    // Resolve user identity
    let user_id = state.users.resolve_or_create("discord", platform_user_id).await?;

    // Build ChatRequest and delegate to Assistant
    let request = ChatRequest {
        user_id,
        channel_id: channel_id.to_string(),
        platform: Platform::Discord,
        message: user_message.to_string(),
    };

    let response = state.assistant.handle(request).await?;

    // Update deferred response via Discord API
    let client = twilight_http::Client::new(state.bot_token.clone());
    let app_id = twilight_model::id::Id::new(state.application_id.parse::<u64>()?);
    client
        .interaction(app_id)
        .update_response(interaction_token)
        .content(Some(&response.text))
        .await?;

    Ok(())
}

async fn send_error_followup(
    state: &DiscordState,
    interaction: &Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let interaction_token = interaction["token"].as_str().unwrap_or("");
    let client = twilight_http::Client::new(state.bot_token.clone());
    let app_id = twilight_model::id::Id::new(state.application_id.parse::<u64>()?);
    client
        .interaction(app_id)
        .update_response(interaction_token)
        .content(Some("Sorry, something went wrong processing your request."))
        .await?;
    Ok(())
}

#[derive(Deserialize)]
struct OAuthCallbackParams {
    code: String,
    state: String,
}

async fn oauth_callback(
    State(state): State<Arc<DiscordState>>,
    Query(params): Query<OAuthCallbackParams>,
) -> axum::response::Response {
    let user_id = match oauth::verify_state(&params.state, &state.oauth_config.state_secret) {
        Ok(uid) => uid,
        Err(e) => {
            tracing::warn!(event = "tpp_poc.oauth.state.invalid", error = %e);
            return (StatusCode::BAD_REQUEST, "invalid state").into_response();
        }
    };

    let token_response = match oauth::exchange_code(
        oauth::DISCORD_TOKEN_ENDPOINT,
        &state.oauth_config.application_id,
        &state.oauth_config.client_secret,
        &params.code,
        &state.oauth_config.redirect_uri,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(event = "tpp_poc.oauth.exchange.error", error = %e);
            return (StatusCode::BAD_GATEWAY, "token exchange failed").into_response();
        }
    };

    tracing::info!(
        event = "tpp_poc.oauth.callback",
        user_id = %user_id,
        webhook_id = %token_response.webhook.id,
        channel_id = %token_response.webhook.channel_id,
        guild_id = ?token_response.webhook.guild_id,
        channel_name = ?token_response.webhook.name,
    );

    if let Err(e) = state
        .tpp_webhooks
        .upsert(
            &user_id,
            &token_response.webhook.id,
            &token_response.webhook.token,
            &token_response.webhook.channel_id,
            token_response.webhook.guild_id.as_deref(),
            token_response.webhook.name.as_deref(),
        )
        .await
    {
        tracing::error!(event = "tpp_poc.oauth.db.error", error = %e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response();
    }

    Html(
        r#"<!doctype html><meta charset="utf-8">
<h1>✅ 授權完成</h1>
<p>回 Discord 打 <code>/tpp-ping</code> 測試</p>"#,
    )
    .into_response()
}

/// Test-only state and router for ping/pong without full app dependencies.
#[derive(Clone)]
pub struct DiscordPingState {
    pub public_key_hex: String,
}

pub fn ping_router(state: Arc<DiscordPingState>) -> Router {
    Router::new()
        .route("/interactions", post(handle_ping_only))
        .with_state(state)
}

async fn handle_ping_only(
    State(state): State<Arc<DiscordPingState>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let signature = headers.get("X-Signature-Ed25519").and_then(|v| v.to_str().ok()).unwrap_or_default();
    let timestamp = headers.get("X-Signature-Timestamp").and_then(|v| v.to_str().ok()).unwrap_or_default();

    if verify_signature(&state.public_key_hex, signature, timestamp, &body).is_err() {
        return (StatusCode::UNAUTHORIZED, "Invalid signature").into_response();
    }

    let interaction: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid JSON").into_response(),
    };

    let kind = interaction["type"]
        .as_u64()
        .and_then(|n| InteractionType::try_from(n as u8).ok());

    match kind {
        Some(InteractionType::Ping) => Json(InteractionResponse {
            kind: InteractionResponseType::Pong,
            data: None,
        })
        .into_response(),
        _ => (StatusCode::BAD_REQUEST, "Only PING supported in test mode").into_response(),
    }
}
