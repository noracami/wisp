use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::Value;
use std::sync::Arc;

use crate::assistant::service::Assistant;
use crate::db::users::UserService;
use crate::error::AppError;
use crate::platform::{ChatRequest, Platform};

#[derive(Clone)]
pub struct LineState {
    pub channel_secret: String,
    pub channel_access_token: String,
    pub assistant: Arc<Assistant>,
    pub users: Arc<UserService>,
    pub client: Arc<super::client::LineClient>,
}

pub fn routes(state: Arc<LineState>) -> Router {
    Router::new()
        .route("/webhook", post(handle_webhook))
        .with_state(state)
}

pub fn verify_line_signature(
    channel_secret: &str,
    signature: &str,
    body: &[u8],
) -> Result<(), AppError> {
    let mut mac = Hmac::<Sha256>::new_from_slice(channel_secret.as_bytes())
        .map_err(|e| AppError::Internal(format!("HMAC error: {e}")))?;
    mac.update(body);

    let decoded = BASE64.decode(signature)
        .map_err(|_| AppError::VerificationFailed)?;
    mac.verify_slice(&decoded)
        .map_err(|_| AppError::VerificationFailed)?;
    Ok(())
}

async fn handle_webhook(
    State(state): State<Arc<LineState>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let signature = headers
        .get("X-Line-Signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();

    if verify_line_signature(&state.channel_secret, signature, &body).is_err() {
        return StatusCode::UNAUTHORIZED;
    }

    let payload: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return StatusCode::BAD_REQUEST,
    };

    // Process events in background
    let events = payload["events"].as_array().cloned().unwrap_or_default();
    for event in events {
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = process_event(state, event).await {
                tracing::error!("Failed to process LINE event: {e}");
            }
        });
    }

    StatusCode::OK
}

async fn process_event(
    state: Arc<LineState>,
    event: Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let event_type = event["type"].as_str().unwrap_or("");
    if event_type != "message" {
        return Ok(());
    }

    let message_type = event["message"]["type"].as_str().unwrap_or("");
    if message_type != "text" {
        return Ok(());
    }

    let platform_user_id = event["source"]["userId"].as_str().unwrap_or("unknown");
    let reply_token = event["replyToken"].as_str().unwrap_or("");
    let text = event["message"]["text"].as_str().unwrap_or("");

    // Show loading animation (only works in 1-on-1 chats, silently ignored otherwise)
    let _ = state.client.show_loading(platform_user_id, 20).await;

    // Determine channel_id based on source type
    let channel_id = event["source"]["groupId"]
        .as_str()
        .or_else(|| event["source"]["roomId"].as_str())
        .unwrap_or(platform_user_id);

    // Resolve user
    let user_id = state.users.resolve_or_create("line", platform_user_id).await?;

    let request = ChatRequest {
        user_id,
        channel_id: channel_id.to_string(),
        platform: Platform::Line,
        message: text.to_string(),
    };

    let response = state.assistant.handle(request).await?;

    // Try reply first, fall back to push if it fails
    if state.client.reply(reply_token, &response.text).await.is_err() {
        tracing::warn!("Reply token expired, using push message");
        state.client.push(platform_user_id, &response.text).await?;
    }

    Ok(())
}
