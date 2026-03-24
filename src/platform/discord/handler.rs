use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};
use std::sync::Arc;

use super::verify::verify_signature;
use crate::assistant::service::Assistant;
use crate::db::users::UserService;
use crate::platform::{ChatRequest, Platform};

#[derive(Clone)]
pub struct DiscordState {
    pub public_key_hex: String,
    pub application_id: String,
    pub bot_token: String,
    pub assistant: Arc<Assistant>,
    pub users: Arc<UserService>,
}

pub fn routes(state: Arc<DiscordState>) -> Router {
    Router::new()
        .route("/interactions", post(handle_interaction))
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

    let interaction_type = interaction["type"].as_u64().unwrap_or(0);

    match interaction_type {
        // PING
        1 => Json(json!({"type": 1})).into_response(),

        // APPLICATION_COMMAND
        2 => {
            let in_guild = interaction["guild_id"].as_str().is_some();
            let state = state.clone();
            let interaction = interaction.clone();
            tokio::spawn(async move {
                if let Err(e) = process_command(&state, &interaction).await {
                    tracing::error!("Failed to process command: {e}");
                    let _ = send_error_followup(&state, &interaction).await;
                }
            });
            // In server: ephemeral (flag 64), in DM: public
            if in_guild {
                Json(json!({"type": 5, "data": {"flags": 64}})).into_response()
            } else {
                Json(json!({"type": 5})).into_response()
            }
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

    if interaction["type"].as_u64() == Some(1) {
        Json(json!({"type": 1})).into_response()
    } else {
        (StatusCode::BAD_REQUEST, "Only PING supported in test mode").into_response()
    }
}
