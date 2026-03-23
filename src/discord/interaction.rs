use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use super::verify::verify_signature;
use super::webhook::WebhookClient;
use crate::llm::claude::ClaudeClient;
use crate::llm::ChatMessage;
use crate::db::memory::Memory;

/// Full application state for the interaction endpoint.
#[derive(Clone)]
pub struct AppState {
    pub public_key_hex: String,
    pub application_id: String,
    pub bot_token: String,
    pub claude: Arc<ClaudeClient>,
    pub memory: Arc<Memory>,
    pub webhook: Arc<WebhookClient>,
}

/// Test-only router with minimal state (no DB, no LLM).
pub fn test_router(public_key_hex: String) -> Router {
    let state = Arc::new(TestState { public_key_hex });
    Router::new()
        .route("/interactions", post(handle_interaction_test))
        .with_state(state)
}

#[derive(Clone)]
struct TestState {
    public_key_hex: String,
}

async fn handle_interaction_test(
    State(state): State<Arc<TestState>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    handle_interaction_common(&state.public_key_hex, &headers, &body, None)
}

/// Full application router.
pub fn app_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/interactions", post(handle_interaction_full))
        .route("/health", axum::routing::get(|| async { "ok" }))
        .with_state(state)
}

async fn handle_interaction_full(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    handle_interaction_common(&state.public_key_hex, &headers, &body, Some(state.clone()))
}

fn handle_interaction_common(
    public_key_hex: &str,
    headers: &HeaderMap,
    body: &Bytes,
    app_state: Option<Arc<AppState>>,
) -> axum::response::Response {
    let signature = headers
        .get("X-Signature-Ed25519")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let timestamp = headers
        .get("X-Signature-Timestamp")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();

    if verify_signature(public_key_hex, signature, timestamp, body).is_err() {
        return (StatusCode::UNAUTHORIZED, "Invalid signature").into_response();
    }

    let interaction: Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid JSON").into_response(),
    };

    let interaction_type = interaction["type"].as_u64().unwrap_or(0);

    match interaction_type {
        // PING
        1 => Json(json!({"type": 1})).into_response(),

        // APPLICATION_COMMAND
        2 => {
            if let Some(state) = app_state {
                let interaction = interaction.clone();
                tokio::spawn(async move {
                    if let Err(e) = process_command(state.clone(), interaction).await {
                        tracing::error!("Failed to process command: {e}");
                        let _ = send_error_response(&state, &e.to_string()).await;
                    }
                });
            }
            // Respond with DEFERRED_CHANNEL_MESSAGE_WITH_SOURCE
            Json(json!({"type": 5})).into_response()
        }

        _ => (StatusCode::BAD_REQUEST, "Unknown interaction type").into_response(),
    }
}

async fn process_command(
    state: Arc<AppState>,
    interaction: Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let user_id = interaction["member"]["user"]["id"]
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

    // Load conversation context
    // TODO: resolve real user_id via UserService in Task 7
    let dummy_user_id = Uuid::new_v4();
    let conv_id = state
        .memory
        .get_or_create_conversation(dummy_user_id, channel_id, "discord")
        .await?;
    state
        .memory
        .store_message(conv_id, "user", user_message, None)
        .await?;

    let history = state.memory.load_recent_messages(conv_id, 20).await?;
    let messages: Vec<ChatMessage> = history
        .into_iter()
        .map(|m| ChatMessage {
            role: m.role,
            content: m.content,
        })
        .collect();

    // Call Claude
    let response = state
        .claude
        .chat(
            &messages,
            Some("You are Wisp, a helpful AI assistant on Discord. Keep responses concise."),
        )
        .await?;

    // Store assistant response
    state
        .memory
        .store_message(conv_id, "assistant", &response, None)
        .await?;

    // Update deferred response via Discord API
    let client = twilight_http::Client::new(state.bot_token.clone());
    let app_id = twilight_model::id::Id::new(state.application_id.parse::<u64>()?);
    client
        .interaction(app_id)
        .update_response(interaction_token)
        .content(Some(&response))
        .await?;

    Ok(())
}

/// Send an error message back when process_command fails.
async fn send_error_response(
    state: &AppState,
    _error_msg: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    state
        .webhook
        .send_message("Sorry, something went wrong processing your request.")
        .await?;
    Ok(())
}
