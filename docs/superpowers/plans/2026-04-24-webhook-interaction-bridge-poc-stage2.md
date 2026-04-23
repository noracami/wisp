# Webhook-Interaction Bridge POC — Stage 2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement Discord OAuth2 `webhook.incoming` flow end-to-end so that Experiment A of Stage 2 spec can validate Q2′/Q3/Q4 (the questions Stage 1 couldn't reach). Also capture Q6–Q9 observations as side-effects of the same flow.

**Architecture:** Add OAuth authorize-URL / callback / token-exchange to the Discord module; persist received webhooks to a new `tpp_webhooks` Postgres table; refactor existing `/tpp-setup` and `/tpp-ping` handlers to use DB instead of in-memory registry. `handle_click` stays unchanged from Stage 1.

**Tech Stack:** Rust 2024, Axum 0.8, sqlx 0.8 (Postgres), reqwest 0.12 (JSON+form), hmac 0.12 + sha2 0.10 (state token), base64 0.22, url 2 (new dep), wiremock 0.6 (tests), tokio.

**Spec:** [`../specs/2026-04-23-webhook-interaction-bridge-poc-stage2-design.md`](../specs/2026-04-23-webhook-interaction-bridge-poc-stage2-design.md)

## Pragmatic decisions

- **Module name stays `tpp_poc`**: Stage 2 is still validation; rename to `tpp_bridge` etc. is Stage 3's job.
- **State token is stateless HMAC**: `base64(user_id|issued_ts) + "." + base64(hmac_sha256(payload, secret))`. No DB/Redis table for CSRF state.
- **Trait over repo for testability**: introduce `TppWebhookStore` trait so `handle_ping`'s unit tests can inject an in-memory fake. `TppWebhookRepo` (concrete, Postgres) is the single production impl.
- **Webhook token stored in plaintext**: POC-acceptable; Stage 3 will evaluate encryption-at-rest.
- **`handle_click` not touched**: Stage 1 code passes review and is sufficient.
- **One OAuth per user, UPSERT-on-conflict**: `UNIQUE (user_id)` constraint; re-authorizing overwrites.
- **No automated `/discord/oauth/callback` e2e test**: wiremock-mocked token endpoint covers the happy path at unit level; real e2e is the manual Experiment A.

---

## File structure

| File | Role |
|---|---|
| `migrations/005_tpp_webhooks.sql` (new) | Table `tpp_webhooks` |
| `src/db/tpp_webhooks.rs` (new) | `TppWebhookStore` trait + `TppWebhookRepo` Postgres impl |
| `src/db/mod.rs` (modify) | `pub mod tpp_webhooks;` + register migration |
| `src/config.rs` (modify) | Add `client_secret`, `oauth_redirect_uri`, `state_secret` to `DiscordConfig` |
| `src/platform/discord/oauth.rs` (new) | State HMAC; `build_authorize_url`; `exchange_code`; response types |
| `src/platform/discord/mod.rs` (modify) | `pub mod oauth;` |
| `src/tpp_poc.rs` (modify) | Refactor `handle_setup` (returns authorize URL); refactor `handle_ping` (reads from store); remove `PocState::webhooks` |
| `src/platform/discord/handler.rs` (modify) | Add `/discord/oauth/callback` route; extend `DiscordState`; drop `poc` field's in-memory role |
| `src/main.rs` (modify) | Read new env vars, construct `TppWebhookRepo`, inject into `DiscordState` |
| `tests/tpp_poc_test.rs` (modify) | Update to match new handler signatures; add in-memory fake store |
| `tests/tpp_oauth_test.rs` (new) | Unit tests for `oauth::*` |
| `Cargo.toml` (modify) | Add `url = "2"`, `async-trait` already present |
| `.env.example` (modify) | Document new env vars |
| `scripts/register-tpp-commands.sh` (new) | Re-register `/tpp-setup` with options removed |

---

## Task 1: Database migration + `TppWebhookStore`

**Files:**
- Create: `migrations/005_tpp_webhooks.sql`
- Create: `src/db/tpp_webhooks.rs`
- Modify: `src/db/mod.rs`

- [ ] **Step 1.1: Create migration**

`migrations/005_tpp_webhooks.sql`:

```sql
CREATE TABLE IF NOT EXISTS tpp_webhooks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id TEXT NOT NULL,
    webhook_id TEXT NOT NULL,
    webhook_token TEXT NOT NULL,
    channel_id TEXT NOT NULL,
    guild_id TEXT,
    channel_name TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id)
);
```

- [ ] **Step 1.2: Register migration in `db::run_migrations`**

Edit `src/db/mod.rs` — append to the chain:

```rust
sqlx::raw_sql(include_str!("../../migrations/005_tpp_webhooks.sql"))
    .execute(pool)
    .await?;
```

and add `pub mod tpp_webhooks;` at top.

- [ ] **Step 1.3: Implement trait + concrete repo**

Create `src/db/tpp_webhooks.rs`:

```rust
use async_trait::async_trait;
use sqlx::PgPool;

#[derive(Debug, Clone)]
pub struct StoredWebhook {
    pub user_id: String,
    pub webhook_id: String,
    pub webhook_token: String,
    pub channel_id: String,
    pub guild_id: Option<String>,
    pub channel_name: Option<String>,
}

#[async_trait]
pub trait TppWebhookStore: Send + Sync {
    async fn upsert(
        &self,
        user_id: &str,
        webhook_id: &str,
        webhook_token: &str,
        channel_id: &str,
        guild_id: Option<&str>,
        channel_name: Option<&str>,
    ) -> sqlx::Result<()>;

    async fn find_by_user(&self, user_id: &str) -> sqlx::Result<Option<StoredWebhook>>;
}

pub struct TppWebhookRepo {
    pool: PgPool,
}

impl TppWebhookRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TppWebhookStore for TppWebhookRepo {
    async fn upsert(
        &self,
        user_id: &str,
        webhook_id: &str,
        webhook_token: &str,
        channel_id: &str,
        guild_id: Option<&str>,
        channel_name: Option<&str>,
    ) -> sqlx::Result<()> {
        sqlx::query(
            "INSERT INTO tpp_webhooks
                (user_id, webhook_id, webhook_token, channel_id, guild_id, channel_name)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (user_id) DO UPDATE SET
                webhook_id = EXCLUDED.webhook_id,
                webhook_token = EXCLUDED.webhook_token,
                channel_id = EXCLUDED.channel_id,
                guild_id = EXCLUDED.guild_id,
                channel_name = EXCLUDED.channel_name,
                updated_at = now()",
        )
        .bind(user_id)
        .bind(webhook_id)
        .bind(webhook_token)
        .bind(channel_id)
        .bind(guild_id)
        .bind(channel_name)
        .execute(&self.pool)
        .await
        .map(|_| ())
    }

    async fn find_by_user(&self, user_id: &str) -> sqlx::Result<Option<StoredWebhook>> {
        sqlx::query_as::<_, (String, String, String, String, Option<String>, Option<String>)>(
            "SELECT user_id, webhook_id, webhook_token, channel_id, guild_id, channel_name
             FROM tpp_webhooks WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map(|opt| opt.map(|(user_id, webhook_id, webhook_token, channel_id, guild_id, channel_name)| {
            StoredWebhook {
                user_id,
                webhook_id,
                webhook_token,
                channel_id,
                guild_id,
                channel_name,
            }
        }))
    }
}
```

- [ ] **Step 1.4: Build**

`cargo build` — expect success.

- [ ] **Step 1.5: Commit**

```
feat(db): add tpp_webhooks table and TppWebhookStore trait
```

---

## Task 2: Config extension

**Files:**
- Modify: `src/config.rs`
- Modify: `.env.example`

- [ ] **Step 2.1: Extend `DiscordConfig`**

```rust
#[derive(Debug, Clone)]
pub struct DiscordConfig {
    pub application_id: String,
    pub public_key: String,
    pub bot_token: String,
    pub webhook_url: String,
    // new
    pub client_secret: String,
    pub oauth_redirect_uri: String,
    pub state_secret: String,
}
```

- [ ] **Step 2.2: Extend `from_env` matching**

Add the new envs to the tuple match. Be careful not to break existing deploys: Stage 2 is an additive fixture — production `.env` must have the new vars before deploy. Keep the existing 4-var requirement grouped, then require 3 new vars.

```rust
let discord = match (
    env::var("DISCORD_APPLICATION_ID"),
    env::var("DISCORD_PUBLIC_KEY"),
    env::var("DISCORD_BOT_TOKEN"),
    env::var("DISCORD_WEBHOOK_URL"),
    env::var("DISCORD_CLIENT_SECRET"),
    env::var("DISCORD_OAUTH_REDIRECT_URI"),
    env::var("TPP_STATE_SECRET"),
) {
    (
        Ok(app_id),
        Ok(pub_key),
        Ok(bot_token),
        Ok(webhook_url),
        Ok(client_secret),
        Ok(redirect),
        Ok(state_secret),
    ) => Some(DiscordConfig {
        application_id: app_id,
        public_key: pub_key,
        bot_token,
        webhook_url,
        client_secret,
        oauth_redirect_uri: redirect,
        state_secret,
    }),
    _ => None,
};
```

**Note**: this tightens the Discord-enabled condition — existing deploys without the 3 new vars will silently disable Discord. Plan's Task 11 updates production `.env` before merge.

- [ ] **Step 2.3: Update `.env.example`**

Under the `# Discord (optional)` block, append:

```
DISCORD_CLIENT_SECRET=
DISCORD_OAUTH_REDIRECT_URI=https://your-host.example.com/discord/oauth/callback
TPP_STATE_SECRET=
```

- [ ] **Step 2.4: Build + commit**

```
feat(config): add Discord OAuth2 + TPP state secret env vars
```

---

## Task 3: OAuth module — types + state HMAC

**Files:**
- Create: `src/platform/discord/oauth.rs`
- Modify: `src/platform/discord/mod.rs`
- Modify: `Cargo.toml`
- Create: `tests/tpp_oauth_test.rs`

- [ ] **Step 3.1: Add `url` dep**

`Cargo.toml`:

```toml
url = "2"
```

(hmac / sha2 / base64 already present.)

- [ ] **Step 3.2: Create `oauth` module with types**

`src/platform/discord/oauth.rs`:

```rust
//! Discord OAuth2 `webhook.incoming` helpers for TPP POC Stage 2.
//!
//! Implements:
//! - stateless HMAC state token for CSRF protection
//! - authorize URL construction
//! - token exchange (code → webhook)

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

type HmacSha256 = Hmac<Sha256>;

const STATE_TTL_SECS: u64 = 600; // 10 minutes

#[derive(Debug, Error)]
pub enum StateError {
    #[error("malformed state")]
    Malformed,
    #[error("invalid HMAC signature")]
    BadSignature,
    #[error("state expired")]
    Expired,
}

#[derive(Debug, Error)]
pub enum OAuthError {
    #[error("token exchange HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("token exchange returned {status}: {body}")]
    NonSuccess { status: u16, body: String },
}

#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub token_type: String,
    pub access_token: String,
    pub scope: String,
    pub expires_in: i64,
    #[serde(default)]
    pub refresh_token: Option<String>,
    pub webhook: IncomingWebhook,
}

#[derive(Debug, Deserialize)]
pub struct IncomingWebhook {
    pub id: String,
    pub token: String,
    pub channel_id: String,
    #[serde(default)]
    pub guild_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    pub url: String,
}
```

- [ ] **Step 3.3: State HMAC generate/verify**

Append to `oauth.rs`:

```rust
/// Produce `b64(user_id|ts).b64(hmac_sha256(user_id|ts, secret))`.
pub fn generate_state(user_id: &str, secret: &str) -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let payload = format!("{user_id}|{ts}");

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("hmac key");
    mac.update(payload.as_bytes());
    let sig = mac.finalize().into_bytes();

    format!("{}.{}", B64.encode(payload), B64.encode(sig))
}

/// Returns the encoded `user_id` if the state verifies and is not expired.
pub fn verify_state(state: &str, secret: &str) -> Result<String, StateError> {
    let (payload_b64, sig_b64) = state.split_once('.').ok_or(StateError::Malformed)?;
    let payload = B64.decode(payload_b64).map_err(|_| StateError::Malformed)?;
    let sig = B64.decode(sig_b64).map_err(|_| StateError::Malformed)?;

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("hmac key");
    mac.update(&payload);
    mac.verify_slice(&sig).map_err(|_| StateError::BadSignature)?;

    let payload_str = String::from_utf8(payload).map_err(|_| StateError::Malformed)?;
    let (user_id, ts_str) = payload_str.split_once('|').ok_or(StateError::Malformed)?;
    let ts: u64 = ts_str.parse().map_err(|_| StateError::Malformed)?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if now.saturating_sub(ts) > STATE_TTL_SECS {
        return Err(StateError::Expired);
    }

    Ok(user_id.to_string())
}
```

- [ ] **Step 3.4: Expose module**

`src/platform/discord/mod.rs` — add `pub mod oauth;`.

- [ ] **Step 3.5: Unit tests**

Create `tests/tpp_oauth_test.rs`:

```rust
use wisp::platform::discord::oauth::{generate_state, verify_state, StateError};

#[test]
fn state_roundtrip() {
    let token = generate_state("329579602429214721", "secret");
    let uid = verify_state(&token, "secret").unwrap();
    assert_eq!(uid, "329579602429214721");
}

#[test]
fn state_wrong_secret() {
    let token = generate_state("user", "secret");
    let err = verify_state(&token, "other").unwrap_err();
    assert!(matches!(err, StateError::BadSignature));
}

#[test]
fn state_tampered_payload() {
    let token = generate_state("user", "secret");
    let (p, s) = token.split_once('.').unwrap();
    let tampered = format!("{}X.{}", p, s);
    let err = verify_state(&tampered, "secret").unwrap_err();
    assert!(matches!(err, StateError::Malformed | StateError::BadSignature));
}

#[test]
fn state_malformed() {
    let err = verify_state("not-a-state", "secret").unwrap_err();
    assert!(matches!(err, StateError::Malformed));
}
```

- [ ] **Step 3.6: Run tests + commit**

`cargo test --test tpp_oauth_test`

```
feat(discord-oauth): add state HMAC + token response types
```

---

## Task 4: OAuth module — authorize URL + token exchange

**Files:**
- Modify: `src/platform/discord/oauth.rs`
- Modify: `tests/tpp_oauth_test.rs`

- [ ] **Step 4.1: Authorize URL builder**

Append to `oauth.rs`:

```rust
pub fn build_authorize_url(
    application_id: &str,
    redirect_uri: &str,
    state: &str,
) -> String {
    let mut url = url::Url::parse("https://discord.com/api/oauth2/authorize")
        .expect("valid base URL");
    url.query_pairs_mut()
        .append_pair("client_id", application_id)
        .append_pair("response_type", "code")
        .append_pair("scope", "webhook.incoming")
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("state", state);
    url.to_string()
}
```

- [ ] **Step 4.2: Token exchange**

Append to `oauth.rs`:

```rust
pub async fn exchange_code(
    token_endpoint: &str,  // parameterised so tests can point at a mock
    client_id: &str,
    client_secret: &str,
    code: &str,
    redirect_uri: &str,
) -> Result<TokenResponse, OAuthError> {
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
    ];
    let resp = reqwest::Client::new()
        .post(token_endpoint)
        .basic_auth(client_id, Some(client_secret))
        .form(&form)
        .send()
        .await?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(OAuthError::NonSuccess {
            status: status.as_u16(),
            body,
        });
    }
    Ok(resp.json::<TokenResponse>().await?)
}

pub const DISCORD_TOKEN_ENDPOINT: &str = "https://discord.com/api/oauth2/token";
```

**Note**: `token_endpoint` is a parameter (not a const) so tests can inject a wiremock URL. Production callers pass `DISCORD_TOKEN_ENDPOINT`.

- [ ] **Step 4.3: Tests for authorize URL**

Add to `tests/tpp_oauth_test.rs`:

```rust
#[test]
fn authorize_url_contains_expected_params() {
    use wisp::platform::discord::oauth::build_authorize_url;
    let url = build_authorize_url("12345", "https://wisp.example.com/cb", "state123");
    assert!(url.starts_with("https://discord.com/api/oauth2/authorize?"));
    assert!(url.contains("client_id=12345"));
    assert!(url.contains("response_type=code"));
    assert!(url.contains("scope=webhook.incoming"));
    assert!(url.contains("state=state123"));
    assert!(url.contains("redirect_uri=https%3A%2F%2Fwisp.example.com%2Fcb"));
}
```

- [ ] **Step 4.4: Test for exchange_code (wiremock)**

```rust
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wisp::platform::discord::oauth::exchange_code;

#[tokio::test]
async fn exchange_code_happy_path() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .and(body_string_contains("grant_type=authorization_code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "token_type": "Bearer",
            "access_token": "at_xxx",
            "scope": "webhook.incoming",
            "expires_in": 604800,
            "refresh_token": "rt_xxx",
            "webhook": {
                "id": "wh123",
                "token": "wt456",
                "channel_id": "ch789",
                "guild_id": "g000",
                "name": "#general",
                "url": "https://discord.com/api/webhooks/wh123/wt456"
            }
        })))
        .mount(&server)
        .await;

    let endpoint = format!("{}/oauth2/token", server.uri());
    let r = exchange_code(&endpoint, "cid", "csecret", "code-xyz", "https://r")
        .await
        .unwrap();
    assert_eq!(r.webhook.id, "wh123");
    assert_eq!(r.webhook.token, "wt456");
    assert_eq!(r.webhook.channel_id, "ch789");
    assert_eq!(r.webhook.guild_id.as_deref(), Some("g000"));
}

#[tokio::test]
async fn exchange_code_non_success() {
    use wisp::platform::discord::oauth::OAuthError;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(ResponseTemplate::new(400).set_body_string("invalid_grant"))
        .mount(&server)
        .await;

    let endpoint = format!("{}/oauth2/token", server.uri());
    let err = exchange_code(&endpoint, "cid", "csecret", "bad", "https://r")
        .await
        .unwrap_err();
    match err {
        OAuthError::NonSuccess { status, .. } => assert_eq!(status, 400),
        _ => panic!("expected NonSuccess"),
    }
}
```

- [ ] **Step 4.5: Run tests + commit**

`cargo test --test tpp_oauth_test`

```
feat(discord-oauth): implement authorize URL + token exchange
```

---

## Task 5: Refactor `handle_setup`

**Files:**
- Modify: `src/tpp_poc.rs`
- Modify: `tests/tpp_poc_test.rs`

- [ ] **Step 5.1: Remove `PocState::webhooks`**

`PocState` is no longer needed for runtime data (it's in DB now). Delete the `webhooks` field and the `Default`/`new` impls that create it. Keep `PocState` as a unit struct if we need to thread runtime flags through later, or **delete it entirely** and wire repo directly. **Choose: delete `PocState`** — simpler.

- [ ] **Step 5.2: New `handle_setup` signature**

```rust
use crate::config::DiscordConfig;
use crate::platform::discord::oauth;

pub async fn handle_setup(
    cfg: &DiscordConfig,
    interaction: &Value,
) -> InteractionResponse {
    tracing::info!(
        event = "tpp_poc.setup",
        payload = %serde_json::to_string(interaction).unwrap_or_default(),
    );

    let Some(user_id) = extract_user_id(interaction) else {
        return ephemeral("⚠️ 無法取得 user id");
    };

    let state = oauth::generate_state(&user_id, &cfg.state_secret);
    let url = oauth::build_authorize_url(
        &cfg.application_id,
        &cfg.oauth_redirect_uri,
        &state,
    );

    ephemeral(format!(
        "點此授權 Wisp 建立 webhook：{url}\n（連結 10 分鐘後失效）"
    ))
}
```

- [ ] **Step 5.3: Update tests**

In `tests/tpp_poc_test.rs`, replace the `handle_setup` tests. Need to build a fake `DiscordConfig`:

```rust
fn fake_config() -> wisp::config::DiscordConfig {
    wisp::config::DiscordConfig {
        application_id: "12345".into(),
        public_key: "p".into(),
        bot_token: "b".into(),
        webhook_url: "w".into(),
        client_secret: "cs".into(),
        oauth_redirect_uri: "https://wisp.example.com/discord/oauth/callback".into(),
        state_secret: "ss".into(),
    }
}

#[tokio::test]
async fn setup_returns_authorize_url() {
    let cfg = fake_config();
    let interaction = serde_json::json!({
        "member": { "user": { "id": "329579602429214721" } },
        "data": { "name": "tpp-setup" },
    });
    let resp = wisp::tpp_poc::handle_setup(&cfg, &interaction).await;
    let content = resp.data.unwrap().content.unwrap();
    assert!(content.contains("https://discord.com/api/oauth2/authorize"));
    assert!(content.contains("client_id=12345"));
    assert!(content.contains("scope=webhook.incoming"));
}

#[tokio::test]
async fn setup_missing_user_id() {
    let cfg = fake_config();
    let interaction = serde_json::json!({ "data": { "name": "tpp-setup" } });
    let resp = wisp::tpp_poc::handle_setup(&cfg, &interaction).await;
    assert!(resp.data.unwrap().content.unwrap().contains("無法取得 user id"));
}
```

- [ ] **Step 5.4: Commit**

`cargo test --test tpp_poc_test` expect compile break for `handle_ping` tests — **ignore for now**, fixed in Task 6.

**Do not commit yet.** Commit with Task 6.

---

## Task 6: Refactor `handle_ping` to use `TppWebhookStore`

**Files:**
- Modify: `src/tpp_poc.rs`
- Modify: `tests/tpp_poc_test.rs`

- [ ] **Step 6.1: New signature**

```rust
use crate::db::tpp_webhooks::TppWebhookStore;

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

    // Same body + POST logic as Stage 1
    // ...
}
```

Keep the existing body-building + reqwest POST block (lines 103–139 of Stage 1 `handle_ping`) unchanged.

- [ ] **Step 6.2: In-memory fake store for tests**

In `tests/tpp_poc_test.rs`:

```rust
use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::RwLock;
use wisp::db::tpp_webhooks::{StoredWebhook, TppWebhookStore};

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
```

- [ ] **Step 6.3: Rewrite `handle_ping` tests**

```rust
#[tokio::test]
async fn ping_not_registered() {
    let store = FakeStore::default();
    let interaction = serde_json::json!({
        "member": { "user": { "id": "uid" } },
        "data": { "name": "tpp-ping" },
    });
    let resp = wisp::tpp_poc::handle_ping(&store, &interaction).await;
    assert!(resp.data.unwrap().content.unwrap().contains("尚未授權"));
}

#[tokio::test]
async fn ping_sends_webhook() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/webhooks/wh123/wt456"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let store = FakeStore::default();
    store.upsert(
        "uid",
        "wh123",
        "wt456",
        "ch",
        None,
        None,
    ).await.unwrap();

    // Override base URL by pointing webhook_id/token at our mock:
    // the handler builds https://discord.com/api/webhooks/{id}/{token}, but for
    // testing we'd need to allow URL override. Simplest: accept that this test
    // hits the real Discord URL pattern — split into two asserts:
    // (1) registered state returns "Sent" only with override; OR
    // (2) skip this path test (ping_sends_webhook removed) and rely on
    //     manual Experiment A.
    // Choose (2): remove this test body to avoid over-engineering. Keep the
    // test name as `#[ignore]` so intent is recorded.
}
```

**Decision**: remove `ping_sends_webhook` — testing the real-URL path needs handler-level URL override plumbing that's POC over-engineering. Stage 1 test only had it because the URL was user-controlled. Manual Experiment A covers this.

- [ ] **Step 6.4: Commit Tasks 5 + 6 together**

`cargo test --test tpp_poc_test --test tpp_oauth_test` — expect all pass.

```
refactor(tpp_poc): handle_setup returns authorize URL; handle_ping reads from DB store
```

---

## Task 7: Callback route + `DiscordState` extension

**Files:**
- Modify: `src/platform/discord/handler.rs`

- [ ] **Step 7.1: Extend `DiscordState`**

```rust
pub struct DiscordState {
    pub public_key_hex: String,
    pub application_id: String,
    pub bot_token: String,
    pub assistant: Arc<Assistant>,
    pub users: Arc<UserService>,
    pub allowed_channels: Arc<AllowedChannels>,
    // replaced:
    pub tpp_webhooks: Arc<dyn crate::db::tpp_webhooks::TppWebhookStore>,
    pub oauth_config: TppOAuthConfig,
}

#[derive(Clone)]
pub struct TppOAuthConfig {
    pub application_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub state_secret: String,
}
```

(Yes, `application_id` is duplicated with the outer field — keep the local copy for cleanliness; the outer field stays because other code paths reference it.)

- [ ] **Step 7.2: Add `/oauth/callback` route**

In the `routes()` function, add:

```rust
pub fn routes(state: Arc<DiscordState>) -> Router {
    Router::new()
        .route("/interactions", post(interactions_handler))
        .route("/oauth/callback", get(oauth_callback))
        .with_state(state)
}
```

- [ ] **Step 7.3: Implement `oauth_callback`**

```rust
use axum::extract::{Query, State};
use axum::response::{Html, IntoResponse};
use axum::http::StatusCode;
use serde::Deserialize;

#[derive(Deserialize)]
struct OAuthCallbackParams {
    code: String,
    state: String,
}

async fn oauth_callback(
    State(state): State<Arc<DiscordState>>,
    Query(params): Query<OAuthCallbackParams>,
) -> impl IntoResponse {
    let user_id = match crate::platform::discord::oauth::verify_state(
        &params.state,
        &state.oauth_config.state_secret,
    ) {
        Ok(uid) => uid,
        Err(e) => {
            tracing::warn!(event = "tpp_poc.oauth.state.invalid", error = %e);
            return (StatusCode::BAD_REQUEST, "invalid state").into_response();
        }
    };

    let endpoint = crate::platform::discord::oauth::DISCORD_TOKEN_ENDPOINT;
    let token_response = match crate::platform::discord::oauth::exchange_code(
        endpoint,
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
```

- [ ] **Step 7.4: Update dispatcher in `interactions_handler`**

Remove `state.poc` usage. Replace the POC command dispatch:

```rust
match command_name {
    "tpp-setup" => {
        let cfg = &state.oauth_config_as_discord_config(); // helper — or pass full DiscordConfig
        return Json(crate::tpp_poc::handle_setup(cfg, &interaction).await)
            .into_response();
    }
    "tpp-ping" => {
        return Json(
            crate::tpp_poc::handle_ping(state.tpp_webhooks.as_ref(), &interaction).await,
        )
        .into_response();
    }
    _ => {}
}
```

**Pragmatic**: `handle_setup` wants a `DiscordConfig`, but `DiscordState` holds the de-structured fields. Two options:
- (a) pass `&state.oauth_config` (`&TppOAuthConfig`) and change `handle_setup` to take `&TppOAuthConfig` (cleaner — OAuth-only config)
- (b) reconstruct a `DiscordConfig` locally (ugly)

Choose **(a)**: rename `handle_setup`'s param to `&TppOAuthConfig`. Revise Task 5's signature accordingly when implementing.

- [ ] **Step 7.5: Build**

`cargo build` — expect fail at main.rs (Task 8 fixes). Do not commit yet.

---

## Task 8: Wire into `main.rs`

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 8.1: Construct repo + config**

Inside `if let Some(ref discord_config) = config.discord { ... }`:

```rust
use wisp::db::tpp_webhooks::TppWebhookRepo;
use wisp::platform::discord::handler::TppOAuthConfig;

let tpp_webhook_repo: Arc<dyn wisp::db::tpp_webhooks::TppWebhookStore> =
    Arc::new(TppWebhookRepo::new(pool.clone()));

let oauth_cfg = TppOAuthConfig {
    application_id: discord_config.application_id.clone(),
    client_secret: discord_config.client_secret.clone(),
    redirect_uri: discord_config.oauth_redirect_uri.clone(),
    state_secret: discord_config.state_secret.clone(),
};

let discord_state = Arc::new(DiscordState {
    public_key_hex: discord_config.public_key.clone(),
    application_id: discord_config.application_id.clone(),
    bot_token: discord_config.bot_token.clone(),
    assistant: assistant.clone(),
    users: users.clone(),
    allowed_channels: allowed_channels.clone(),
    tpp_webhooks: tpp_webhook_repo,
    oauth_config: oauth_cfg,
});
```

- [ ] **Step 8.2: Remove `PocState::new()` usage**

Drop the `poc` field — no longer exists on `DiscordState`.

- [ ] **Step 8.3: Remove unused `use wisp::tpp_poc::PocState;`**

- [ ] **Step 8.4: Build + test**

```bash
cargo build
cargo test
```

Expected: clean build; all `tpp_poc_test` / `tpp_oauth_test` / `interaction_test` / `discord_verify_test` / `discord_webhook_test` / `config_test` pass. Pre-existing broken `assistant_test` / `llm_claude_test` remain broken (T-004/T-005).

- [ ] **Step 8.5: Commit Tasks 7 + 8 together**

```
feat(discord): add /discord/oauth/callback route; wire TppWebhookStore into DiscordState
```

---

## Task 9: Discord Dev Portal OAuth2 setup + production env

**Not code — manual.**

- [ ] **Step 9.1: Redirect URI**

Discord Dev Portal → Wisp app → **OAuth2** tab → **Redirects** → Add: `https://wisp.miao-bao.cc/discord/oauth/callback` → Save

- [ ] **Step 9.2: Copy Client Secret**

OAuth2 tab → Client Secret → Copy (or regenerate). Store securely.

- [ ] **Step 9.3: Generate `TPP_STATE_SECRET`**

```bash
openssl rand -hex 32
```

- [ ] **Step 9.4: Update production `.env`**

```bash
ssh wisp 'cat >> /opt/wisp/.env <<EOF

DISCORD_CLIENT_SECRET=<pasted>
DISCORD_OAUTH_REDIRECT_URI=https://wisp.miao-bao.cc/discord/oauth/callback
TPP_STATE_SECRET=<pasted>
EOF'
```

- [ ] **Step 9.5: Restart (to pick up new env on existing image, before the Stage 2 deploy)**

```bash
ssh wisp 'cd /opt/wisp && docker compose -f docker-compose.prod.yml up -d wisp'
```

(Once the Stage 2 code is merged + deployed, these envs are required; `Config::from_env` will silently disable Discord if missing. Set them **before** merging.)

---

## Task 10: Re-register `/tpp-setup` with new schema

**Not code — manual.**

- [ ] **Step 10.1: Create helper script**

`scripts/register-tpp-commands.sh`:

```bash
#!/usr/bin/env bash
# Re-register /tpp-setup (removes the `url` option) and /tpp-ping (unchanged).
# Prereq: /tmp/wisp-creds/creds.env written by input-tpp-creds.sh

set -euo pipefail
test -f /tmp/wisp-creds/creds.env
set -a; source /tmp/wisp-creds/creds.env; set +a
: "${DISCORD_APPLICATION_ID:?}"; : "${DISCORD_BOT_TOKEN:?}"

URL="https://discord.com/api/v10/applications/$DISCORD_APPLICATION_ID/commands"
AUTH="Authorization: Bot $DISCORD_BOT_TOKEN"
CT="Content-Type: application/json"

# PUT overwrites the full command list atomically (safer than individual POSTs).
curl -sS -w "\nHTTP %{http_code}\n" -X PUT "$URL" -H "$AUTH" -H "$CT" -d '[
  {
    "name": "tpp-setup",
    "description": "Authorize Wisp to post to a Discord channel via webhook",
    "integration_types": [1],
    "contexts": [0, 1, 2]
  },
  {
    "name": "tpp-ping",
    "description": "Send a test button message to the registered webhook",
    "integration_types": [1],
    "contexts": [0, 1, 2]
  }
]'
```

- [ ] **Step 10.2: Run**

```bash
bash scripts/input-tpp-creds.sh  # prompts for creds
bash scripts/register-tpp-commands.sh
rm -f /tmp/wisp-creds/creds.env && rmdir /tmp/wisp-creds 2>/dev/null || true
```

Expect `HTTP 200` with a JSON array of two commands.

- [ ] **Step 10.3: Discord client verify**

In Discord, `/tpp-setup` autocomplete should no longer ask for `url`. Command is now zero-arg.

---

## Task 11: Deploy + Experiments A / A2 / A3

**Manual.**

- [ ] **Step 11.1: Push + wait for deploy**

Ensure Tasks 1–8 are committed and merged. CI/CD builds + deploys. Verify:

```bash
curl https://wisp.miao-bao.cc/version
```

Check the returned `commit` is the expected Stage 2 merge commit. Confirm Discord is re-enabled (new envs loaded).

- [ ] **Step 11.2: Tail logs**

```bash
ssh wisp 'cd /opt/wisp && docker compose -f docker-compose.prod.yml logs -f wisp 2>&1 \
  | grep -E "tpp_poc"'
```

Keep this running throughout all experiments.

- [ ] **Step 11.3: Experiment A (happy path, full Manage Webhooks)**

Follow spec §觀察與實驗 Experiment A. Record:
- Authorize page screenshot (Q6 / Q9)
- `tpp_poc.oauth.callback` log line (Q7 — webhook object shape)
- `tpp_poc.ping.send.done` status (Q2′ infrastructure)
- Channel appearance: **does the button render?** (Q2′ pass/fail)
- Click the button; observe `tpp_poc.click` (Q3)
- Click payload fields (Q4): record `application_id`, `message.webhook_id`, `message.id`, `channel_id`, `user.id` / `member.user.id`, `data.custom_id`, `authorizing_integration_owners`

- [ ] **Step 11.4: Experiment A2 (Q8b — channel override only)**

Setup a test guild where your user has no role-wide Manage Webhooks but one channel has a channel-override grant. Run `/tpp-setup`, open authorize link, **screenshot channel selector**. Attempt to complete authorization — if successful, run `/tpp-ping` and observe. Record observations.

- [ ] **Step 11.5: Experiment A3 (Q8c — no permission)**

A guild where the user has zero Manage Webhooks. Run `/tpp-setup`, open authorize link, screenshot what Discord shows. Do not complete.

- [ ] **Step 11.6: Write results**

Append `## Stage 2 結果` to `docs/feature-request/webhook-interaction-bridge-poc.md`. Structure (mirrors Stage 1 result format):

```markdown
## Stage 2 結果

> 實驗日期：<YYYY-MM-DD>｜HEAD：<commit>

### Q2′ — App-owned webhook 能否 render components？
<✅/❌ + evidence>

### Q3 — Click 是否 route 到 /discord/interactions？
<✅/❌ + log excerpt>

### Q4 — Click payload 欄位
<field table>

### Q6 / Q7 — OAuth UX + webhook object 欄位
<observations>

### Q8a/b/c — 權限梯度的授權頁呈現
<screenshots + notes>

### Q9 — Channel selector 是否只列 guild channel
<yes/no + evidence>

### 結論
<Stage 3 go/no-go + any pivots>
```

Commit:

```
docs(poc): record Stage 2 experiment results
```

---

## Out-of-scope for Stage 2 (confirm with spec)

- Multi-round TPP game loop (`StoryEngine`, `GameRegistry`)
- Multi-webhook-per-user management UX
- Webhook revocation / re-binding
- Custom `/tpp-ping` messages (fixed single button is enough)
- Error UX polish (e.g. "channel was deleted" recovery)
- `webhook_token` encryption at rest
- Q10 execution-after-permission-loss testing

---

## Acceptance checklist (Stage 2 is "done" when all true)

- [ ] `migrations/005_tpp_webhooks.sql` applied on production
- [ ] `TppWebhookStore` trait + `TppWebhookRepo` concrete impl + unit tests
- [ ] `DiscordConfig` extended; `.env.example` updated
- [ ] `oauth.rs` with state HMAC + authorize URL + token exchange + unit tests (incl. wiremock)
- [ ] `handle_setup` returns authorize URL; `handle_ping` reads from store
- [ ] `/discord/oauth/callback` route registered and wired
- [ ] Production `.env` has 3 new vars; Discord enabled after restart
- [ ] `/tpp-setup` re-registered without `url` option
- [ ] Experiment A run; Q2′ + Q3 + Q4 have definitive results
- [ ] Experiment A2 + A3 observations recorded (or explicit skip rationale)
- [ ] Results appended to `docs/feature-request/webhook-interaction-bridge-poc.md`
- [ ] Stage 3 go/no-go decision documented
