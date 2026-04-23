# Webhook-Interaction Bridge POC — Stage 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Empirically validate whether a manually-created Discord webhook, combined with user-installed slash commands, can deliver a round-trip button interaction to Wisp's existing `/discord/interactions` endpoint — without requiring the bot to be installed in the target guild.

**Architecture:** Add a single-file POC module `src/tpp_poc.rs` holding an in-memory `PocState` (maps `user_id → webhook_url`) and three handlers (`handle_setup`, `handle_ping`, `handle_click`). Extend `handler.rs` with a type:3 (MessageComponent) branch and a slash-command name dispatcher, migrating existing raw u64 interaction numbers to `twilight-model` enums along the way.

**Tech Stack:** Rust 2024, Axum 0.8, twilight-model 0.16, reqwest 0.12, tokio RwLock, wiremock 0.6 (tests), axum-test 19 (tests), tracing.

**Spec:** [`../specs/2026-04-23-webhook-interaction-bridge-poc-stage1-design.md`](../specs/2026-04-23-webhook-interaction-bridge-poc-stage1-design.md)

## Pragmatic decisions

- **twilight-model scope**: use twilight enums for interaction types and response construction; use raw `serde_json::json!` for the webhook outbound message body (fewer dependencies on exact Button field shapes in twilight 0.16).
- **Test location**: `tests/tpp_poc_test.rs` (matches existing convention — all tests live under `tests/`, not inline).
- **POC scope boundary**: no full-router integration tests. Each handler function is unit-tested directly; the handler.rs dispatch changes are small and visually reviewable. Real end-to-end verification happens via the manual experiments in Task 10.

---

## File structure

| File | Role |
|---|---|
| `src/tpp_poc.rs` (new) | `PocState` + 3 handlers + webhook POST helper + ephemeral response builder |
| `src/lib.rs` (modify) | Add `pub mod tpp_poc;` |
| `src/platform/discord/handler.rs` (modify) | Migrate to twilight enums; add type:3 branch; add command-name dispatch inside type:2 |
| `src/main.rs` (modify) | Construct `Arc<PocState>`; inject into `DiscordState` |
| `tests/tpp_poc_test.rs` (new) | Unit tests for the 3 POC handlers |

---

## Task 1: Scaffold PocState and module

**Files:**
- Create: `src/tpp_poc.rs`
- Modify: `src/lib.rs`
- Test: `tests/tpp_poc_test.rs`

- [ ] **Step 1.1: Create module with PocState skeleton**

Create `src/tpp_poc.rs`:

```rust
//! Webhook-Interaction Bridge POC — Stage 1.
//!
//! Empirically validates whether manually-created Discord webhooks can deliver
//! button-click interactions to this app's Interactions Endpoint, combined with
//! user-installed slash commands. See
//! `docs/superpowers/specs/2026-04-23-webhook-interaction-bridge-poc-stage1-design.md`.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

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
```

- [ ] **Step 1.2: Expose module in lib.rs**

Edit `src/lib.rs` — add `pub mod tpp_poc;` under the other `pub mod` lines:

```rust
pub mod assistant;
pub mod config;
pub mod db;
pub mod error;
pub mod llm;
pub mod platform;
pub mod tools;
pub mod tpp_poc;
pub mod weather;
```

- [ ] **Step 1.3: Write test for PocState basic operations**

Create `tests/tpp_poc_test.rs`:

```rust
use wisp::tpp_poc::PocState;

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
```

- [ ] **Step 1.4: Run tests to verify scaffold**

Run: `cargo test --test tpp_poc_test`

Expected: both tests pass.

- [ ] **Step 1.5: Commit**

```bash
git add src/tpp_poc.rs src/lib.rs tests/tpp_poc_test.rs
git commit -m "feat(tpp_poc): scaffold PocState module for webhook POC"
```

---

## Task 2: Implement `handle_setup`

**Files:**
- Modify: `src/tpp_poc.rs`
- Test: `tests/tpp_poc_test.rs`

Goal: `/tpp-setup url:<webhook_url>` stores the URL keyed by invoker's user_id, returns an ephemeral `ChannelMessageWithSource` response.

- [ ] **Step 2.1: Write failing tests for `handle_setup`**

Append to `tests/tpp_poc_test.rs`:

```rust
use serde_json::json;
use twilight_model::channel::message::MessageFlags;
use twilight_model::http::interaction::InteractionResponseType;
use wisp::tpp_poc::{handle_setup, PocState};

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
```

- [ ] **Step 2.2: Run tests to verify failure**

Run: `cargo test --test tpp_poc_test handle_setup`

Expected: FAIL — `handle_setup` not defined.

- [ ] **Step 2.3: Implement `handle_setup` and helpers in `src/tpp_poc.rs`**

Append to `src/tpp_poc.rs`:

```rust
use serde_json::Value;
use twilight_model::channel::message::MessageFlags;
use twilight_model::http::interaction::{
    InteractionResponse, InteractionResponseData, InteractionResponseType,
};

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
```

- [ ] **Step 2.4: Run tests to verify they pass**

Run: `cargo test --test tpp_poc_test handle_setup`

Expected: all three tests pass.

- [ ] **Step 2.5: Commit**

```bash
git add src/tpp_poc.rs tests/tpp_poc_test.rs
git commit -m "feat(tpp_poc): implement /tpp-setup handler"
```

---

## Task 3: Implement `handle_ping` — not-registered path

**Files:**
- Modify: `src/tpp_poc.rs`
- Test: `tests/tpp_poc_test.rs`

- [ ] **Step 3.1: Write failing test**

Append to `tests/tpp_poc_test.rs`:

```rust
use wisp::tpp_poc::handle_ping;

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
```

- [ ] **Step 3.2: Run test to verify failure**

Run: `cargo test --test tpp_poc_test handle_ping_without_setup`

Expected: FAIL — `handle_ping` not defined.

- [ ] **Step 3.3: Implement `handle_ping` no-url path**

Append to `src/tpp_poc.rs`:

```rust
/// `/tpp-ping` — POST a single button message to the invoker's registered webhook.
pub async fn handle_ping(state: &PocState, interaction: &Value) -> InteractionResponse {
    let Some(user_id) = extract_user_id(interaction) else {
        return ephemeral("⚠️ Error: missing user id in interaction payload");
    };

    let url = match state.webhooks.read().await.get(&user_id).cloned() {
        Some(u) => u,
        None => {
            return ephemeral("⚠️ 尚未登記 webhook，請先 /tpp-setup url:<url>");
        }
    };

    // Webhook POST is added in Task 4.
    ephemeral(format!("Would POST to {url} (not implemented yet)"))
}
```

- [ ] **Step 3.4: Run test to verify pass**

Run: `cargo test --test tpp_poc_test handle_ping_without_setup`

Expected: PASS.

- [ ] **Step 3.5: Commit**

```bash
git add src/tpp_poc.rs tests/tpp_poc_test.rs
git commit -m "feat(tpp_poc): add /tpp-ping not-registered branch"
```

---

## Task 4: Implement `handle_ping` — happy path with webhook POST

**Files:**
- Modify: `src/tpp_poc.rs`
- Test: `tests/tpp_poc_test.rs`

- [ ] **Step 4.1: Write failing test (wiremock)**

Append to `tests/tpp_poc_test.rs`:

```rust
use wiremock::matchers::{body_partial_json, method};
use wiremock::{Mock, MockServer, ResponseTemplate};

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
```

- [ ] **Step 4.2: Run tests to verify failure**

Run: `cargo test --test tpp_poc_test handle_ping_posts`

Expected: FAIL — current stub returns "Would POST".

- [ ] **Step 4.3: Replace `handle_ping` body with real POST**

In `src/tpp_poc.rs`, replace the current `handle_ping` (keep the not-registered branch, replace the placeholder):

```rust
pub async fn handle_ping(state: &PocState, interaction: &Value) -> InteractionResponse {
    let Some(user_id) = extract_user_id(interaction) else {
        return ephemeral("⚠️ Error: missing user id in interaction payload");
    };

    let url = match state.webhooks.read().await.get(&user_id).cloned() {
        Some(u) => u,
        None => {
            return ephemeral("⚠️ 尚未登記 webhook，請先 /tpp-setup url:<url>");
        }
    };

    // Component numeric types: 1 = ACTION_ROW, 2 = BUTTON. Button style 1 = PRIMARY.
    // Kept as raw JSON (not twilight-model structs) because this shape mirrors the
    // Discord webhook Execute payload and doesn't depend on twilight field layouts.
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

    tracing::info!(event = "tpp_poc.ping.send.start", user_id = %user_id, url = %url);

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
```

- [ ] **Step 4.4: Run tests to verify pass**

Run: `cargo test --test tpp_poc_test handle_ping`

Expected: all `handle_ping` tests pass (the two new ones plus the not-registered one from Task 3).

- [ ] **Step 4.5: Commit**

```bash
git add src/tpp_poc.rs tests/tpp_poc_test.rs
git commit -m "feat(tpp_poc): post button message to webhook on /tpp-ping"
```

---

## Task 5: Implement `handle_click` (type:3 handler)

**Files:**
- Modify: `src/tpp_poc.rs`
- Test: `tests/tpp_poc_test.rs`

- [ ] **Step 5.1: Write failing test**

Append to `tests/tpp_poc_test.rs`:

```rust
use wisp::tpp_poc::handle_click;

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
```

- [ ] **Step 5.2: Run test to verify failure**

Run: `cargo test --test tpp_poc_test handle_click`

Expected: FAIL — `handle_click` not defined.

- [ ] **Step 5.3: Implement `handle_click`**

Append to `src/tpp_poc.rs`:

```rust
/// type:3 MessageComponent handler — logs the full payload and ACKs with
/// DeferredUpdateMessage (does not change the original message).
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
```

- [ ] **Step 5.4: Run test to verify pass**

Run: `cargo test --test tpp_poc_test handle_click`

Expected: PASS.

- [ ] **Step 5.5: Run all POC tests to confirm no regressions**

Run: `cargo test --test tpp_poc_test`

Expected: all tests pass.

- [ ] **Step 5.6: Commit**

```bash
git add src/tpp_poc.rs tests/tpp_poc_test.rs
git commit -m "feat(tpp_poc): add handle_click with payload logging"
```

---

## Task 6: Migrate `handler.rs` to twilight-model enums

**Files:**
- Modify: `src/platform/discord/handler.rs`

**Goal:** replace raw integer literals (1/2/5/6/64) with twilight-model enum variants. No behavior change. Existing tests in `tests/interaction_test.rs` and `tests/discord_verify_test.rs` must continue to pass.

- [ ] **Step 6.1: Read the current handler.rs to reconfirm the exact spots to change**

Run: `sed -n '1,170p' src/platform/discord/handler.rs`

Targets to migrate (by line pattern — line numbers will shift as edits are applied):
1. `let interaction_type = interaction["type"].as_u64().unwrap_or(0);` and its `match` arms `1 =>` / `2 =>`.
2. PONG response: `Json(json!({"type": 1}))`.
3. Deferred response (ephemeral / non-ephemeral branches): `Json(json!({"type": 5, "data": {"flags": 64}}))` and `Json(json!({"type": 5}))`.
4. The `handle_ping_only` function (used by the test-only `ping_router`).

- [ ] **Step 6.2: Add twilight-model imports at top of `src/platform/discord/handler.rs`**

Insert after existing `use` lines (near the top):

```rust
use twilight_model::application::interaction::InteractionType;
use twilight_model::channel::message::MessageFlags;
use twilight_model::http::interaction::{
    InteractionResponse, InteractionResponseData, InteractionResponseType,
};
```

- [ ] **Step 6.3: Migrate the main `handle_interaction` dispatch**

Replace the existing dispatch block (the part starting `let interaction_type = interaction["type"].as_u64()...` through the `match interaction_type { ... }` closing brace) with:

```rust
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
        let guild_id = interaction["guild_id"].as_str();
        let channel_id = interaction["channel_id"].as_str().unwrap_or("");

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

    _ => (StatusCode::BAD_REQUEST, "Unknown interaction type").into_response(),
}
```

- [ ] **Step 6.4: Migrate the test-only `handle_ping_only`**

Replace the body of `handle_ping_only` — the `if interaction["type"].as_u64() == Some(1) { ... }` block — with:

```rust
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
```

- [ ] **Step 6.5: Build and run existing tests to confirm no regressions**

Run: `cargo build`
Expected: compiles clean.

Run: `cargo test --test interaction_test --test discord_verify_test`
Expected: all existing tests still pass.

- [ ] **Step 6.6: Commit**

```bash
git add src/platform/discord/handler.rs
git commit -m "refactor(discord): use twilight-model enums for interaction types"
```

---

## Task 7: Wire POC handlers into handler.rs

**Files:**
- Modify: `src/platform/discord/handler.rs`

**Goal:** inside the `ApplicationCommand` arm, dispatch `tpp-setup` and `tpp-ping` to POC handlers before falling through to the existing Assistant flow. Add a new `MessageComponent` arm that calls `handle_click`.

`DiscordState` will gain a `poc` field (wired in Task 8); this task can forward-reference it — compilation succeeds once Task 8 is done, so we accept the build will break until both tasks are committed (an acceptable temporary state with an immediate follow-up).

- [ ] **Step 7.1: Add `pub poc: Arc<wisp::tpp_poc::PocState>` to `DiscordState`**

In `src/platform/discord/handler.rs`, update the `DiscordState` struct:

```rust
#[derive(Clone)]
pub struct DiscordState {
    pub public_key_hex: String,
    pub application_id: String,
    pub bot_token: String,
    pub assistant: Arc<Assistant>,
    pub users: Arc<UserService>,
    pub allowed_channels: Arc<AllowedChannels>,
    pub poc: Arc<crate::tpp_poc::PocState>,
}
```

- [ ] **Step 7.2: Add command-name dispatch inside `ApplicationCommand` arm**

In the dispatch match from Task 6, wrap the existing `ApplicationCommand` body so POC commands short-circuit before the Assistant flow:

```rust
Some(InteractionType::ApplicationCommand) => {
    let command_name = interaction["data"]["name"].as_str().unwrap_or("");

    // POC commands: handled synchronously, return their own InteractionResponse.
    match command_name {
        "tpp-setup" => {
            return Json(crate::tpp_poc::handle_setup(&state.poc, &interaction).await)
                .into_response();
        }
        "tpp-ping" => {
            return Json(crate::tpp_poc::handle_ping(&state.poc, &interaction).await)
                .into_response();
        }
        _ => {}
    }

    // Fall through to existing Assistant flow.
    let guild_id = interaction["guild_id"].as_str();
    let channel_id = interaction["channel_id"].as_str().unwrap_or("");

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
```

Note: `return` in a match arm inside an `async fn` returning `impl IntoResponse` works because the match value is the function's return expression. If the function structure forbids `return`, refactor to assign to a variable. Run the build after this step to confirm.

- [ ] **Step 7.3: Add `MessageComponent` arm**

Add a new arm between `ApplicationCommand` and the fallthrough `_`:

```rust
Some(InteractionType::MessageComponent) => {
    Json(crate::tpp_poc::handle_click(&interaction).await).into_response()
}
```

- [ ] **Step 7.4: Build (expect failure — main.rs not updated yet)**

Run: `cargo build`

Expected: FAIL at `main.rs` — missing `poc` field when constructing `DiscordState`.

This is intended; Task 8 resolves it immediately.

**Do not commit yet.** Commit together with Task 8 so HEAD always builds.

---

## Task 8: Integrate `PocState` into `main.rs`

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 8.1: Add `use` for `PocState`**

Near the other `use wisp::...` lines at the top of `src/main.rs`:

```rust
use wisp::tpp_poc::PocState;
```

- [ ] **Step 8.2: Construct `PocState` and inject into `DiscordState`**

Inside the `if let Some(ref discord_config) = config.discord { ... }` block, update the `DiscordState` construction to include `poc`:

```rust
if let Some(ref discord_config) = config.discord {
    let discord_state = Arc::new(DiscordState {
        public_key_hex: discord_config.public_key.clone(),
        application_id: discord_config.application_id.clone(),
        bot_token: discord_config.bot_token.clone(),
        assistant: assistant.clone(),
        users: users.clone(),
        allowed_channels: allowed_channels.clone(),
        poc: PocState::new(),
    });
    app = app.nest("/discord", discord_routes(discord_state));
    tracing::info!("Discord platform enabled");
}
```

- [ ] **Step 8.3: Build to verify**

Run: `cargo build`

Expected: compiles clean.

- [ ] **Step 8.4: Run the full test suite**

Run: `cargo test`

Expected: all tests pass (ignored integration tests stay ignored).

- [ ] **Step 8.5: Commit Tasks 7 + 8 together**

```bash
git add src/platform/discord/handler.rs src/main.rs
git commit -m "feat(discord): dispatch tpp-setup/tpp-ping and type:3 clicks to tpp_poc"
```

---

## Task 9: Discord Developer Portal configuration + slash command registration

**Not code — follow these steps manually, then verify.**

- [ ] **Step 9.1: Configure app as user-installable**

1. Go to https://discord.com/developers/applications → select the Wisp app
2. "Installation" tab:
   - Under "Installation Contexts", enable **User Install**
   - Keep Guild Install enabled (existing functionality stays working)
3. Save

- [ ] **Step 9.2: Obtain user-install link**

Still in "Installation" tab, copy the "Install Link" (Discord-provided). This is what the user opens to install the app to their account.

- [ ] **Step 9.3: Install the app to your Discord user account**

Open the install link in a browser while logged into the Discord account you'll use for experiments. Authorize.

- [ ] **Step 9.4: Register `/tpp-setup` slash command**

Use the project's `discord-commands` skill (or equivalent curl) to POST to
`https://discord.com/api/v10/applications/{application_id}/commands`:

```json
{
  "name": "tpp-setup",
  "description": "Register a webhook URL for TPP POC",
  "integration_types": [1],
  "contexts": [0, 1, 2],
  "options": [
    {
      "type": 3,
      "name": "url",
      "description": "Discord webhook URL",
      "required": true
    }
  ]
}
```

Authorization header: `Bot <bot_token>`. Content-Type: `application/json`.

Expected: HTTP 200/201 with JSON echoing the command.

- [ ] **Step 9.5: Register `/tpp-ping`**

Same endpoint, separate request:

```json
{
  "name": "tpp-ping",
  "description": "Send a test button message to the registered webhook",
  "integration_types": [1],
  "contexts": [0, 1, 2]
}
```

- [ ] **Step 9.6: Verify commands appear in Discord**

In any channel where you're installed (guild channel, bot DM, or group DM), type `/`. Expected: `tpp-setup` and `tpp-ping` appear in the autocomplete.

---

## Task 10: Run experiments and record results

**Not code — operate the deployed Wisp and record observations.**

Prerequisites: Wisp is running somewhere reachable by Discord (e.g., the production VM), with the Interactions Endpoint URL configured in the Dev Portal pointing at `https://<host>/discord/interactions`. Confirm via Dev Portal's "Save Changes" returning success (Discord sends a PING that must validate).

- [ ] **Step 10.1: Prepare a manual webhook in a test Discord channel**

Pick a Discord guild where you have `Manage Webhooks` permission (this is your user permission, not the bot's — the bot does not need to be in the guild).

1. Right-click the channel → Edit Channel → Integrations → Webhooks → New Webhook
2. Copy the Webhook URL

- [ ] **Step 10.2: Experiment A — Guild channel (bot not installed)**

1. In the test channel, run: `/tpp-setup url:<webhook_url_from_10.1>`
2. Observe server log for `tpp_poc.setup`. **Record**: full payload JSON (redact tokens); note presence/absence of `channel_id`, `guild_id`, `context`, `authorizing_integration_owners`, `member` vs `user`.
3. Run: `/tpp-ping`
4. Observe log for `tpp_poc.ping.send.start` and `tpp_poc.ping.send.done`. **Record**: HTTP status returned by Discord; confirm the button message appears in the channel.
5. Click the button.
6. Observe log for `tpp_poc.click`. **Record**: full click payload. Note `application_id`, `message.webhook_id`, `message.id`, `channel_id`, `user.id` or `member.user.id`, `data.custom_id`, `guild_id`, and anything else that looks useful.
7. If the click does **not** arrive (no `tpp_poc.click` log) within ~10 seconds of clicking, mark Q3 as FAILED.

- [ ] **Step 10.3: Experiment B — Bot DM**

Open a direct message with the Wisp bot.

1. Run `/tpp-setup url:<any webhook url, can reuse 10.1's>`
2. Record payload differences vs Experiment A.
3. Skip `/tpp-ping` — webhook doesn't belong to this conversation.

- [ ] **Step 10.4: Experiment C — Group DM**

In a group DM with 2+ other users (or with Wisp bot added), run `/tpp-setup url:<any>`.

Record payload differences.

- [ ] **Step 10.5: Write up results**

Append a `## POC 結果` section to `docs/feature-request/webhook-interaction-bridge-poc.md`:

```markdown
## POC 結果 (Stage 1)

實驗日期：YYYY-MM-DD

### Q1 — user-installed slash command payload
(貼 `/tpp-setup` 收到的關鍵欄位 / 完整 payload 片段)

### Q2 — webhook accepts components
(貼 Discord 對 `/tpp-ping` POST 的 HTTP status 與 body)

### Q3 — button click routes to interactions endpoint
PASS / FAIL。(若 PASS：哪個 context 下成立；若 FAIL：描述觀察)

### Q4 — click payload structure
(貼 click interaction payload 的關鍵欄位)

### Q5 — context differences
(列表比較 Experiment A/B/C 的 payload 差異)

### 階段 2 下一步
(直接進實作 / 需改走 OAuth webhook.incoming / 放棄，附理由)
```

- [ ] **Step 10.6: Commit the writeup**

```bash
git add docs/feature-request/webhook-interaction-bridge-poc.md
git commit -m "docs(tpp_poc): record Stage 1 experiment results"
```

---

## Verification Summary

Stage 1 is complete when:

- [x] `cargo test` passes
- [x] `src/tpp_poc.rs` implements `PocState`, `handle_setup`, `handle_ping`, `handle_click`
- [x] `src/platform/discord/handler.rs` dispatches type:3 and `tpp-*` commands to `tpp_poc`
- [x] `src/platform/discord/handler.rs` uses `twilight-model` enums instead of raw u64 interaction numbers
- [x] Wisp app is user-installable in Discord Dev Portal
- [x] `/tpp-setup` and `/tpp-ping` slash commands are registered with `integration_types: [1]`
- [x] Experiment A run to completion, Q3 + Q4 answered
- [x] Experiments B/C run (unless A answered Q5 comprehensively)
- [x] Results committed to `docs/feature-request/webhook-interaction-bridge-poc.md`
